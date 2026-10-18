//! Shared helpers for the scenario-based `umbra-engine` tests (M2): the example scenario, the
//! built-in registry, instantiation with a manual wall clock, and per-tick traces.
//!
//! All engine access that depends on the callback signature of `Engine::run`/`Engine::step` goes
//! through `run_traced`, `step_traced` and `run_silent` below (the callback returns
//! `ControlFlow<()>`).
//!
//! Each test binary uses a subset of the helpers.
#![expect(
    dead_code,
    reason = "each test binary uses a subset of the shared helpers"
)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

use core::time::Duration;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};

use umbra_core::{Registry, StateHash};
use umbra_engine::builtins::builtin_registry;
use umbra_engine::scenario::{RunOptions, ScenarioRun, instantiate, load_scenario};
use umbra_engine::{
    ClockMode, Engine, EntitySnapshot, ManualWallClock, RunUntil, Snapshot, TickView,
};
use umbra_templates::Scenario;

/// The example scenario's tick count in the determinism runs (30 s at 30 Hz).
pub const TICKS: u64 = 900;
/// Pause and resume the run at this tick.
pub const PAUSE_AT: u64 = 100;

pub fn registry() -> Registry {
    builtin_registry().expect("the built-in registry builds")
}

/// `examples/scenarios/walking_skeleton`.
pub fn example_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/scenarios/walking_skeleton")
}

pub fn example_path() -> PathBuf {
    example_dir().join("scenario.toml")
}

pub fn fixture_path(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(rel)
}

pub fn load_example() -> Scenario {
    load_scenario(&registry(), &example_path()).unwrap_or_else(|e| panic!("example scenario: {e}"))
}

pub fn options(read_cost: Duration, seed: Option<u64>, clock: Option<ClockMode>) -> RunOptions {
    RunOptions {
        wall: Box::new(ManualWallClock::with_auto_advance(read_cost)),
        seed,
        clock,
    }
}

/// Instantiates a loaded scenario on a manual wall clock that advances `read_cost` per read, so
/// different costs give different wall-time scripts for the same simulation.
pub fn instantiate_with(
    scenario: &Scenario,
    read_cost: Duration,
    seed: Option<u64>,
    clock: Option<ClockMode>,
) -> ScenarioRun {
    instantiate(&registry(), scenario, options(read_cost, seed, clock))
        .unwrap_or_else(|e| panic!("instantiate: {e}"))
}

/// The example on a 0.5 ms-per-read manual wall clock with the file's seed and clock.
pub fn example_run() -> ScenarioRun {
    instantiate_with(&load_example(), Duration::from_micros(500), None, None)
}

pub fn example_engine(seed: Option<u64>, clock: Option<ClockMode>) -> Engine {
    instantiate_with(&load_example(), Duration::from_micros(500), seed, clock).engine
}

/// The per-tick record of one run: state hash and snapshot after each tick.
#[derive(Default)]
pub struct Trace {
    pub hashes: Vec<StateHash>,
    pub snapshots: Vec<Snapshot>,
}

impl Trace {
    pub fn record(&mut self, view: &TickView<'_>) {
        self.hashes.push(view.state_hash());
        self.snapshots.push(view.snapshot());
    }

    pub fn len(&self) -> u64 {
        u64::try_from(self.hashes.len()).expect("length fits u64")
    }

    /// The post-tick snapshot of the entity at `index` (id order) for every tick.
    pub fn entity(&self, index: usize) -> Vec<EntitySnapshot> {
        self.snapshots
            .iter()
            .map(|s| s.entities[index].clone())
            .collect()
    }
}

pub fn run_traced(engine: &mut Engine, until: RunUntil, trace: &mut Trace) {
    engine
        .run(until, |v| {
            trace.record(v);
            ControlFlow::Continue(())
        })
        .expect("run");
}

pub fn step_traced(engine: &mut Engine, n: u64, trace: &mut Trace) {
    engine
        .step(n, |v| {
            trace.record(v);
            ControlFlow::Continue(())
        })
        .expect("step");
}

