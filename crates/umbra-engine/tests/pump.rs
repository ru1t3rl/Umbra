//! AT-2 (`Engine::pump`, M5 WI-5): one pacing decision at a time, never more than one tick, and
//! the same per-tick results as `run`/`step`. Requirements: REQ-K-3, REQ-K-4, ADR 0024, ADR 0021
//! (`Pumped::Aborted`, ASM-145 a).

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]
#![expect(clippy::panic, reason = "tests fail by panicking")]

mod plugin_support;

use core::time::Duration;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};

use umbra_core::{
    BbValue, Behavior, BehaviorContext, BehaviorId, BehaviorInstance, BlackboardError, ComponentId,
    Ecef, EntityView, InstantiateError, Intent, IntentBuffer, KinematicState, MetersPerSecond,
    MotionModel, MotionModelId, Ownership, ParamValue, Position, Radians, Seconds, SimEvent,
    StateHash, Status, TerrainProvider, TickRate, Vec3,
};
use umbra_engine::builtins::standard_registry;
use umbra_engine::scenario::{RunOptions, instantiate, load_scenario};
use umbra_engine::{
    ClockMode, Engine, EngineError, LoadedScenario, ManualWallClock, Pumped, RunUntil, SimTime,
    SpawnSpec, TimeScale, WallClock, instantiate_with_plugins, load_scenario_with_plugins,
};

const SLICE: Duration = Duration::from_millis(10);
/// Enough ticks for the first `Fire` of `patrol_and_engage` (blue-2, tick 1) and a few seconds of
/// every other scenario.
const TICKS: u64 = 150;

static WANDER: LazyLock<PathBuf> = LazyLock::new(plugin_support::wander);

fn scenarios() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/scenarios")
}

/// An engine for one of the three reference scenarios on a manual wall clock that advances 0.5 ms
/// per read, with the given clock mode.
struct Rig {
    engine: Engine,
    _loaded: Option<LoadedScenario>,
}

fn rig(name: &str, clock: ClockMode) -> Rig {
    let registry = standard_registry().expect("the standard registry builds");
    let path = scenarios().join(name).join("scenario.toml");
    let options = RunOptions {
        wall: Box::new(ManualWallClock::with_auto_advance(Duration::from_micros(
            500,
        ))),
        seed: None,
        clock: Some(clock),
    };
    if name == "plugin_wander" {
        let loaded = load_scenario_with_plugins(&registry, &path, std::slice::from_ref(&*WANDER))
            .expect("load plugin_wander");
        let engine = instantiate_with_plugins(&loaded, options)
            .expect("instantiate plugin_wander")
            .engine;
        Rig {
            engine,
            _loaded: Some(loaded),
        }
    } else {
        let scenario = load_scenario(&registry, &path).expect("load the scenario");
        let engine = instantiate(&registry, &scenario, options)
            .expect("instantiate")
            .engine;
        Rig {
            engine,
            _loaded: None,
        }
    }
}

/// What one tick looked like from outside.
#[derive(Debug, PartialEq)]
struct Tick {
    time: SimTime,
    hash: StateHash,
    events: Vec<SimEvent>,
}

fn run_driven(name: &str, clock: ClockMode) -> Vec<Tick> {
    let mut rig = rig(name, clock);
    let mut out = Vec::new();
    let record = |v: &umbra_engine::TickView<'_>| {
        out.push(Tick {
            time: v.report.time,
            hash: v.state_hash(),
            events: v.report.events.clone(),
        });
        ControlFlow::Continue(())
    };
    if clock == ClockMode::Stepped {
        rig.engine.step(TICKS, record).expect("step");
    } else {
        rig.engine.run(RunUntil::Ticks(TICKS), record).expect("run");
    }
    out
}

