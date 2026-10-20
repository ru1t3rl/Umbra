//! `umbra_server::run` with `ShutdownMode::Signals` (WI-17, ASM-217): SIGINT, sent from inside the
//! `on_listening` callback (the earliest a client could send it), triggers the shutdown and the run
//! returns. A handler registered lazily after the announcement is NOT caught here (the worker polls
//! the wait task before the spawned `kill` arrives); the unit test `sigint_between_install_and_wait_is_delivered`
//! in `src/run.rs` catches it. This file holds the one test that signals the test process itself.

#![cfg(unix)]
#![forbid(unsafe_code)]

#[expect(
    dead_code,
    reason = "each test binary uses a subset of the shared helpers"
)]
mod run_support;

use std::process::Command;
use std::sync::mpsc;
use std::time::Duration;

use run_support::{recv, start};
use umbra_server::{ShutdownMode, run};

#[test]
fn sigint_right_after_the_announcement_ends_the_run_and_stops_the_engine() {
    let (settings, handle, thread) = start(Duration::from_secs(5));
    let probe = handle.clone();
    let (done_tx, done_rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = run(settings, handle, thread, ShutdownMode::Signals, |_| {
            let sent = Command::new("kill")
                .args(["-INT", &std::process::id().to_string()])
                .status()?;
            sent.success()
                .then_some(())
                .ok_or_else(|| std::io::Error::other("kill failed"))
        });
        let _ = done_tx.send(result);
    });
    let result = recv(&done_rx, "the run ending after SIGINT");
    assert!(result.is_ok(), "{result:?}");
    assert!(probe.is_closed());
}
