//! `umbra_server::run` with `ShutdownMode::Signals` (WI-17, ASM-217): SIGTERM triggers the shutdown
//! and the run returns within the grace deadline. This file holds the one test that signals the test
//! process itself, so no other test of this binary can be hit by it.

#![cfg(unix)]
#![forbid(unsafe_code)]

mod run_support;

use std::process::Command;
use std::sync::mpsc;
use std::time::Duration;

use run_support::{get, recv, start};
use umbra_server::{ShutdownMode, run};

#[test]
fn sigterm_ends_the_run_and_stops_the_engine() {
    let (settings, handle, thread) = start(Duration::from_secs(5));
    let probe = handle.clone();
    let (addr_tx, addr_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = run(
            settings,
            handle,
            thread,
            ShutdownMode::Signals,
            move |addr| addr_tx.send(addr).map_err(std::io::Error::other),
        );
        let _ = done_tx.send(result);
    });
    let addr = recv(&addr_rx, "the listening callback");
    // An answered request proves the serve loop, and so the handlers, are up.
    assert!(get(addr, "/api/v1/status").starts_with("HTTP/1.1 200"));
    let sent = Command::new("kill")
        .args(["-TERM", &std::process::id().to_string()])
        .status()
        .expect("kill");
    assert!(sent.success());
    let result = recv(&done_rx, "the run ending after SIGTERM");
    assert!(result.is_ok(), "{result:?}");
    assert!(probe.is_closed());
}