/// Pump-driven ticks, and how many `Waited` answers it took.
fn pump_driven(name: &str, clock: ClockMode) -> (Vec<Tick>, u64) {
    let mut rig = rig(name, clock);
    let engine = &mut rig.engine;
    if clock == ClockMode::Stepped {
        engine.request_steps(TICKS);
    }
    let (mut out, mut waited, mut calls) = (Vec::new(), 0_u64, 0_u64);
    while engine.now().tick() < TICKS {
        calls += 1;
        assert!(calls < 1_000_000, "pump makes no progress");
        match engine.pump(SLICE).expect("pump") {
            Pumped::Ticked(report) => out.push(Tick {
                time: report.time,
                hash: engine.state_hash(),
                events: report.events,
            }),
            Pumped::Waited => waited += 1,
            other => panic!("{name} {clock:?}: unexpected {other:?}"),
        }
    }
    (out, waited)
}

fn modes() -> [ClockMode; 4] {
    [
        ClockMode::AsFastAsPossible,
        ClockMode::RealTime,
        ClockMode::Scaled(TimeScale::new(4.0).expect("valid scale")),
        ClockMode::Stepped,
    ]
}

/// REQ-K-3, REQ-K-4: driving by `pump` gives, tick by tick, the time, state hash and events that
/// `run`/`step` give, for each reference scenario (`plugin_wander` with the real `example-wander`
/// library) and each clock mode.
#[test]
fn pump_driven_equals_run_driven_per_tick() {
    for name in ["walking_skeleton", "patrol_and_engage", "plugin_wander"] {
        for clock in modes() {
            let want = run_driven(name, clock);
            let (got, waited) = pump_driven(name, clock);
            assert_eq!(want.len() as u64, TICKS, "{name} {clock:?}: run length");
            assert_eq!(got, want, "{name} {clock:?}");
            if clock == ClockMode::RealTime {
                assert!(waited > 0, "{name}: real time must have waited at times");
            }
        }
    }
}

/// The equivalence test must be able to see events: `patrol_and_engage` fires in its first ticks.
#[test]
fn reference_run_contains_events() {
    let ticks = run_driven("patrol_and_engage", ClockMode::AsFastAsPossible);
    assert!(
        ticks
            .iter()
            .any(|t| t.events.iter().any(|e| matches!(e, SimEvent::Fire { .. })))
    );
}

/// ADR 0024: a pump call runs at most one tick, in every mode, also when far behind schedule.
#[test]
fn pump_ticks_at_most_once() {
    for clock in [ClockMode::AsFastAsPossible, ClockMode::Stepped] {
        let mut engine = rig("walking_skeleton", clock).engine;
        engine.request_steps(5);
        for expected in 1..=3_u64 {
            let pumped = engine.pump(SLICE).expect("pump");
            assert!(matches!(pumped, Pumped::Ticked(_)), "{clock:?}: {pumped:?}");
            assert_eq!(engine.now().tick(), expected, "{clock:?}");
        }
    }
    // Real time, ten seconds behind schedule: still one tick per call.
    let wall = ManualWallClock::new();
    let registry = standard_registry().expect("registry");
    let path = scenarios().join("walking_skeleton/scenario.toml");
    let scenario = load_scenario(&registry, &path).expect("load");
    let options = RunOptions {
        wall: Box::new(wall.clone()),
        seed: None,
        clock: Some(ClockMode::RealTime),
    };
    let mut engine = instantiate(&registry, &scenario, options)
        .expect("instantiate")
        .engine;
    assert!(matches!(engine.pump(SLICE), Ok(Pumped::Ticked(_))));
    wall.advance(Duration::from_secs(10));
    for expected in 2..=4_u64 {
        assert!(matches!(engine.pump(SLICE), Ok(Pumped::Ticked(_))));
        assert_eq!(engine.now().tick(), expected);
    }
}

/// The deadline arithmetic rounds to the nanosecond.
fn assert_near(got: Duration, want: Duration) {
    assert!(
        got.abs_diff(want) <= Duration::from_nanos(2),
        "{got:?} vs {want:?}"
    );
}

