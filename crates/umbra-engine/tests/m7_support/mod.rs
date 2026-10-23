//! Shared helpers for the M7 engine acceptance tests (WI-1, step 4): the launcher behavior that
//! emits `Spawn` and `Despawn`, the `fixtures/m7` scenarios, recording and replay round trips.
//!
//! The launcher `test.behavior.launcher` takes `{ spawn_at: [tick, ..], template, per_tick,
//! despawn_at, ping }`: on its own tick number `n` (1 for its first tick) it emits `per_tick` copies of
//! `Spawn { template, position: its own position, heading: 0.25 rad }` when `n` is in `spawn_at`,
//! and `Despawn` (`despawn_count` times, default 1) when `n == despawn_at`; with `ping` it also emits a `Custom` intent every tick (a
//! kernel event to order the spawner's events against). Each test binary uses a subset.
#![expect(
    dead_code,
    reason = "each test binary uses a subset of the shared helpers"
)]
#![expect(clippy::expect_used, reason = "helpers build known-valid fixtures")]
#![expect(clippy::panic, reason = "helpers fail by panicking")]

use core::time::Duration;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use umbra_core::{
    BbValue, Behavior, BehaviorContext, BehaviorId, BehaviorInstance, BlackboardError, BuildError,
    ComponentId, ComponentInfo, EntityView, Factory, InstantiateError, Intent, IntentBuffer,
    ParamSchema, ParamValue, Provenance, Radians, Registry, StateHash, StateHashVersion, Status,
    TemplateId, TickRate, Version,
};
use umbra_engine::builtins::standard_registry;
use umbra_engine::scenario::RunOptions;
use umbra_engine::{
    ManualWallClock, RecordOptions, Recorder, ReplayError, ReplayOptions, ReplaySummary, RunUntil,
    WallClock, instantiate_with_plugins, load_scenario_with_plugins, replay,
};

pub const LAUNCHER: &str = "test.behavior.launcher";

struct Launcher {
    spawn_at: Vec<u64>,
    template: TemplateId,
    per_tick: u64,
    despawn_at: Option<u64>,
    despawn_count: u64,
    ping: bool,
    n: u64,
}

impl BehaviorInstance for Launcher {
    fn tick(
        &mut self,
        _: &mut BehaviorContext<'_>,
        entity: &EntityView<'_>,
        out: &mut IntentBuffer<'_>,
    ) -> Status {
        self.n += 1;
        if self.ping {
            out.push(Intent::Custom {
                kind: ComponentId::new(LAUNCHER).expect("id"),
                payload: ParamValue::Null,
            });
        }
        if self.spawn_at.contains(&self.n) {
            for _ in 0..self.per_tick {
                out.push(Intent::Spawn {
                    template: self.template.clone(),
                    position: entity.state.position,
                    heading: Radians(0.25),
                });
            }
        }
        if self.despawn_at == Some(self.n) {
            for _ in 0..self.despawn_count {
                out.push(Intent::Despawn);
            }
        }
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

struct LauncherBehavior {
    id: BehaviorId,
    spawn_at: Vec<u64>,
    template: TemplateId,
    per_tick: u64,
    despawn_at: Option<u64>,
    despawn_count: u64,
    ping: bool,
}

impl Behavior for LauncherBehavior {
    fn id(&self) -> &BehaviorId {
        &self.id
    }
    fn instantiate(&self, _: TickRate) -> Result<Box<dyn BehaviorInstance>, InstantiateError> {
        Ok(Box::new(Launcher {
            spawn_at: self.spawn_at.clone(),
            template: self.template.clone(),
            per_tick: self.per_tick,
            despawn_at: self.despawn_at,
            despawn_count: self.despawn_count,
            ping: self.ping,
            n: 0,
        }))
    }
}

struct LauncherFactory {
    id: ComponentId,
    schema: ParamSchema,
}

impl Factory<dyn Behavior> for LauncherFactory {
    fn id(&self) -> &ComponentId {
        &self.id
    }
    fn param_schema(&self) -> &ParamSchema {
        &self.schema
    }
    fn build(&self, params: &ParamValue) -> Result<Arc<dyn Behavior>, BuildError> {
        let spawn_at = params
            .get("spawn_at")
            .and_then(ParamValue::as_array)
            .map(|a| a.iter().filter_map(ParamValue::as_u64).collect())
            .unwrap_or_default();
        let template = params
            .get("template")
            .and_then(ParamValue::as_str)
            .unwrap_or("m7.child");
        Ok(Arc::new(LauncherBehavior {
            id: BehaviorId::new(LAUNCHER).map_err(|e| BuildError::new(e.to_string()))?,
            spawn_at,
            template: TemplateId::new(template).map_err(|e| BuildError::new(e.to_string()))?,
            per_tick: params
                .get("per_tick")
                .and_then(ParamValue::as_u64)
                .unwrap_or(1),
            despawn_at: params.get("despawn_at").and_then(ParamValue::as_u64),
            despawn_count: params
                .get("despawn_count")
                .and_then(ParamValue::as_u64)
                .unwrap_or(1),
            ping: params
                .get("ping")
                .and_then(ParamValue::as_bool)
                .unwrap_or(false),
        }))
    }
}

/// `registry` plus the launcher behavior.
pub fn with_launcher(mut registry: Registry) -> Registry {
    registry
        .register_behavior(
            Arc::new(LauncherFactory {
                id: ComponentId::new(LAUNCHER).expect("id"),
                schema: ParamSchema::from_json_str(
                    r#"{"type":"object","examples":[{}],"properties":{
                        "spawn_at":{"type":"array","items":{"type":"integer","minimum":1}},
                        "template":{"type":"string"},
                        "per_tick":{"type":"integer","minimum":1},
                        "despawn_at":{"type":"integer","minimum":1},
                        "despawn_count":{"type":"integer","minimum":1},
                        "ping":{"type":"boolean"}}}"#,
                )
                .expect("schema"),
            }),
            ComponentInfo::new("launcher", Provenance::Plugin, Version::new(0, 1, 0)),
        )
        .expect("register the launcher");
    registry
}

