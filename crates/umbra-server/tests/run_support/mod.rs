//! Helpers of the `run` tests (`run.rs`, `run_signal.rs`): a settings/handle pair for
//! `umbra_server::run`, and a blocking HTTP client on `std::net`.
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

use umbra_engine::Config;
use umbra_engine::handle::{EngineHandle, EngineThread};
use umbra_server::ServerSettings;

/// The longest a test waits for something that must happen (a failing test, never an assertion
/// about time).
pub const HANG_GUARD: Duration = Duration::from_secs(60);

/// Settings with a loopback port-0 bind, the given grace and the scenario root `CARGO_TARGET_TMPDIR`,
/// and the engine handle built from them as `umbra serve` builds it.
pub fn start(grace: Duration) -> (ServerSettings, EngineHandle, EngineThread) {
    let mut config = Config::default();
    config.server.bind = SocketAddr::from(([127, 0, 0, 1], 0));
    config.server.shutdown_grace = grace;
    config.server.scenario_root = Path::new(env!("CARGO_TARGET_TMPDIR")).to_path_buf();
    let settings = ServerSettings::from_config(&config);
    let handle_config = settings.handle_config().expect("handle configuration");
    let (handle, thread) = EngineHandle::spawn(handle_config).expect("spawn the engine");
    (settings, handle, thread)
}

/// `GET path` with `Connection: close`; the response text.
pub fn get(addr: SocketAddr, path: &str) -> String {
    let mut stream = TcpStream::connect(addr).expect("connect");
    stream.set_read_timeout(Some(HANG_GUARD)).expect("timeout");
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n"
    )
    .expect("write");
    let mut text = String::new();
    stream.read_to_string(&mut text).expect("read");
    text
}

/// Receives from `rx`, failing the test after the hang guard.
pub fn recv<T>(rx: &mpsc::Receiver<T>, what: &str) -> T {
    rx.recv_timeout(HANG_GUARD)
        .unwrap_or_else(|_| panic!("{what} did not happen within the hang guard"))
}