/// ADR 0024: `Waited` never blocks longer than `max_wait`, nor past the next tick's deadline.
#[test]
fn pump_waits_at_most_max_wait() {
    let wall = ManualWallClock::new();
    let registry = standard_registry().expect("registry");
    let path = scenarios().join("walking_skeleton/scenario.toml");
    let scenario = load_scenario(&registry, &path).expect("load");
    let options = RunOptions {
        wall: Box::new(wall.clone()),
        seed: None,
        clock: Some(ClockMode::RealTime),
    };
    let mut engine = instantiate(&registry, &scenario, options)
        .expect("instantiate")
        .engine;
    // Tick 0 is due at once; the next one 1/30 s later.
    assert!(matches!(engine.pump(SLICE), Ok(Pumped::Ticked(_))));
    let period = Duration::from_secs(1) / 30;
    for slice in 1..=3_u32 {
        assert_eq!(engine.pump(SLICE).expect("pump"), Pumped::Waited);
        assert_eq!(wall.now(), SLICE * slice, "waits one slice");
    }
    assert_eq!(engine.now().tick(), 1, "no tick while waiting");
    // The fourth slice would overshoot the deadline: the wait stops at the deadline.
    assert_eq!(engine.pump(SLICE).expect("pump"), Pumped::Waited);
    assert_near(wall.now(), period);
    assert!(matches!(engine.pump(SLICE), Ok(Pumped::Ticked(_))));
    // A zero wait does not move the clock; a long one stops at the deadline, not after a second.
    assert_eq!(engine.pump(Duration::ZERO).expect("pump"), Pumped::Waited);
    assert_near(wall.now(), period);
    assert_eq!(
        engine.pump(Duration::from_secs(1)).expect("pump"),
        Pumped::Waited
    );
    assert_near(wall.now(), period * 2);
}

/// ADR 0024: paused, or stepped without pending steps, the answer is `Idle` and nothing runs.
#[test]
fn pump_idle_when_paused_or_without_steps() {
    let mut engine = rig("walking_skeleton", ClockMode::AsFastAsPossible).engine;
    engine.pause();
    assert_eq!(engine.pump(SLICE).expect("pump"), Pumped::Idle);
    assert_eq!(engine.now().tick(), 0);
    engine.resume();
    assert!(matches!(engine.pump(SLICE), Ok(Pumped::Ticked(_))));

    let mut engine = rig("walking_skeleton", ClockMode::Stepped).engine;
    assert_eq!(engine.pump(SLICE).expect("pump"), Pumped::Idle);
    engine.request_steps(2);
    assert!(matches!(engine.pump(SLICE), Ok(Pumped::Ticked(_))));
    assert!(matches!(engine.pump(SLICE), Ok(Pumped::Ticked(_))));
    assert_eq!(engine.pump(SLICE).expect("pump"), Pumped::Idle);
    // Paused with steps pending: still idle, the steps stay.
    engine.request_steps(3);
    engine.pause();
    assert_eq!(engine.pump(SLICE).expect("pump"), Pumped::Idle);
    assert_eq!(engine.pending_steps(), 3);
}

/// `pending_steps`, `cancel_steps`, `is_paused` and `clock_mode` read and drive the clock.
#[test]
fn pending_steps_accessor() {
    let mut engine = rig("walking_skeleton", ClockMode::Stepped).engine;
    assert_eq!(engine.pending_steps(), 0);
    engine.request_steps(3);
    engine.request_steps(2);
    assert_eq!(engine.pending_steps(), 5);
    assert!(matches!(engine.pump(SLICE), Ok(Pumped::Ticked(_))));
    assert_eq!(engine.pending_steps(), 4);
    engine.cancel_steps();
    assert_eq!(engine.pending_steps(), 0);
    assert_eq!(engine.clock_mode(), ClockMode::Stepped);
    assert!(!engine.is_paused());
    engine.pause();
    assert!(engine.is_paused());
    engine.resume();
    assert!(!engine.is_paused());
}

/// `Engine::remove` drops an entity between ticks; ids are not reused.
#[test]
fn engine_remove_drops_the_entity_and_unknown_is_an_error() {
    let mut engine = rig("walking_skeleton", ClockMode::AsFastAsPossible).engine;
    let ids: Vec<_> = engine.snapshot().entities.iter().map(|e| e.id).collect();
    assert_eq!(ids.len(), 3);
    engine.remove(ids[1]).expect("remove");
    let left: Vec<_> = engine.snapshot().entities.iter().map(|e| e.id).collect();
    assert_eq!(left, vec![ids[0], ids[2]]);
    assert!(matches!(engine.remove(ids[1]), Err(EngineError::Kernel(_))));
}

