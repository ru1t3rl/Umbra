//! UDP sockets, two I/O threads and two bounded queues (ADR 0034, ADR 0033, NFR-4).
//!
//! One receive thread reads datagrams (a short read timeout lets it see the stop flag) and queues them;
//! one send thread drains the outbound queue to the targets of the mode. Both queues are bounded and
//! drop and count on overflow, on both sides. The threads never call the host API (ADR 0033), never
//! panic on input, and [`Transport::stop`] joins both of them (within a poll interval). Nothing here
//! is global: every instance owns its socket, queues, flags and counters (ASM-410).

#![deny(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

use std::io::ErrorKind;
use std::net::{SocketAddr, SocketAddrV4, UdpSocket};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{
    Receiver, RecvTimeoutError, SyncSender, TryRecvError, TrySendError, sync_channel,
};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use socket2::{Domain, Protocol, Socket, Type};

use crate::config::Mode;

/// Datagrams above this size are dropped and counted (ADR 0034).
pub const MAX_DATAGRAM: usize = 8192;
/// How long a thread waits before it looks at the stop flag.
const POLL_INTERVAL: Duration = Duration::from_millis(50);
/// How long `stop` lets the send thread drain the queue; the rest is dropped and counted.
const DRAIN_LIMIT: Duration = Duration::from_millis(500);
/// Pause after a socket error, so a persistent error does not spin.
const ERROR_BACKOFF: Duration = Duration::from_millis(10);

/// Why a transport cannot start.
#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    /// A socket call failed.
    #[error("{what}: {source}")]
    Io {
        /// What was being done, with the address.
        what: String,
        /// The error.
        source: std::io::Error,
    },
}

fn io_error(what: impl Into<String>) -> impl FnOnce(std::io::Error) -> TransportError {
    let what = what.into();
    move |source| TransportError::Io { what, source }
}

/// The transport is stopped or an I/O thread is gone: nothing more can be sent or received.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the DIS transport is stopped or its I/O thread ended")]
pub struct Closed;

/// Counters shared with the I/O threads; all only grow.
#[derive(Debug, Default)]
struct Shared {
    tx_dropped: AtomicU64,
    rx_dropped: AtomicU64,
    io_errors: AtomicU64,
    oversized_rx: AtomicU64,
    oversized_tx: AtomicU64,
    sent: AtomicU64,
    received: AtomicU64,
}

fn bump(counter: &AtomicU64) {
    counter.fetch_add(1, Ordering::Relaxed);
}

/// A snapshot of the transport's counters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TransportCounters {
    /// Outbound datagrams dropped: queue full, oversized, or a failed `send_to`.
    pub tx_dropped: u64,
    /// Inbound datagrams dropped: queue full or oversized.
    pub rx_dropped: u64,
    /// Socket errors.
    pub io_errors: u64,
    /// Inbound datagrams over [`MAX_DATAGRAM`] (included in `rx_dropped`).
    pub oversized_rx: u64,
    /// Outbound datagrams over [`MAX_DATAGRAM`] (included in `tx_dropped`).
    pub oversized_tx: u64,
    /// Datagrams handed to the socket, per target.
    pub sent: u64,
    /// Datagrams queued for polling.
    pub received: u64,
}

/// What the I/O threads need of a socket; the real one is a [`UdpSocket`]. A fake in the unit tests
/// stands in to block or kill a thread (ASM-433).
pub(crate) trait Datagrams: Send + 'static {
    /// Receives one datagram (a timeout is an `Err` of kind `WouldBlock` or `TimedOut`).
    fn recv_from(&self, buffer: &mut [u8]) -> std::io::Result<usize>;
    /// Sends one datagram.
    fn send_to(&self, datagram: &[u8], to: SocketAddr) -> std::io::Result<usize>;
}

impl Datagrams for UdpSocket {
    fn recv_from(&self, buffer: &mut [u8]) -> std::io::Result<usize> {
        UdpSocket::recv_from(self, buffer).map(|(len, _)| len)
    }

    fn send_to(&self, datagram: &[u8], to: SocketAddr) -> std::io::Result<usize> {
        UdpSocket::send_to(self, datagram, to)
    }
}

