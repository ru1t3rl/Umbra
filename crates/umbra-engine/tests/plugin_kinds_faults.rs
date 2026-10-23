//! AT-16 engine rows (M7 WI-8): faults of plugin motion models and sensors under the scenario's
//! `on_plugin_fault` policy. Requirements: REQ-P-4, REQ-P-7, REQ-M-1, REQ-K-4, ADR 0021, ADR 0050,
//! ASM-614, ASM-615.
//!
//! The plugin is `umbra-test-plugin` as `test.plugin.kinds` (SDK-built; until WI-9 the transitional
//! feature `kinds_motion_sensor`, ASM-718): `test.motion.coast` moves an entity along its velocity,
//! `test.motion.nan` returns a NaN position (the host refuses it), `test.sensor.all` contacts every
//! candidate and `test.sensor.panic_sense` panics in `sense`. Under `abort` the run ends after the
//! faulting tick with `EngineError::PluginFault` (or `StopReason::PluginFault` on the engine handle),
//! and that tick's frame is still seen: the faulted motion model's entity held (position and
//! orientation kept, velocity zero, ASM-614), the faulted sensor's entity without contacts (ASM-615).
//! Under `disable_capability` the run goes to the end with one fault, the held entity stays held and
//! the healthy capabilities keep working.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod common;
mod handle_support;
mod kinds_support;
mod plugin_support;

use core::fmt::Write as _;
use core::ops::ControlFlow;
use core::time::Duration;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use common::options;
use kinds_support::write_dir;
use umbra_core::{EntityId, KinematicState, StateHash, Vec3};
use umbra_engine::builtins::standard_registry;
use umbra_engine::bus::{FrameCause, StopReason};
use umbra_engine::{
    Engine, EngineError, LoadedScenario, RunSummary, RunUntil, Snapshot, instantiate_with_plugins,
    load_scenario_with_plugins,
};

/// The kinds fixture as package `fixture` (no manifest).
static PLUGINS: LazyLock<PathBuf> =
    LazyLock::new(|| plugin_support::fixture(&["kinds_motion_sensor"]));

const PLUGIN: &str = "test.plugin.kinds";
const TICKS: u64 = 30;

