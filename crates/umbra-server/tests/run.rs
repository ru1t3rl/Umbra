//! `umbra_server::run` with `ShutdownMode::Manual` (WI-17, ASM-217): it announces the bound
//! address, serves, and returns once the trigger fires, within the grace deadline and with the
//! engine thread ended. The signal path is `run_signal.rs`.

#![forbid(unsafe_code)]

mod run_support;

use std::io::Write;
use std::net::{SocketAddr, TcpStream};
use std::sync::mpsc;
use std::time::Duration;

use run_support::{get, recv, start};
use umbra_server::{ServerError, Shutdown, ShutdownMode, run};

/// Runs the server on a thread; the receiver gets the announced address.
fn spawn_run(
    grace: Duration,
) -> (
    umbra_engine::handle::EngineHandle,
    Shutdown,
    mpsc::Receiver<SocketAddr>,
    mpsc::Receiver<Result<(), ServerError>>,
) {
    let (settings, handle, thread) = start(grace);
    let shutdown = Shutdown::new();
    let (addr_tx, addr_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    let probe = handle.clone();
    let mode = ShutdownMode::Manual(shutdown.clone());
    std::thread::spawn(move || {
        let result = run(settings, handle, thread, mode, move |addr| {
            addr_tx.send(addr).map_err(std::io::Error::other)
        });
        let _ = done_tx.send(result);
    });
    (probe, shutdown, addr_rx, done_rx)
}

#[test]
fn a_manual_trigger_ends_the_run_cleanly_and_stops_the_engine() {
    let (probe, shutdown, addr_rx, done_rx) = spawn_run(Duration::from_secs(5));
    let addr = recv(&addr_rx, "the listening callback");
    assert_ne!(addr.port(), 0, "the announced port is the bound one");
    assert!(get(addr, "/api/v1/status").starts_with("HTTP/1.1 200"));
    assert!(!probe.is_closed(), "the engine runs while serving");
    shutdown.trigger();
    let result = recv(&done_rx, "the run ending");
    assert!(result.is_ok(), "{result:?}");
    assert!(probe.is_closed(), "run ended the engine thread");
}

#[test]
#[expect(
    clippy::disallowed_methods,
    reason = "measures the real wait of a real server, outside simulation logic"
)]
fn the_grace_deadline_ends_the_run_despite_a_stuck_request() {
    let grace = Duration::from_millis(200);
    let (_probe, shutdown, addr_rx, done_rx) = spawn_run(grace);
    let addr = recv(&addr_rx, "the listening callback");
    // A request that never completes is open at shutdown; only the deadline ends the wait.
    let mut stuck = TcpStream::connect(addr).expect("connect");
    stuck
        .write_all(b"GET /api/v1/status HTTP/1.1\r\nHost: 127.0.0.1\r\n")
        .expect("write");
    assert!(get(addr, "/api/v1/status").starts_with("HTTP/1.1 200"));
    let fired = std::time::Instant::now();
    shutdown.trigger();
    let result = recv(&done_rx, "the run ending");
    let elapsed = fired.elapsed();
    assert!(result.is_ok(), "{result:?}");
    assert!(
        elapsed >= grace,
        "the run waited for the open request: {elapsed:?}"
    );
    assert!(elapsed < run_support::HANG_GUARD / 2, "{elapsed:?}");
    drop(stuck);
}

#[test]
fn an_address_in_use_is_a_bind_error_and_the_engine_still_ends() {
    let taken = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let (mut settings, handle, thread) = start(Duration::from_secs(1));
    settings.server.bind = taken.local_addr().expect("address");
    let probe = handle.clone();
    let result = run(
        settings,
        handle,
        thread,
        ShutdownMode::Manual(Shutdown::new()),
        |_| Ok(()),
    );
    assert!(
        matches!(result, Err(ServerError::Bind { .. })),
        "{result:?}"
    );
    assert!(probe.is_closed());
}