/// The sockets, queues and threads of one adapter instance.
#[derive(Debug)]
pub struct Transport {
    local: SocketAddr,
    outbound: Option<SyncSender<Vec<u8>>>,
    inbound: Receiver<Vec<u8>>,
    shared: Arc<Shared>,
    stop: Arc<AtomicBool>,
    stopped: bool,
    threads: Vec<JoinHandle<()>>,
}

fn open(bind: SocketAddrV4, mode: &Mode) -> Result<(UdpSocket, Vec<SocketAddr>), TransportError> {
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))
        .map_err(io_error("creating the UDP socket"))?;
    // Unicast keeps the port exclusive; broadcast and multicast peers share a port on one host.
    let shared_port = !matches!(mode, Mode::Unicast { .. });
    if shared_port {
        socket
            .set_reuse_address(true)
            .map_err(io_error("setting SO_REUSEADDR"))?;
    }
    // Linux delivers a group's packets to every socket on the port by default (IP_MULTICAST_ALL);
    // only the groups this socket joined may reach it, or two instances on one port in different
    // groups would hear each other.
    #[cfg(target_os = "linux")]
    if matches!(mode, Mode::Multicast { .. }) {
        socket.set_multicast_all_v4(false).map_err(io_error(
            "restricting multicast delivery to the joined group",
        ))?;
    }
    socket
        .bind(&SocketAddr::V4(bind).into())
        .map_err(io_error(format!("binding {bind}")))?;
    let local = socket
        .local_addr()
        .map_err(io_error("reading the bound address"))?
        .as_socket()
        .ok_or_else(|| TransportError::Io {
            what: "reading the bound address".to_owned(),
            source: std::io::Error::other("not an IP socket address"),
        })?;
    let targets = match mode {
        Mode::Unicast { destinations } => {
            destinations.iter().copied().map(SocketAddr::V4).collect()
        }
        Mode::Broadcast { address } => {
            socket
                .set_broadcast(true)
                .map_err(io_error("enabling broadcast"))?;
            vec![SocketAddr::V4(*address)]
        }
        Mode::Multicast {
            group,
            interface,
            ttl,
        } => {
            socket
                .join_multicast_v4(group, interface)
                .map_err(io_error(format!("joining {group} on {interface}")))?;
            socket
                .set_multicast_if_v4(interface)
                .map_err(io_error(format!("selecting interface {interface}")))?;
            socket
                .set_multicast_ttl_v4(*ttl)
                .map_err(io_error("setting the multicast TTL"))?;
            socket
                .set_multicast_loop_v4(true)
                .map_err(io_error("enabling multicast loopback"))?;
            vec![SocketAddr::new((*group).into(), local.port())]
        }
    };
    Ok((socket.into(), targets))
}

impl Transport {
    /// Opens the socket for `bind` and `mode` and starts both threads. Queue sizes are in datagrams
    /// (at least 1).
    ///
    /// # Errors
    /// [`TransportError`] when a socket call fails (for example the port is in use).
    pub fn start(
        bind: SocketAddrV4,
        mode: &Mode,
        send_queue: usize,
        receive_queue: usize,
    ) -> Result<Self, TransportError> {
        let (socket, targets) = open(bind, mode)?;
        let local = socket
            .local_addr()
            .map_err(io_error("reading the bound address"))?;
        socket
            .set_read_timeout(Some(POLL_INTERVAL))
            .map_err(io_error("setting the read timeout"))?;
        // A send that blocks (a full socket buffer) times out, is counted, and cannot hold `stop`.
        socket
            .set_write_timeout(Some(POLL_INTERVAL))
            .map_err(io_error("setting the write timeout"))?;
        let send_socket = socket
            .try_clone()
            .map_err(io_error("cloning the socket for the send thread"))?;
        Self::start_with(
            socket,
            send_socket,
            local,
            targets,
            send_queue,
            receive_queue,
        )
    }

