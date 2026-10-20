//! The connection loop over a generic sink and stream (REQ-X-6, REQ-X-7, ADR 0027), and the types
//! that describe a connection to its caller: the parameters, the close reason and the counters.
//!
//! The loop is two futures on one task, a reader half and a writer half. The slow-client policy
//! (WI-16) is in the writer: a bounded engine queue, only the newest frame sent, and a send timeout
//! that ends the connection (`too_slow`) instead of waiting for a client that does not read.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use axum::extract::ws::Message;
use futures_util::{Sink, Stream};
use tokio::sync::{Notify, mpsc, watch};
use umbra_engine::StreamConfig;
use umbra_engine::bus::Subscription;
use umbra_engine::handle::EngineHandle;

use super::{Encoding, Shared, reader, writer};
use crate::shutdown::Shutdown;

/// What the stream of a real socket reports when a read fails. A typed error, so that no `Display`
/// text decides a close code (ASM-213).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ReadFailure {
    /// A message above the library's read cap: the connection closes with 1009.
    TooBig,
    /// Any other read failure: the connection just ends.
    Other(String),
}

impl std::fmt::Display for ReadFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooBig => f.write_str("message too big"),
            Self::Other(text) => f.write_str(text),
        }
    }
}

impl From<String> for ReadFailure {
    fn from(text: String) -> Self {
        Self::Other(text)
    }
}

/// Why a connection ended (the second value `run_connection` returns).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CloseReason {
    /// The client closed, or its stream ended or failed.
    ClientGone,
    /// A send failed: the sink is closed.
    SendFailed,
    /// The server is shutting down: close code 1001 `going_away`.
    GoingAway,
    /// A client message was over the limit: close code 1009 `message_too_big`.
    MessageTooBig,
    /// The engine thread has ended: close code 1011 `engine_unavailable`.
    EngineUnavailable,
    /// A frame could not be produced: close code 1011 `internal`.
    Internal,
    /// A send did not complete within `send_timeout_s`: close code 1008 `too_slow`. The close frame
    /// itself cannot pass a blocked sink, so the connection is dropped (ASM-214).
    TooSlow,
}

impl CloseReason {
    /// The WebSocket close code and reason text the server sends for this reason; `None` if it sends
    /// none (the client is already gone).
    #[must_use]
    pub fn close_frame(self) -> Option<(u16, &'static str)> {
        match self {
            Self::ClientGone | Self::SendFailed => None,
            Self::GoingAway => Some((1001, "going_away")),
            Self::MessageTooBig => Some((1009, "message_too_big")),
            Self::EngineUnavailable => Some((1011, "engine_unavailable")),
            Self::Internal => Some((1011, "internal")),
            Self::TooSlow => Some((1008, "too_slow")),
        }
    }
}

/// What one connection needs.
#[non_exhaustive]
pub struct ConnectionParams {
    /// The engine to subscribe to.
    pub handle: EngineHandle,
    /// The stream settings in force.
    pub settings: StreamConfig,
    /// Closes the connection with 1001 when it fires.
    pub shutdown: Shutdown,
    /// The negotiated encoding.
    pub encoding: Encoding,
    /// Where the counters are kept, when the server lists them (`Server::stream_stats`).
    pub(crate) live: Option<Arc<LiveStats>>,
}

impl ConnectionParams {
    /// Parameters for one connection.
    #[must_use]
    pub fn new(
        handle: EngineHandle,
        settings: StreamConfig,
        shutdown: Shutdown,
        encoding: Encoding,
    ) -> Self {
        Self {
            handle,
            settings,
            shutdown,
            encoding,
            live: None,
        }
    }

    /// The same parameters with counters the caller can read while the connection runs.
    #[must_use]
    pub(crate) fn with_live(mut self, live: Arc<LiveStats>) -> Self {
        self.live = Some(live);
        self
    }
}

/// Counters of one connection, for tests (`Server::stream_stats`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct ConnectionStats {
    /// The longest an engine-side queue of this connection has been (over all its subscriptions).
    pub max_queue_len: usize,
    /// Frames the engine bus dropped for this connection.
    pub bus_dropped: u64,
    /// Frames the writer skipped because a newer one was queued.
    pub skipped: u64,
    /// Frames sent (not counting `hello`).
    pub sent: u64,
}

