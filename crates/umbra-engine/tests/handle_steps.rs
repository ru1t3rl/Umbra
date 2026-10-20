//! AT-4 (engine handle, steps; M5 WI-9): `step` on the stepped clock, one pending at a time, its
//! outcome when the run ends early, its cancellation, and the frames around it.
//! Requirements: REQ-X-1, REQ-K-3, REQ-K-4, ADR 0024, ADR 0026 (M10, M11, M2).
//!
//! Long runs use `long_skeleton` (a duration no test reaches) with the stepped clock, so a pending
//! step stays pending until the test ends it.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]
#![expect(clippy::panic, reason = "tests fail by panicking")]

mod common;
mod handle_support;
mod plugin_support;

use core::time::Duration;
use std::num::{NonZeroU64, NonZeroUsize};

use common::{example_engine, run_traced};
use handle_support::{
    GUARD, capture, config, engine, examples, load_stepped, long_skeleton, plugin_lock, receive,
    scenario_copy, short_skeleton, start_engine, state_of,
};
use umbra_core::SimEvent;
use umbra_engine::builtins::standard_registry;
use umbra_engine::bus::{FrameCause, RunState, StopReason, SubscribeOptions};
use umbra_engine::handle::{Command, EngineHandle, HandleError, StepOutcome};
use umbra_engine::scenario::{RunOptions, instantiate, load_scenario};
use umbra_engine::{ClockMode, ManualWallClock, RunUntil, SimTime};

/// A stepped, started run that cannot end by itself, with a step of 10 million ticks pending.
fn pending_step() -> (
    EngineHandle,
    umbra_engine::handle::EngineThread,
    std::sync::mpsc::Receiver<Result<StepOutcome, HandleError>>,
) {
    let (handle, thread) = engine();
    load_stepped(&handle, long_skeleton());
    handle.start().expect("start");
    let (reply, rx) = capture::<StepOutcome>();
    handle
        .submit(Command::Step {
            count: 10_000_000,
            reply,
        })
        .expect("submit the long step");
    (handle, thread, rx)
}

#[test]
#[ignore = "WI-9"]
fn step_reply_after_n_ticks_completed_true() {
    let (handle, _thread) = engine();
    load_stepped(&handle, long_skeleton());
    handle.start().expect("start");
    let mut reference = example_engine(None, None);
    let mut trace = common::Trace::default();
    run_traced(&mut reference, RunUntil::Ticks(5), &mut trace);

    let outcome = handle.step(5).expect("step");

    assert_eq!(outcome.requested, 5);
    assert!(outcome.completed);
    assert_eq!(outcome.applied_at.tick(), 0);
    assert_eq!(outcome.reached.tick(), 5);
    assert_eq!(Some(&outcome.state_hash), trace.hashes.last());
    assert_eq!(outcome.status.state, RunState::Running);
    assert_eq!(outcome.status.time.map(SimTime::tick), Some(5));
    assert!(!outcome.status.step_pending);
}

#[test]
#[ignore = "WI-9"]
fn second_step_while_pending_is_step_in_progress() {
    let (handle, _thread, first) = pending_step();

    // Submitted, not called: a wrongly accepted second step then fails after the guard instead of
    // blocking the test forever.
    let (reply, second_rx) = capture::<StepOutcome>();
    handle
        .submit(Command::Step { count: 1, reply })
        .expect("submit the second step");
    let second = receive(&second_rx);
    let pending = handle.status().expect("status").run.step_pending;
    handle.unload().expect("unload");

    assert!(
        matches!(second, Err(HandleError::StepInProgress)),
        "{second:?}"
    );
    assert!(pending, "the first step is still pending");
    assert!(matches!(
        receive(&first),
        Err(HandleError::StepCancelled { .. })
    ));
}

#[test]
#[ignore = "WI-9"]
fn step_ended_by_duration_is_completed_false() {
    let (handle, _thread) = engine();
    let info = handle
        .load(short_skeleton(), None, Some(ClockMode::Stepped))
        .expect("load");
    handle.start().expect("start");

    let outcome = handle.step(info.duration_ticks + 1000).expect("step");

    assert!(!outcome.completed);
    assert_eq!(outcome.requested, info.duration_ticks + 1000);
    assert_eq!(outcome.reached.tick(), info.duration_ticks);
    assert_eq!(outcome.status.state, RunState::Stopped);
    assert_eq!(outcome.status.stop_reason, Some(StopReason::Duration));
}

#[test]
#[ignore = "WI-9"]
fn step_ended_by_stop_is_completed_false() {
    let (handle, _thread, step) = pending_step();

    handle.stop().expect("stop");
    let outcome = receive(&step).expect("the step is answered, not cancelled");

    assert!(!outcome.completed);
    assert!(outcome.reached.tick() < 10_000_000);
    assert_eq!(outcome.status.state, RunState::Stopped);
    assert_eq!(outcome.status.stop_reason, Some(StopReason::Command));
    assert_eq!(state_of(&handle), RunState::Stopped);
}