/// The standard registry plus the launcher.
pub fn launcher_registry() -> Registry {
    with_launcher(standard_registry().expect("the standard registry builds"))
}

/// `tests/fixtures/m7/<dir>/scenario.toml`.
pub fn fixture(dir: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/m7")
        .join(dir)
        .join("scenario.toml")
}

/// `examples/scenarios/<name>/scenario.toml`.
pub fn example(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/scenarios")
        .join(name)
        .join("scenario.toml")
}

/// A fresh path under the target tmp dir.
pub fn scratch(tag: &str) -> PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("m7");
    std::fs::create_dir_all(&dir).expect("scratch directory");
    dir.join(format!(
        "{}-{}-{tag}.umbrarec.jsonl",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ))
}

static PLUGIN_TEST_LOCK: Mutex<()> = Mutex::new(());

/// Serialises the tests of one binary that load real plugin libraries (rule m8).
pub fn plugin_lock() -> MutexGuard<'static, ()> {
    PLUGIN_TEST_LOCK
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

/// The golden final hashes of `scripts/gates.sh` (its `GOLDENS` list is the one source) by scenario.
pub fn goldens() -> BTreeMap<String, String> {
    let text = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/gates.sh"),
    )
    .expect("read gates.sh");
    text.lines()
        .map(|l| l.trim_start_matches("GOLDENS='"))
        .filter_map(|l| {
            let mut words = l.split_whitespace();
            Some((words.next()?.to_owned(), words.next()?.to_owned()))
        })
        .filter(|(_, hash)| hash.len() == 16 && hash.bytes().all(|b| b.is_ascii_hexdigit()))
        .collect()
}

pub fn run_options(seed: Option<u64>) -> RunOptions {
    run_options_on(
        Box::new(ManualWallClock::with_auto_advance(Duration::from_micros(
            500,
        ))),
        seed,
    )
}

pub fn run_options_on(wall: Box<dyn WallClock>, seed: Option<u64>) -> RunOptions {
    RunOptions {
        wall,
        seed,
        clock: None,
    }
}

/// A recorded run: the file and the per-tick state hashes the live run produced.
pub struct Recorded {
    pub file: PathBuf,
    pub hashes: Vec<StateHash>,
    pub entity_counts: Vec<usize>,
}

/// Runs `scenario` (with `plugins`) for `ticks` ticks (the scenario's duration when `None`) with a
/// recorder that hashes every tick, and finishes the recording.
pub fn record(
    registry: &Registry,
    scenario: &Path,
    plugins: &[PathBuf],
    ticks: Option<u64>,
    seed: Option<u64>,
) -> Recorded {
    let wall = Box::new(ManualWallClock::with_auto_advance(Duration::from_micros(
        500,
    )));
    record_on(registry, scenario, plugins, ticks, seed, wall)
}

/// [`record`] on the given wall clock (a real one for real-time runs).
pub fn record_on(
    registry: &Registry,
    scenario: &Path,
    plugins: &[PathBuf],
    ticks: Option<u64>,
    seed: Option<u64>,
    wall: Box<dyn WallClock>,
) -> Recorded {
    record_with(
        registry,
        scenario,
        plugins,
        ticks,
        seed,
        wall,
        StateHashVersion::V2,
    )
}