/// The counters of a connection while it runs. The writer owns the subscription; this keeps a
/// second `Arc` of it so the bus counters stay readable while the writer is blocked in a send.
#[derive(Default)]
pub(crate) struct LiveStats {
    skipped: AtomicU64,
    sent: AtomicU64,
    subscriptions: Mutex<Totals>,
}

/// The bus-side counters: those of subscriptions already replaced, and the current one.
#[derive(Default)]
struct Totals {
    max_queue_len: usize,
    bus_dropped: u64,
    current: Option<Arc<Subscription>>,
}

impl LiveStats {
    fn totals(&self) -> std::sync::MutexGuard<'_, Totals> {
        self.subscriptions
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Makes `next` the current subscription (or none), adding the counters of the one it replaces
    /// to the totals.
    pub(super) fn replace(&self, next: Option<Arc<Subscription>>) {
        let old = {
            let mut totals = self.totals();
            let old = std::mem::replace(&mut totals.current, next);
            if let Some(old) = &old {
                let stats = old.stats();
                totals.max_queue_len = totals.max_queue_len.max(stats.max_queue_len);
                totals.bus_dropped = totals.bus_dropped.saturating_add(stats.dropped_total);
            }
            old
        };
        // Dropped outside the lock: it unregisters from the bus.
        drop(old);
    }

    pub(super) fn add_skipped(&self, frames: u64) {
        self.skipped.fetch_add(frames, Ordering::Relaxed);
    }

    pub(super) fn add_sent(&self) {
        self.sent.fetch_add(1, Ordering::Relaxed);
    }

    /// The counters now.
    pub(crate) fn snapshot(&self) -> ConnectionStats {
        let totals = self.totals();
        let current = totals
            .current
            .as_ref()
            .map(|subscription| subscription.stats())
            .unwrap_or_default();
        ConnectionStats {
            max_queue_len: totals.max_queue_len.max(current.max_queue_len),
            bus_dropped: totals.bus_dropped.saturating_add(current.dropped_total),
            skipped: self.skipped.load(Ordering::Relaxed),
            sent: self.sent.load(Ordering::Relaxed),
        }
    }
}

/// Errors and close requests the reader may have queued for the writer; a reader that is ahead waits.
const CONTROL_QUEUE: usize = 16;

/// Runs one stream connection until it closes: a reader half handling `subscribe` and the size
/// limits, a writer half sending the newest frame at the subscription's rate. Returns the
/// connection's counters and why it ended.
///
/// A send is never cancelled because something else happened; only the send timeout
/// (`send_timeout_s`) ends one, and it ends the connection with [`CloseReason::TooSlow`].
///
/// The stream's error must convert into [`ReadFailure`] (a `String` does): `TooBig` closes the
/// connection with 1009, anything else ends it.
pub async fn run_connection<Si, St, E>(
    sink: Si,
    stream: St,
    mut params: ConnectionParams,
) -> (ConnectionStats, CloseReason)
where
    Si: Sink<Message> + Unpin + Send + 'static,
    Si::Error: std::fmt::Display + Send,
    St: Stream<Item = Result<Message, E>> + Unpin + Send + 'static,
    E: Into<ReadFailure> + Send,
{
    let live = params.live.take().unwrap_or_default();
    let shared = Arc::new(Shared {
        handle: params.handle,
        settings: params.settings,
        notify: Arc::new(Notify::new()),
        shutdown: params.shutdown,
        live: Arc::clone(&live),
    });
    let (to_writer, inbox) = mpsc::channel(CONTROL_QUEUE);
    let (mailbox_tx, mailbox_rx) = watch::channel(None);
    // Both halves run on this task, so nothing has to be `'static`. The reader ends first when the
    // client leaves or breaks a limit; dropping its sender then ends the writer, after any send
    // in progress. When the writer ends first (a close it sent, the engine gone, a send that timed
    // out), the reader is dropped: nothing is left to read for.
    let reader = reader::run(stream, Arc::clone(&shared), to_writer, mailbox_tx);
    let writer = writer::run(sink, inbox, mailbox_rx, &shared, params.encoding);
    tokio::pin!(reader, writer);
    let reason = tokio::select! {
        reason = &mut writer => reason,
        () = &mut reader => writer.await,
    };
    let stats = live.snapshot();
    live.replace(None);
    (stats, reason)
}