#[test]
#[ignore = "WI-9"]
fn pause_unload_clock_change_and_fault_cancel_the_step() {
    type Action = fn(&EngineHandle);
    let actions: [(&str, Action); 3] = [
        ("pause", |h| {
            h.pause().expect("pause");
        }),
        ("unload", |h| h.unload().expect("unload")),
        ("clock change away from stepped", |h| {
            h.set_clock(ClockMode::RealTime).expect("set clock");
        }),
    ];
    for (name, action) in actions {
        let (handle, _thread, step) = pending_step();

        action(&handle);

        let cancelled = receive(&step);
        assert!(
            matches!(cancelled, Err(HandleError::StepCancelled { .. })),
            "{name}: {cancelled:?}"
        );
        assert!(
            !handle.status().map_or(true, |s| s.run.step_pending),
            "{name}"
        );
    }

    // A plugin fault under `abort` cancels it too.
    let _lock = plugin_lock();
    let mut with_fixture = config();
    with_fixture.load.extra_plugin_paths = vec![plugin_support::fixture(&[])];
    let (handle, _thread) = start_engine(with_fixture);
    let scenario =
        plugin_support::fault_scenario("steps-abort", "abort", 100.0, &["healthy", "direct"]);
    handle
        .load(scenario, None, Some(ClockMode::Stepped))
        .expect("load");
    handle.start().expect("start");

    let faulted = handle.step(1000);

    assert!(
        matches!(faulted, Err(HandleError::StepCancelled { .. })),
        "{faulted:?}"
    );
    let run = handle.status().expect("status").run;
    assert_eq!(run.state, RunState::Stopped);
    assert!(matches!(run.stop_reason, Some(StopReason::PluginFault(_))));
}

#[test]
#[ignore = "WI-9"]
fn frame_published_at_step_completion_even_with_stride() {
    let (handle, _thread) = engine();
    load_stepped(&handle, long_skeleton());
    let mut sparse = SubscribeOptions::new();
    sparse.min_tick_stride = NonZeroU64::new(1000).expect("non-zero");
    sparse.capacity = NonZeroUsize::new(64).expect("non-zero");
    let sub = handle.subscribe(sparse);
    handle.start().expect("start");
    drop(sub.wait(GUARD));

    handle.step(7).expect("step");

    let frames = sub.drain().frames;
    let last = frames.last().expect("a frame at the step's end");
    assert_eq!(last.snapshot.as_ref().map(|s| s.time.tick()), Some(7));
    assert_eq!(last.run.state, RunState::Running);
    assert!(
        frames.iter().all(|f| f.cause == FrameCause::Control),
        "no tick of 1..=7 is due at stride 1000"
    );
}

#[test]
#[ignore = "WI-9"]
fn step_100_across_a_fire_delivers_the_event_to_a_capacity_4_subscriber() {
    const TICKS: u64 = 100;
    // The reference: the Fire events of the first 100 ticks.
    let registry = standard_registry().expect("registry");
    let scenario =
        load_scenario(&registry, &handle_support::example("patrol_and_engage")).expect("load");
    let options = RunOptions {
        wall: Box::new(ManualWallClock::with_auto_advance(Duration::from_micros(
            500,
        ))),
        seed: None,
        clock: None,
    };
    let mut reference = instantiate(&registry, &scenario, options)
        .expect("instantiate")
        .engine;
    let mut want = Vec::new();
    reference
        .run(RunUntil::Ticks(TICKS), |view| {
            want.extend(
                view.report
                    .events
                    .iter()
                    .filter(|e| matches!(e, SimEvent::Fire { .. }))
                    .cloned(),
            );
            std::ops::ControlFlow::Continue(())
        })
        .expect("run");
    assert!(!want.is_empty(), "the reference fires within {TICKS} ticks");

    let (handle, _thread) = engine();
    load_stepped(&handle, examples().join("patrol_and_engage/scenario.toml"));
    let sub = handle.subscribe(SubscribeOptions::new());
    handle.start().expect("start");
    handle.step(TICKS).expect("step");

    let drained = sub.drain();
    let got: Vec<SimEvent> = drained
        .carried_events
        .iter()
        .chain(drained.frames.iter().flat_map(|f| f.events.iter()))
        .filter(|e| matches!(e, SimEvent::Fire { .. }))
        .cloned()
        .collect();

    assert!(
        drained.dropped > 0,
        "a capacity of 4 cannot hold {TICKS} frames"
    );
    assert!(!drained.events_truncated);
    assert_eq!(got, want);
}

#[test]
fn scenario_copy_helper_is_available() {
    // extra: runs now; keeps the shared helper imports honest in this binary.
    let path = scenario_copy(&examples().join("walking_skeleton"), "steps-helper", |t| t);
    assert!(path.is_file());
}