    /// Starts the threads over any [`Datagrams`] (`recv` for the receive thread, `send` for the send
    /// thread).
    pub(crate) fn start_with<D: Datagrams>(
        recv: D,
        send: D,
        local: SocketAddr,
        targets: Vec<SocketAddr>,
        send_queue: usize,
        receive_queue: usize,
    ) -> Result<Self, TransportError> {
        let shared = Arc::new(Shared::default());
        let stop = Arc::new(AtomicBool::new(false));
        let (out_tx, out_rx) = sync_channel(send_queue.max(1));
        let (in_tx, in_rx) = sync_channel(receive_queue.max(1));
        let spawn = |name: &str, f: Box<dyn FnOnce() + Send>| {
            std::thread::Builder::new()
                .name(name.to_owned())
                .spawn(f)
                .map_err(io_error(format!("starting the {name} thread")))
        };
        let recv_thread = {
            let (shared, stop) = (Arc::clone(&shared), Arc::clone(&stop));
            spawn(
                "dis-recv",
                Box::new(move || receive_loop(&recv, &in_tx, &shared, &stop)),
            )?
        };
        let send_thread = {
            let (shared, stop_flag) = (Arc::clone(&shared), Arc::clone(&stop));
            match spawn(
                "dis-send",
                Box::new(move || send_loop(&send, &targets, &out_rx, &shared, &stop_flag)),
            ) {
                Ok(t) => t,
                Err(e) => {
                    // Release and join the receive thread before reporting.
                    stop.store(true, Ordering::SeqCst);
                    let _ = recv_thread.join();
                    return Err(e);
                }
            }
        };
        Ok(Self {
            local,
            outbound: Some(out_tx),
            inbound: in_rx,
            shared,
            stop,
            stopped: false,
            threads: vec![recv_thread, send_thread],
        })
    }

    /// The address the socket is bound to (the chosen port when `bind` had port 0).
    #[must_use]
    pub fn local_addr(&self) -> SocketAddr {
        self.local
    }

    /// Queues `datagram` for sending to every target. A full queue or an oversized datagram is
    /// dropped and counted and is still `Ok`.
    ///
    /// # Errors
    /// [`Closed`] when the transport is stopped or the send thread ended.
    pub fn send(&self, datagram: Vec<u8>) -> Result<(), Closed> {
        let Some(outbound) = &self.outbound else {
            return Err(Closed);
        };
        if datagram.len() > MAX_DATAGRAM {
            bump(&self.shared.oversized_tx);
            bump(&self.shared.tx_dropped);
            return Ok(());
        }
        match outbound.try_send(datagram) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => {
                bump(&self.shared.tx_dropped);
                Ok(())
            }
            Err(TrySendError::Disconnected(_)) => Err(Closed),
        }
    }

    /// The next received datagram, if one is queued.
    ///
    /// # Errors
    /// [`Closed`] when the transport is stopped, or the receive thread ended and the queue is empty.
    pub fn receive(&self) -> Result<Option<Vec<u8>>, Closed> {
        if self.stopped {
            return Err(Closed);
        }
        match self.inbound.try_recv() {
            Ok(d) => Ok(Some(d)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err(Closed),
        }
    }

    /// Whether [`Transport::stop`] has run.
    #[must_use]
    pub fn is_stopped(&self) -> bool {
        self.stopped
    }

    /// A snapshot of the counters.
    #[must_use]
    pub fn counters(&self) -> TransportCounters {
        let s = &self.shared;
        TransportCounters {
            tx_dropped: s.tx_dropped.load(Ordering::Relaxed),
            rx_dropped: s.rx_dropped.load(Ordering::Relaxed),
            io_errors: s.io_errors.load(Ordering::Relaxed),
            oversized_rx: s.oversized_rx.load(Ordering::Relaxed),
            oversized_tx: s.oversized_tx.load(Ordering::Relaxed),
            sent: s.sent.load(Ordering::Relaxed),
            received: s.received.load(Ordering::Relaxed),
        }
    }

    /// Stops both threads and joins them: the send thread first sends what is queued, then both
    /// notice the closed queue or the stop flag within a poll interval. Repeating it does nothing.
    pub fn stop(&mut self) {
        if self.stopped {
            return;
        }
        self.stopped = true;
        self.stop.store(true, Ordering::SeqCst);
        // Dropping the sender lets the send thread drain the queue and end.
        self.outbound = None;
        for thread in self.threads.drain(..) {
            // A thread that panicked has nothing left to release.
            let _ = thread.join();
        }
    }
}

