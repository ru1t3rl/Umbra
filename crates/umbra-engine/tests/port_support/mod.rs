//! Loopback UDP ports for the DIS tests that start engines, plugins or processes which each need the
//! other's port before any of them is bound (ASM-1200, which amends ASM-246). Port 0 cannot break
//! that cycle here: a plugin adapter does not report its bound address. So ports are chosen first
//! and released just before the adapters bind them; another process can take one in that gap, and
//! [`retry_on_port_in_use`] reruns the test body, at most [`ATTEMPTS`] times, for that failure only.
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

use std::net::UdpSocket;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};

/// Runs of a test body before the last failure is final.
const ATTEMPTS: usize = 5;

/// What the OS and the DIS transport say when a port is taken (`binding 127.0.0.1:P: Address
/// already in use`).
const IN_USE: &str = "Address already in use";

/// `n` distinct free loopback ports, held until all are chosen, then released.
pub fn ports(n: usize) -> Vec<u16> {
    let sockets: Vec<UdpSocket> = (0..n)
        .map(|_| UdpSocket::bind("127.0.0.1:0").expect("ephemeral port"))
        .collect();
    sockets
        .iter()
        .map(|s| s.local_addr().expect("address").port())
        .collect()
}

fn is_in_use(payload: &(dyn std::any::Any + Send)) -> bool {
    payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| payload.downcast_ref::<&str>().copied())
        .is_some_and(|m| m.contains(IN_USE))
}

/// Runs `body` (which picks its own [`ports`]); when it panics because a port was taken in the gap,
/// runs it again with fresh ports. Any other panic, and the last attempt, propagate unchanged.
pub fn retry_on_port_in_use(mut body: impl FnMut()) {
    for _ in 1..ATTEMPTS {
        match catch_unwind(AssertUnwindSafe(&mut body)) {
            Ok(()) => return,
            Err(payload) if is_in_use(payload.as_ref()) => {}
            Err(payload) => resume_unwind(payload),
        }
    }
    body();
}