pub fn run_silent(engine: &mut Engine, until: RunUntil) {
    engine
        .run(until, |_| ControlFlow::Continue(()))
        .expect("run");
}

/// Runs to `ticks` with `run`, optionally pausing and resuming at `PAUSE_AT`.
pub fn run_to(mut engine: Engine, ticks: u64, pause: bool) -> Trace {
    let mut trace = Trace::default();
    if pause {
        run_traced(&mut engine, RunUntil::Ticks(PAUSE_AT), &mut trace);
        engine.pause();
        run_traced(&mut engine, RunUntil::Ticks(ticks), &mut trace);
        assert_eq!(trace.len(), PAUSE_AT, "no tick may run while paused");
        engine.resume();
    }
    run_traced(&mut engine, RunUntil::Ticks(ticks), &mut trace);
    trace
}

/// The first difference between two entity snapshots, as `(field, left, right)`.
pub fn entity_diff(a: &EntitySnapshot, b: &EntitySnapshot) -> Option<(String, String, String)> {
    if a.id != b.id {
        return Some((
            "id (entity order differs)".into(),
            a.id.to_string(),
            b.id.to_string(),
        ));
    }
    let bits = |x: f64| format!("{x:?} (bits {:#018x})", x.to_bits());
    let (pa, pb) = (&a.state, &b.state);
    let fields = [
        ("position.x", pa.position.0.x, pb.position.0.x),
        ("position.y", pa.position.0.y, pb.position.0.y),
        ("position.z", pa.position.0.z, pb.position.0.z),
        ("velocity.x", pa.velocity.x, pb.velocity.x),
        ("velocity.y", pa.velocity.y, pb.velocity.y),
        ("velocity.z", pa.velocity.z, pb.velocity.z),
        ("orientation.w", pa.orientation.w, pb.orientation.w),
        ("orientation.x", pa.orientation.x, pb.orientation.x),
        ("orientation.y", pa.orientation.y, pb.orientation.y),
        ("orientation.z", pa.orientation.z, pb.orientation.z),
    ];
    for (name, x, y) in fields {
        if x.to_bits() != y.to_bits() {
            return Some((name.to_string(), bits(x), bits(y)));
        }
    }
    if a.route_next != b.route_next {
        return Some((
            "route_next".into(),
            format!("{:?}", a.route_next),
            format!("{:?}", b.route_next),
        ));
    }
    if a.ownership != b.ownership {
        return Some((
            "ownership".into(),
            format!("{:?}", a.ownership),
            format!("{:?}", b.ownership),
        ));
    }
    if a.motion_model != b.motion_model {
        return Some((
            "motion_model".into(),
            a.motion_model.to_string(),
            b.motion_model.to_string(),
        ));
    }
    None
}

/// Panics at the first divergent tick, naming the entity and field (REQ-K-4).
pub fn assert_same_trace(label: &str, got: &Trace, want: &Trace) {
    assert_eq!(
        got.hashes.len(),
        want.hashes.len(),
        "{label}: number of ticks"
    );
    for (i, (g, w)) in got.hashes.iter().zip(&want.hashes).enumerate() {
        if g == w {
            continue;
        }
        let (gs, ws) = (&got.snapshots[i], &want.snapshots[i]);
        let detail = if gs.entities.len() == ws.entities.len() {
            gs.entities
                .iter()
                .zip(&ws.entities)
                .find_map(|(a, b)| entity_diff(a, b).map(|d| (a.id, d)))
                .map_or_else(
                    || {
                        "snapshots equal: the difference is in RNG state or hash order (not in the snapshot)"
                            .to_string()
                    },
                    |(id, (field, a, b))| format!("entity {id}, field {field}: {a} vs {b}"),
                )
        } else {
            format!(
                "entity count {} vs {}",
                gs.entities.len(),
                ws.entities.len()
            )
        };
        panic!(
            "{label}: first divergent tick is {} (hash {g} vs {w}): {detail}",
            i + 1
        );
    }
}