impl Drop for Transport {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Waits on an I/O thread after a socket error so a persistent error does not spin. The adapter's
/// I/O threads are not simulation logic: they never touch the tick (REQ-K-4 concerns the tick path).
#[expect(
    clippy::disallowed_methods,
    reason = "REQ-K-4 bans sleeping in simulation logic; this is an I/O thread backoff"
)]
fn pause(d: Duration) {
    std::thread::sleep(d);
}

fn receive_loop(
    socket: &impl Datagrams,
    queue: &SyncSender<Vec<u8>>,
    shared: &Shared,
    stop: &AtomicBool,
) {
    // One byte more than the limit tells an oversized datagram from one of exactly the limit.
    let mut buffer = vec![0_u8; MAX_DATAGRAM.saturating_add(1)];
    while !stop.load(Ordering::SeqCst) {
        match socket.recv_from(&mut buffer) {
            Ok(len) => {
                if len > MAX_DATAGRAM {
                    bump(&shared.oversized_rx);
                    bump(&shared.rx_dropped);
                    continue;
                }
                let Some(datagram) = buffer.get(..len) else {
                    continue;
                };
                match queue.try_send(datagram.to_vec()) {
                    Ok(()) => bump(&shared.received),
                    Err(TrySendError::Full(_)) => bump(&shared.rx_dropped),
                    Err(TrySendError::Disconnected(_)) => return,
                }
            }
            Err(e)
                if matches!(
                    e.kind(),
                    ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::Interrupted
                ) => {}
            Err(_) => {
                bump(&shared.io_errors);
                pause(ERROR_BACKOFF);
            }
        }
    }
}

/// The wall clock for the drain deadline of `stop`; I/O thread bookkeeping, not simulation logic.
#[expect(
    clippy::disallowed_methods,
    reason = "REQ-K-4 bans the wall clock in simulation logic; this bounds how long stop may take"
)]
fn now() -> Instant {
    Instant::now()
}