/// [`record_on`] with the state hash version of the recording (ADR 0055).
pub fn record_with(
    registry: &Registry,
    scenario: &Path,
    plugins: &[PathBuf],
    ticks: Option<u64>,
    seed: Option<u64>,
    wall: Box<dyn WallClock>,
    state_hash: StateHashVersion,
) -> Recorded {
    let loaded = load_scenario_with_plugins(registry, scenario, plugins)
        .unwrap_or_else(|e| panic!("load {}: {e}", scenario.display()));
    let run = instantiate_with_plugins(&loaded, run_options_on(wall, seed))
        .unwrap_or_else(|e| panic!("instantiate: {e}"));
    let mut engine = run.engine;
    let file = scratch("rec");
    let mut options = RecordOptions::new(&file);
    options.hash_every = Some(1);
    options.state_hash = state_hash;
    let recorder = Recorder::create(options, scenario, &loaded, seed)
        .unwrap_or_else(|e| panic!("recorder: {e}"));
    engine.attach_recorder(recorder);
    let (mut hashes, mut entity_counts) = (Vec::new(), Vec::new());
    engine
        .run(
            RunUntil::Ticks(ticks.unwrap_or(run.duration_ticks)),
            |view| {
                hashes.push(view.state_hash());
                entity_counts.push(view.snapshot().entities.len());
                core::ops::ControlFlow::Continue(())
            },
        )
        .unwrap_or_else(|e| panic!("run: {e}"));
    engine
        .finish_recording("completed")
        .unwrap_or_else(|e| panic!("finish: {e}"));
    Recorded {
        file,
        hashes,
        entity_counts,
    }
}

/// Replays `file` against `registry`; returns the per-tick hashes and the summary.
pub fn replay_file(
    registry: &Registry,
    file: &Path,
    plugins: &[PathBuf],
) -> Result<(Vec<StateHash>, ReplaySummary), ReplayError> {
    let mut options = ReplayOptions::default();
    options.plugin_paths = plugins.to_vec();
    let mut hashes = Vec::new();
    let summary = replay(registry, file, options, |view| {
        hashes.push(view.state_hash());
    })?;
    Ok((hashes, summary))
}

/// The index of the first tick at which the two hash lists differ, if any.
pub fn first_divergence(a: &[StateHash], b: &[StateHash]) -> Option<usize> {
    a.iter()
        .zip(b)
        .position(|(x, y)| x != y)
        .or_else(|| (a.len() != b.len()).then_some(a.len().min(b.len())))
}

/// Loads and instantiates `scenario` against `registry` on a manual wall clock.
pub fn start(registry: &Registry, scenario: &Path) -> umbra_engine::scenario::ScenarioRun {
    let loaded = load_scenario_with_plugins(registry, scenario, &[])
        .unwrap_or_else(|e| panic!("load {}: {e}", scenario.display()));
    instantiate_with_plugins(&loaded, run_options(None))
        .unwrap_or_else(|e| panic!("instantiate {}: {e}", scenario.display()))
}

/// One engine stepped a tick at a time, with what each tick reported.
pub struct Stepper {
    pub engine: umbra_engine::Engine,
    pub events: Vec<Vec<umbra_engine::SimEvent>>,
    pub hashes: Vec<StateHash>,
    pub counts: Vec<usize>,
}

impl Stepper {
    pub fn new(engine: umbra_engine::Engine) -> Self {
        Self {
            engine,
            events: Vec::new(),
            hashes: Vec::new(),
            counts: Vec::new(),
        }
    }

    pub fn tick(&mut self) {
        let (events, hashes, counts) = (&mut self.events, &mut self.hashes, &mut self.counts);
        self.engine
            .step(1, |view| {
                events.push(view.report.events.clone());
                hashes.push(view.state_hash());
                counts.push(view.snapshot().entities.len());
                core::ops::ControlFlow::Continue(())
            })
            .expect("step");
    }

    pub fn ticks(&mut self, n: u64) {
        for _ in 0..n {
            self.tick();
        }
    }
}

/// A stepper over `tests/fixtures/m7/<dir>` on the launcher registry.
pub fn stepper(dir: &str) -> Stepper {
    Stepper::new(start(&launcher_registry(), &fixture(dir)).engine)
}

/// Collects every WARN event (its message and fields, as one line) of the closure `f`
/// (the capture pattern of `net_warnings.rs`).
pub fn capture_warnings(f: impl FnOnce()) -> Vec<String> {
    use tracing::field::{Field, Visit};
    use tracing::span::{Attributes, Id, Record};
    use tracing::{Event, Level, Metadata, Subscriber};

    #[derive(Clone, Default)]
    struct Warnings(Arc<Mutex<Vec<String>>>);

    struct Line(Vec<String>);

    impl Visit for Line {
        fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
            if field.name() == "message" {
                self.0.insert(0, format!("{value:?}"));
            } else {
                self.0.push(format!("{}={value:?}", field.name()));
            }
        }
    }

    impl Subscriber for Warnings {
        fn enabled(&self, m: &Metadata<'_>) -> bool {
            *m.level() == Level::WARN
        }
        fn new_span(&self, _: &Attributes<'_>) -> Id {
            Id::from_u64(1)
        }
        fn record(&self, _: &Id, _: &Record<'_>) {}
        fn record_follows_from(&self, _: &Id, _: &Id) {}
        fn event(&self, e: &Event<'_>) {
            let mut line = Line(Vec::new());
            e.record(&mut line);
            self.0
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(line.0.join(" "));
        }
        fn enter(&self, _: &Id) {}
        fn exit(&self, _: &Id) {}
    }

    let warnings = Warnings::default();
    tracing::subscriber::with_default(warnings.clone(), f);
    let guard = warnings.0.lock().unwrap_or_else(PoisonError::into_inner);
    let got = guard.clone();
    drop(guard);
    got
}