/// One entity of a scene: name, motion model, sensor (if any).
type Member = (&'static str, &'static str, Option<&'static str>);

/// Writes a scene under `policy` (30 Hz, flat terrain, seed 7): one template and one entity per
/// member, 50 m apart, heading east at 5 m/s. The ground-truth model is the plugin's `coast`, so
/// every entity moves unless its model faulted.
fn scene(tag: &str, policy: &str, members: &[Member]) -> PathBuf {
    let mut scenario = format!(
        "schema_version = 1\n\n[scenario]\nname = \"{tag}\"\nseed = 7\ntick_rate_hz = 30\n\
         clock = \"afap\"\nduration_s = 10.0\ntemplate_paths = [\"templates\"]\n\
         on_plugin_fault = \"{policy}\"\n\n\
         [terrain]\nprovider = \"builtin.terrain.flat\"\nparams = {{ height_m = 0.0 }}\n"
    );
    let mut files = Vec::new();
    for (i, (name, motion, sensor)) in members.iter().enumerate() {
        let mut template = format!(
            "schema_version = 1\nid = \"test.entity.{name}\"\ndescription = \"generated\"\n\n\
             [motion]\nmodel = \"{motion}\"\nparams = {{}}\n"
        );
        if let Some(sensor) = sensor {
            write!(
                template,
                "\n[[sensors]]\nkind = \"{sensor}\"\nparams = {{}}\n"
            )
            .expect("write to a String");
        }
        files.push((format!("templates/{name}.toml"), template));
        let lon = 4.0 + 0.0007 * f64::from(u32::try_from(i).expect("small index"));
        write!(
            scenario,
            "\n[[entities]]\ntemplate = \"test.entity.{name}\"\nname = \"{name}\"\n\
             position = {{ lat_deg = 52.0, lon_deg = {lon:.7} }}\nheading_deg = 90.0\nspeed_mps = 5.0\n"
        )
        .expect("write to a String");
    }
    files.push(("scenario.toml".to_owned(), scenario));
    let refs: Vec<(&str, &str)> = files
        .iter()
        .map(|(p, t)| (p.as_str(), t.as_str()))
        .collect();
    write_dir(tag, &refs).join("scenario.toml")
}

fn load(path: &Path) -> LoadedScenario {
    load_scenario_with_plugins(
        &standard_registry().expect("the standard registry builds"),
        path,
        std::slice::from_ref(&*PLUGINS),
    )
    .unwrap_or_else(|e| panic!("load: {e}"))
}

/// What a run left: the engine, the snapshot before the first tick, every tick's snapshot and hash
/// seen by the callback, the result, and the entity ids by member name.
struct Run {
    engine: Engine,
    start: Snapshot,
    frames: Vec<(Snapshot, StateHash)>,
    result: Result<RunSummary, EngineError>,
    ids: Vec<(String, EntityId)>,
}

impl Run {
    fn id(&self, name: &str) -> EntityId {
        self.ids
            .iter()
            .find(|(n, _)| n == name)
            .map_or_else(|| panic!("no entity {name}"), |(_, id)| *id)
    }
}

fn state(snapshot: &Snapshot, id: EntityId) -> KinematicState {
    snapshot
        .entities
        .iter()
        .find(|e| e.id == id)
        .map(|e| e.state)
        .expect("the entity is in the snapshot")
}

fn contacts(snapshot: &Snapshot, id: EntityId) -> usize {
    snapshot
        .entities
        .iter()
        .find(|e| e.id == id)
        .map(|e| e.contacts.len())
        .expect("the entity is in the snapshot")
}

fn run(loaded: &LoadedScenario) -> Run {
    let started = instantiate_with_plugins(loaded, options(Duration::from_micros(500), None, None))
        .unwrap_or_else(|e| panic!("instantiate: {e}"));
    let ids = started
        .entities
        .iter()
        .map(|e| (e.name.clone().unwrap_or_default(), e.id))
        .collect();
    let mut engine = started.engine;
    let start = engine.snapshot();
    let mut frames = Vec::new();
    let result = engine.run(RunUntil::Ticks(TICKS), |view| {
        frames.push((view.snapshot(), view.state_hash()));
        ControlFlow::Continue(())
    });
    Run {
        engine,
        start,
        frames,
        result,
        ids,
    }
}

/// ASM-614: position and orientation of `before`, velocity zero.
fn assert_held(before: &KinematicState, now: &KinematicState, what: &str) {
    assert_eq!(now.position, before.position, "{what}: position kept");
    assert_eq!(
        now.orientation, before.orientation,
        "{what}: orientation kept"
    );
    assert_eq!(now.velocity, Vec3::ZERO, "{what}: velocity zero");
}

fn plugin_fault(result: &Result<RunSummary, EngineError>) -> &umbra_engine::PluginFault {
    let Err(EngineError::PluginFault(fault)) = result else {
        panic!("expected EngineError::PluginFault, got {result:?}");
    };
    fault
}

const MOTION_SCENE: [Member; 2] = [
    ("mover", "test.motion.coast", None),
    ("broken", "test.motion.nan", None),
];

const SENSOR_SCENE: [Member; 2] = [
    ("watcher", "test.motion.coast", Some("test.sensor.all")),
    (
        "blind",
        "test.motion.coast",
        Some("test.sensor.panic_sense"),
    ),
];

/// ADR 0021, ASM-614: a refused motion step under `abort` ends the run after its tick; the callback
/// saw that tick, with the faulted model's entity held and the healthy one moved.
#[test]
fn under_abort_a_faulting_plugin_motion_model_ends_the_run_after_the_tick_with_its_frame() {
    let loaded = load(&scene("motion-abort", "abort", &MOTION_SCENE));

    let r = run(&loaded);

    let fault = plugin_fault(&r.result);
    assert_eq!(
        (fault.plugin.as_str(), fault.capability.as_str(), fault.call),
        (PLUGIN, "test.motion.nan", "step")
    );
    assert!(fault.message.contains("finite"), "{}", fault.message);
    assert_eq!(
        r.engine.now().tick(),
        1,
        "the run stops right after the faulting tick"
    );
    assert_eq!(r.frames.len(), 1, "the faulting tick was seen");
    let (frame, hash) = &r.frames[0];
    assert_eq!(frame.time.tick(), 1);
    assert_eq!(*hash, r.engine.state_hash(), "the frame is the final state");
    let broken = r.id("broken");
    assert_held(&state(&r.start, broken), &state(frame, broken), "broken");
    let mover = r.id("mover");
    assert_ne!(
        state(frame, mover).position,
        state(&r.start, mover).position,
        "the healthy model moved"
    );
}

/// ADR 0021: the state at the faulting tick does not depend on the policy.
#[test]
fn abort_and_disable_capability_agree_on_the_tick_of_a_motion_fault() {
    let abort = run(&load(&scene("motion-agree-abort", "abort", &MOTION_SCENE)));
    let disable = run(&load(&scene(
        "motion-agree-disable",
        "disable_capability",
        &MOTION_SCENE,
    )));

    assert!(abort.result.is_err());
    assert_eq!(abort.frames.len(), 1);
    assert_eq!(disable.frames.len(), usize::try_from(TICKS).expect("fits"));
    assert_eq!(abort.frames[0].1, disable.frames[0].1, "hash at tick 1");
}

/// ASM-614, ADR 0021: under `disable_capability` the run goes on; the faulted model is never called
/// again, its entity stays held on every tick, and the fault is recorded once.
#[test]
fn under_disable_capability_a_faulting_plugin_motion_model_holds_its_entity_and_the_run_goes_on() {
    let loaded = load(&scene(
        "motion-disable",
        "disable_capability",
        &MOTION_SCENE,
    ));

    let r = run(&loaded);

    let summary = r.result.as_ref().expect("the run goes on");
    assert_eq!(summary.ticks_run, TICKS);
    let broken = r.id("broken");
    let start = state(&r.start, broken);
    for (frame, _) in &r.frames {
        assert_held(&start, &state(frame, broken), "broken");
    }
    let mover = r.id("mover");
    let last = &r.frames.last().expect("frames").0;
    assert_ne!(state(last, mover).position, state(&r.start, mover).position);
    let faults = loaded.plugins.faults();
    assert_eq!(faults.len(), 1, "{faults:?}");
    assert_eq!(faults[0].capability.as_str(), "test.motion.nan");
}

/// ADR 0021, ASM-615: a panic in `sense` under `abort` ends the run after its tick; that tick's frame
/// has no contacts for the faulted sensor's entity, while the healthy sensor saw its candidate.
#[test]
fn under_abort_a_faulting_plugin_sensor_ends_the_run_after_the_tick_with_its_frame() {
    let loaded = load(&scene("sensor-abort", "abort", &SENSOR_SCENE));

    let r = run(&loaded);

    let fault = plugin_fault(&r.result);
    assert_eq!(
        (fault.plugin.as_str(), fault.capability.as_str(), fault.call),
        (PLUGIN, "test.sensor.panic_sense", "sense")
    );
    assert_eq!(r.engine.now().tick(), 1);
    assert_eq!(r.frames.len(), 1);
    let frame = &r.frames[0].0;
    assert_eq!(contacts(frame, r.id("blind")), 0);
    assert_eq!(contacts(frame, r.id("watcher")), 1, "the other entity");
}

/// ASM-615: under `disable_capability` the faulted sensor yields no contacts on any tick and the
/// other sensor keeps contacting.
#[test]
fn under_disable_capability_a_faulting_plugin_sensor_yields_no_contacts_and_the_run_goes_on() {
    let loaded = load(&scene(
        "sensor-disable",
        "disable_capability",
        &SENSOR_SCENE,
    ));

    let r = run(&loaded);

    assert_eq!(r.result.as_ref().expect("the run goes on").ticks_run, TICKS);
    let (blind, watcher) = (r.id("blind"), r.id("watcher"));
    for (frame, _) in &r.frames {
        assert_eq!(contacts(frame, blind), 0);
        assert_eq!(contacts(frame, watcher), 1);
    }
    let faults = loaded.plugins.faults();
    assert_eq!(faults.len(), 1, "{faults:?}");
    assert_eq!(faults[0].capability.as_str(), "test.sensor.panic_sense");
}

/// The handle path (ADR 0021, `after_abort`): the faulting tick's frame is published, then the run
/// stops with `StopReason::PluginFault` naming the capability and the call.
fn assert_handle_abort(tag: &str, members: &[Member], capability: &str, call: &str) {
    let _lock = handle_support::plugin_lock();
    let mut config = handle_support::config();
    config.load.extra_plugin_paths = vec![PLUGINS.clone()];
    let (handle, _thread) = handle_support::start_engine(config);
    handle
        .load(scene(tag, "abort", members), None, None)
        .expect("load");
    let sub = handle.subscribe(handle_support::lossless());

    handle.start().expect("start");
    let frames = handle_support::until_stopped(&sub);

    let stop = frames.last().expect("frames");
    let Some(StopReason::PluginFault(fault)) = &stop.run.stop_reason else {
        panic!("expected StopReason::PluginFault, got {:?}", stop.run);
    };
    assert_eq!(
        (fault.plugin.as_str(), fault.capability.as_str(), fault.call),
        (PLUGIN, capability, call)
    );
    let before_stop = &frames[frames.len() - 2];
    assert_eq!(
        before_stop.cause,
        FrameCause::Tick,
        "the faulting tick's own frame"
    );
    let tick = |f: &umbra_engine::bus::Frame| f.snapshot.as_ref().map(|s| s.time.tick());
    assert_eq!(tick(before_stop), Some(1));
    assert_eq!(tick(stop), Some(1));
}

#[test]
fn on_the_engine_handle_a_plugin_motion_fault_publishes_the_faulting_tick_then_stops() {
    assert_handle_abort("motion-handle", &MOTION_SCENE, "test.motion.nan", "step");
}

#[test]
fn on_the_engine_handle_a_plugin_sensor_fault_publishes_the_faulting_tick_then_stops() {
    assert_handle_abort(
        "sensor-handle",
        &SENSOR_SCENE,
        "test.sensor.panic_sense",
        "sense",
    );
}