// --- Pumped::Aborted ---------------------------------------------------------------------------

struct Still(MotionModelId);

impl MotionModel for Still {
    fn id(&self) -> &MotionModelId {
        &self.0
    }
    fn step(
        &self,
        state: &KinematicState,
        _: &[Intent],
        _: Seconds,
        _: &dyn TerrainProvider,
    ) -> KinematicState {
        *state
    }
}

/// A behavior that emits one `Custom` intent per tick.
struct Ping(BehaviorId);

struct PingInstance;

impl BehaviorInstance for PingInstance {
    fn tick(
        &mut self,
        _: &mut BehaviorContext<'_>,
        _: &EntityView<'_>,
        out: &mut IntentBuffer<'_>,
    ) -> Status {
        out.push(Intent::Custom {
            kind: ComponentId::new("test.custom.ping").expect("valid id"),
            payload: ParamValue::from(1),
        });
        Status::Running
    }
    fn halt(&mut self) {}
    fn output(&self, _: &str) -> Option<BbValue> {
        None
    }
    fn set_input(&mut self, _: &str, _: Option<BbValue>) -> Result<(), BlackboardError> {
        Ok(())
    }
}

impl Behavior for Ping {
    fn id(&self) -> &BehaviorId {
        &self.0
    }
    fn instantiate(&self, _: TickRate) -> Result<Box<dyn BehaviorInstance>, InstantiateError> {
        Ok(Box::new(PingInstance))
    }
}

/// ADR 0021, ASM-145 a, review M9: under `abort`, the pump call that ran the faulting tick returns
/// `Aborted` with that tick's report, events included, and the fault; the tick is not lost.
#[test]
fn pump_returns_aborted_with_the_faulting_tick_report_and_events() {
    let fixture = plugin_support::fixture(&[]);
    let scenario = plugin_support::fault_scenario("pump-abort", "abort", 2.0, &["direct"]);
    let registry = standard_registry().expect("registry");
    let loaded =
        load_scenario_with_plugins(&registry, &scenario, &[fixture]).expect("load the fault run");
    let options = RunOptions {
        wall: Box::new(ManualWallClock::new()),
        seed: None,
        clock: None,
    };
    let mut engine = instantiate_with_plugins(&loaded, options)
        .expect("instantiate")
        .engine;
    // A second entity, after the plugin one (ids order the events), emits a Custom event each tick.
    let spawn = SpawnSpec {
        position: Position::Ecef(Ecef(Vec3::new(6_378_137.0, 0.0, 0.0))),
        heading: Radians(0.0),
        speed: MetersPerSecond(0.0),
        ownership: Ownership::Owned,
        motion: Arc::new(Still(MotionModelId::new("test.model.still").expect("id"))),
        route: None,
    };
    let agent = umbra_engine::AgentInit {
        behavior: Some(Arc::new(Ping(
            BehaviorId::new("test.behavior.ping").expect("id"),
        ))),
        ..umbra_engine::AgentInit::default()
    };
    let pinger = engine.spawn_with(spawn, agent).expect("spawn the pinger");

    let pumped = engine.pump(SLICE).expect("pump");
    let Pumped::Aborted { report, fault } = pumped else {
        panic!("expected Aborted, got {pumped:?}");
    };
    assert_eq!(report.time.tick(), 1, "the faulting tick ran");
    assert_eq!(engine.now().tick(), 1);
    assert_eq!(
        report.events,
        vec![SimEvent::Custom {
            source: Some(pinger),
            kind: ComponentId::new("test.custom.ping").expect("id"),
            payload: ParamValue::from(1),
        }],
        "the events of the faulting tick are kept"
    );
    assert_eq!(fault.capability.as_str(), "test.behavior.panic_tick");
    // The fault was taken: the engine is not wedged and keeps pacing.
    assert!(engine.pump(SLICE).is_ok());
    assert_eq!(engine.now().tick(), 2);
}
