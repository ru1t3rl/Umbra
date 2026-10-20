//! AT-4 (engine handle, shutdown; M5 WI-8 and WI-9): shutdown never blocks on the command queue,
//! answers a pending step `Closed`, and joins the thread on drop. Requirements: REQ-X-1, ADR 0024
//! (review M12).

#![forbid(unsafe_code)]

mod handle_support;

use std::sync::mpsc;

use handle_support::{
    GUARD, capture, config, engine, finishes, jam, load_stepped, long_skeleton, receive, release,
    start_engine,
};
use umbra_engine::handle::{Command, EngineStatus, HandleError, StepOutcome};

#[test]
#[ignore = "WI-8"]
fn shutdown_while_command_queue_full() {
    let mut small = config();
    small.command_capacity = 1;
    let (handle, thread) = start_engine(small);
    let held = jam(&handle);
    let (queued, queued_rx) = capture::<EngineStatus>();
    handle
        .submit(Command::Status { reply: queued })
        .expect("fits");
    let (refused, _refused_rx) = capture::<EngineStatus>();
    assert!(matches!(
        handle.submit(Command::Status { reply: refused }),
        Err(HandleError::Busy)
    ));
    let (done_tx, done_rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = done_tx.send(thread.shutdown());
    });

    held.release();

    let shut = done_rx
        .recv_timeout(GUARD)
        .expect("shutdown did not hang on the full queue");
    assert!(shut.is_ok());
    // The queued command was answered one way or the other, never left waiting.
    let _answered: Result<EngineStatus, HandleError> = receive(&queued_rx);
}

#[test]
#[ignore = "WI-9"]
fn shutdown_cancels_pending_step_with_closed() {
    let (handle, thread) = engine();
    load_stepped(&handle, long_skeleton());
    handle.start().expect("start");
    let (reply, step) = capture::<StepOutcome>();
    handle
        .submit(Command::Step {
            count: 10_000_000,
            reply,
        })
        .expect("submit");

    finishes(move || thread.shutdown()).expect("shutdown");

    assert!(matches!(receive(&step), Err(HandleError::Closed)));
}

#[test]
#[ignore = "WI-8"]
fn dropping_engine_thread_joins() {
    let (handle, thread) = engine();
    assert!(handle.status().is_ok());

    finishes(move || release(thread));

    assert!(
        matches!(handle.status(), Err(HandleError::Closed)),
        "the thread ended before the drop returned"
    );
}

#[test]
#[ignore = "WI-8"]
fn dropping_all_handles_ends_the_thread() {
    let (handle, thread) = engine();
    let clone = handle.clone();
    assert!(clone.status().is_ok());

    drop(handle);
    drop(clone);

    let shut = finishes(move || thread.shutdown());
    assert!(shut.is_ok(), "the thread ended normally, not by a panic");
}