fn send_loop(
    socket: &impl Datagrams,
    targets: &[SocketAddr],
    queue: &Receiver<Vec<u8>>,
    shared: &Shared,
    stop: &AtomicBool,
) {
    // Once `stop` is asked, queued datagrams are still sent, but only until this deadline.
    let mut drain_until: Option<Instant> = None;
    loop {
        match queue.recv_timeout(POLL_INTERVAL) {
            Ok(datagram) => {
                if stop.load(Ordering::SeqCst) {
                    let limit = *drain_until.get_or_insert_with(|| now() + DRAIN_LIMIT);
                    if now() >= limit {
                        bump(&shared.tx_dropped);
                        continue;
                    }
                }
                for target in targets {
                    if socket.send_to(&datagram, *target).is_ok() {
                        bump(&shared.sent);
                    } else {
                        // A failed or timed-out send loses this datagram for this target; the
                        // adapter goes on.
                        bump(&shared.io_errors);
                        bump(&shared.tx_dropped);
                    }
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                if stop.load(Ordering::SeqCst) {
                    return;
                }
            }
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

#[cfg(test)]
#[expect(
    clippy::disallowed_methods,
    clippy::panic,
    reason = "the fakes sleep to block a thread or panic to kill one, and the tests time `stop` (REQ-K-4 governs simulation logic)"
)]
mod tests {
    use std::collections::VecDeque;
    use std::io::{Error, ErrorKind, Result};
    use std::net::SocketAddr;
    use std::sync::Mutex;

    use super::*;

    /// One step of a fake receive thread.
    enum Step {
        Data(Vec<u8>),
        Panic,
    }

    /// A socket that does what the test scripts: receives `script` then idles, and sends slowly,
    /// failing, or by panicking.
    struct Fake {
        script: Mutex<VecDeque<Step>>,
        send_delay: Duration,
        send_error: bool,
        send_panics: bool,
    }

    impl Fake {
        fn idle() -> Self {
            Self {
                script: Mutex::new(VecDeque::new()),
                send_delay: Duration::ZERO,
                send_error: false,
                send_panics: false,
            }
        }
    }

    impl Datagrams for Fake {
        fn recv_from(&self, buffer: &mut [u8]) -> Result<usize> {
            let step = self.script.lock().expect("script lock").pop_front();
            match step {
                Some(Step::Data(d)) => {
                    let n = d.len().min(buffer.len());
                    buffer
                        .get_mut(..n)
                        .expect("n is within the buffer")
                        .copy_from_slice(d.get(..n).expect("n is within the data"));
                    Ok(n)
                }
                Some(Step::Panic) => panic!("fake receive thread dies"),
                None => {
                    std::thread::sleep(Duration::from_millis(5));
                    Err(Error::from(ErrorKind::WouldBlock))
                }
            }
        }

        fn send_to(&self, datagram: &[u8], _to: SocketAddr) -> Result<usize> {
            assert!(!self.send_panics, "fake send thread dies");
            std::thread::sleep(self.send_delay);
            if self.send_error {
                Err(Error::from(ErrorKind::TimedOut))
            } else {
                Ok(datagram.len())
            }
        }
    }

    fn addr() -> SocketAddr {
        "127.0.0.1:9".parse().expect("address")
    }

    fn start(recv: Fake, send: Fake, queue: usize) -> Transport {
        Transport::start_with(recv, send, addr(), vec![addr()], queue, 8).expect("start")
    }

    fn wait_until(what: &str, mut f: impl FnMut() -> bool) {
        let begin = Instant::now();
        while !f() {
            assert!(
                begin.elapsed() < Duration::from_secs(10),
                "timed out: {what}"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    /// A send path that blocks (a full socket buffer): `stop` still returns within a second, sends
    /// what the drain deadline allows and counts the rest as dropped.
    #[test]
    fn stop_is_bounded_when_every_send_blocks() {
        let slow = Fake {
            send_delay: Duration::from_millis(200),
            ..Fake::idle()
        };
        let mut t = start(Fake::idle(), slow, 100);
        for i in 0..50_u8 {
            t.send(vec![i]).expect("queued");
        }
        let begin = Instant::now();
        t.stop();
        let took = begin.elapsed();
        assert!(took < Duration::from_secs(1), "stop took {took:?}");
        let c = t.counters();
        assert!(c.sent >= 1, "the drain sent some: {c:?}");
        assert!(c.tx_dropped > 0, "the rest was dropped: {c:?}");
        assert_eq!(
            c.sent + c.tx_dropped,
            50,
            "every datagram is either sent or counted: {c:?}"
        );
    }

    /// A send that times out is an I/O error and a dropped datagram; the thread goes on.
    #[test]
    fn a_timed_out_send_is_an_io_error_and_a_drop() {
        let failing = Fake {
            send_error: true,
            ..Fake::idle()
        };
        let t = start(Fake::idle(), failing, 100);
        for _ in 0..3 {
            t.send(vec![1]).expect("queued");
        }
        wait_until("three failures", || t.counters().io_errors == 3);
        let c = t.counters();
        assert_eq!((c.io_errors, c.tx_dropped, c.sent), (3, 3, 0));
    }

    /// A send thread that died: queued sends stop being accepted, `send` is `Err(Closed)`.
    #[test]
    fn send_fails_once_the_send_thread_is_gone() {
        let dying = Fake {
            send_panics: true,
            ..Fake::idle()
        };
        let t = start(Fake::idle(), dying, 8);
        assert_eq!(
            t.send(vec![1]),
            Ok(()),
            "the thread was alive when it was queued"
        );
        wait_until("the send thread to die", || t.send(vec![2]) == Err(Closed));
        assert_eq!(t.send(vec![3]), Err(Closed));
    }

    /// A receive thread that died: what it had queued is delivered, then `receive` is `Err(Closed)`.
    #[test]
    fn receive_delivers_what_is_queued_then_fails_once_the_receive_thread_is_gone() {
        let recv = Fake {
            script: Mutex::new(VecDeque::from([
                Step::Data(vec![1]),
                Step::Data(vec![2]),
                Step::Panic,
            ])),
            ..Fake::idle()
        };
        let t = start(recv, Fake::idle(), 8);
        let mut got = Vec::new();
        wait_until("the receive thread to die", || match t.receive() {
            Ok(Some(d)) => {
                got.push(d);
                false
            }
            Ok(None) => false,
            Err(Closed) => true,
        });
        assert_eq!(got, [vec![1], vec![2]]);
        assert_eq!(t.receive(), Err(Closed));
    }
}
