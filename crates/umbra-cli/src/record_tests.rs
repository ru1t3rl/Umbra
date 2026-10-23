//! Unit tests of how `run --record` ends a recording (ASM-732, review round 1 minors 6 and 7).

use std::ops::ControlFlow;
use std::path::PathBuf;

use umbra_engine::builtins::standard_registry;
use umbra_engine::scenario::RunOptions;
use umbra_engine::{
    ComponentId, Engine, EngineError, KeySpace, LoopbackHub, ManualWallClock, NetFault,
    RecordOptions, Recorder, RunSummary, instantiate_with_plugins, load_scenario_with_plugins,
};

use super::{abort_start, end_reason, finish_run, start_run};

fn scenario() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/scenarios/walking_skeleton/scenario.toml")
}

/// An engine of the example, recording to a file under `target/`.
fn recording_engine(tag: &str) -> (Engine, PathBuf) {
    let registry = standard_registry().expect("registry");
    let loaded = load_scenario_with_plugins(&registry, &scenario(), &[]).expect("load");
    let run = instantiate_with_plugins(
        &loaded,
        RunOptions {
            wall: Box::new(ManualWallClock::new()),
            seed: None,
            clock: None,
        },
    )
    .expect("instantiate");
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/cli-unit");
    std::fs::create_dir_all(&dir).expect("dir");
    let file = dir.join(format!("{tag}.umbrarec.jsonl"));
    let mut engine = run.engine;
    let recorder =
        Recorder::create(RecordOptions::new(&file), &scenario(), &loaded, None).expect("recorder");
    engine.attach_recorder(recorder).expect("attach");
    (engine, file)
}

fn summary(stop: bool) -> Result<RunSummary, EngineError> {
    let (mut engine, file) = recording_engine("summary");
    let result = engine.step(2, |_| {
        if stop {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    });
    drop(engine);
    let _ = std::fs::remove_file(file);
    result
}

#[test]
fn the_end_reason_follows_the_outcome_and_a_stop_fault() {
    let fault = NetFault::new("lb", "broke");
    let net = Err(EngineError::NetworkFault(fault.clone()));

    assert_eq!(end_reason(&summary(false), None), "completed");
    assert_eq!(end_reason(&summary(true), None), "stopped");
    assert_eq!(end_reason(&net, None), "network_fault");
    // Minor 6: a fault while sending `Stop` is the final status, like the handle's.
    assert_eq!(end_reason(&summary(false), Some(&fault)), "network_fault");
    assert_eq!(end_reason(&summary(true), Some(&fault)), "network_fault");
}

/// Minor 7: a fault on `Start` ends the run before any tick; the recording still gets its `end`.
#[test]
fn a_fault_on_start_still_closes_the_recording() {
    let (mut engine, file) = recording_engine("start-fault");

    let error = abort_start(&mut engine, &NetFault::new("lb", "broke"));

    assert!(
        format!("{error:#}").contains("starting the run"),
        "{error:#}"
    );
    let text = std::fs::read_to_string(&file).expect("recording");
    let last = text.lines().last().unwrap_or_default();
    assert!(
        last.contains("\"type\":\"end\"") && last.contains("\"reason\":\"network_fault\""),
        "{last}"
    );
    drop(engine);
    let _ = std::fs::remove_file(file);
}

/// The engine of the `net_pair/control` fixture (one loopback adapter publishing run control, policy
/// `abort`) recording to a file; the hub is returned to break the adapter.
fn control_engine(tag: &str) -> (Engine, PathBuf, std::sync::Arc<LoopbackHub>) {
    let hub = LoopbackHub::new(64);
    let mut registry = standard_registry().expect("registry");
    registry
        .register_network_adapter(hub.factory(
            ComponentId::new("test.net.loopback").expect("id"),
            KeySpace {
                base: 1000,
                count: 100,
            },
        ))
        .expect("register");
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../umbra-engine/tests/fixtures/net_pair/control/scenario.toml");
    let loaded = load_scenario_with_plugins(&registry, &path, &[]).expect("load");
    let run = instantiate_with_plugins(
        &loaded,
        RunOptions {
            wall: Box::new(ManualWallClock::new()),
            seed: None,
            clock: None,
        },
    )
    .expect("instantiate");
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/cli-unit");
    std::fs::create_dir_all(&dir).expect("dir");
    let file = dir.join(format!("{tag}.umbrarec.jsonl"));
    let mut engine = run.engine;
    let recorder =
        Recorder::create(RecordOptions::new(&file), &path, &loaded, None).expect("recorder");
    engine.attach_recorder(recorder).expect("attach");
    (engine, file, hub)
}

fn last_line(file: &PathBuf) -> String {
    let text = std::fs::read_to_string(file).expect("recording");
    text.lines().last().unwrap_or_default().to_owned()
}

/// Minor 1 (re-review): the real path of a run whose `Stop` faults: the recording ends with
/// `network_fault` and the fault is returned.
#[test]
fn a_stop_that_faults_ends_the_recording_with_network_fault() {
    let (mut engine, file, hub) = control_engine("stop-fault");
    let stepped = engine.step(0, |_| ControlFlow::Continue(()));
    assert!(stepped.is_ok());
    hub.close();

    let (fault, recorded) = finish_run(&mut engine, &stepped);

    assert!(fault.is_some() && recorded.is_ok());
    assert!(last_line(&file).contains("\"reason\":\"network_fault\""));
    drop(engine);
    let _ = std::fs::remove_file(file);
}

/// The same run without a fault ends `completed`, so the row above depends on the fault.
#[test]
fn a_stop_that_works_ends_the_recording_completed() {
    let (mut engine, file, _hub) = control_engine("stop-ok");
    let stepped = engine.step(0, |_| ControlFlow::Continue(()));

    let (fault, recorded) = finish_run(&mut engine, &stepped);

    assert!(fault.is_none() && recorded.is_ok());
    assert!(last_line(&file).contains("\"reason\":\"completed\""));
    drop(engine);
    let _ = std::fs::remove_file(file);
}

/// Minor 1: `start_run` on a broken adapter fails with the start error and closes the recording.
#[test]
fn a_start_that_faults_fails_the_run_and_closes_the_recording() {
    let (mut engine, file, hub) = control_engine("start-fault2");
    hub.close();

    let started = start_run(&mut engine);

    let error = started.expect_err("the start faults");
    assert!(format!("{error:#}").contains("starting the run"));
    assert!(last_line(&file).contains("\"reason\":\"network_fault\""));
    drop(engine);
    let _ = std::fs::remove_file(file);
}
