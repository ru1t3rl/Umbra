//! The WebSocket stream (REQ-X-6, ADR 0027): the upgrade, encoding negotiation, and the connection
//! loop over a generic sink and stream, split into a reader half and a writer half.
//!
//! The slow-client policy (the send timeout and the `too_slow` close, the bounded queue counters
//! and `Server::stream_stats`) arrives with WI-16 on top of the halves built here.

mod encode;
mod filter;
mod reader;
pub(crate) mod upgrade;
mod writer;

use std::sync::Arc;

use axum::extract::ws::Message;
use futures_util::{Sink, Stream};
use tokio::sync::{Notify, mpsc, watch};
use umbra_api_types as api;
use umbra_engine::StreamConfig;
use umbra_engine::bus::Subscription;
use umbra_engine::handle::EngineHandle;

use crate::shutdown::Shutdown;
use filter::Filter;

/// The longest accepted client message, in bytes; a longer one closes the connection with 1009
/// (ADR 0027 m9).
pub const MAX_MESSAGE_BYTES: usize = 64 * 1024;

/// The most `entity_ids` one `subscribe` may list (ADR 0027 m9).
pub const MAX_ENTITY_IDS: usize = 10_000;

/// Messages from the reader to the writer, which owns the sink. A `subscribe` does not travel here:
/// it goes through the single-slot mailbox ([`reader::Pending`]), so a burst of them collapses to the
/// newest and at most one engine subscription is live (ASM-211).
enum ToWriter {
    /// A problem with a client message (`error`); the connection stays open.
    Error(api::StreamError),
    /// Close the connection with this code and reason.
    Close(u16, &'static str),
}

/// The subscription a connection currently has.
struct Active {
    subscription: Subscription,
    filter: Arc<Filter>,
    /// The shortest time between two sent frames: `1 / max_rate_hz`.
    min_interval: std::time::Duration,
    state_hash: bool,
}

/// What the two halves share.
struct Shared {
    handle: EngineHandle,
    settings: StreamConfig,
    /// Rung by the engine bus after every push to the connection's subscription.
    notify: Arc<Notify>,
    shutdown: Shutdown,
}

/// Close code: the server is shutting down.
const CLOSE_GOING_AWAY: u16 = 1001;
/// Close code: a client message was over the size limit.
const CLOSE_MESSAGE_TOO_BIG: u16 = 1009;
/// Close code: the engine thread is gone.
const CLOSE_ENGINE_GONE: u16 = 1011;
/// Close code: a frame could not be produced.
const CLOSE_INTERNAL: u16 = 1011;

/// What the stream of a real socket reports when it refuses a message for its size (the library's
/// read cap). Its `Display` is the sentinel `run_connection` recognises to close with 1009; any
/// other error just ends the connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ReadFailure {
    /// A message above the library's read cap.
    TooBig,
    /// Any other read failure.
    Other(String),
}

/// The `Display` text of [`ReadFailure::TooBig`].
const READ_TOO_BIG: &str = "message too big";

impl std::fmt::Display for ReadFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooBig => f.write_str(READ_TOO_BIG),
            Self::Other(text) => f.write_str(text),
        }
    }
}

/// The negotiated stream encoding. Only `json` exists in v1; `msgpack` is refused with
/// `400 unsupported_encoding`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Encoding {
    /// One JSON object per text message.
    Json,
}

impl Encoding {
    /// The value of the `encoding` query parameter: absent means `json`.
    ///
    /// # Errors
    /// The unsupported value, to be named in the `unsupported_encoding` problem.
    pub fn parse(requested: Option<&str>) -> Result<Self, String> {
        match requested {
            None | Some("json") => Ok(Self::Json),
            Some(other) => Err(other.to_string()),
        }
    }

    /// The query-parameter spelling.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Json => "json",
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
        }
    }
}

/// Counters of one connection, for tests (`Server::stream_stats`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct ConnectionStats {
    /// The longest the engine-side queue has been.
    pub max_queue_len: usize,
    /// Frames the engine bus dropped for this connection.
    pub bus_dropped: u64,
    /// Frames the writer skipped because a newer one was queued.
    pub skipped: u64,
    /// Frames sent.
    pub sent: u64,
}

/// Runs one stream connection until it closes (WI-15, WI-16): a reader half handling `subscribe`
/// and the size limits, a writer half sending the newest frame at the subscription's rate. A send is
/// never cancelled mid-flight. The send timeout and the `too_slow` close arrive with WI-16.
pub async fn run_connection<Si, St, E>(sink: Si, stream: St, params: ConnectionParams)
where
    Si: Sink<Message> + Unpin + Send + 'static,
    Si::Error: std::fmt::Display + Send,
    St: Stream<Item = Result<Message, E>> + Unpin + Send + 'static,
    E: std::fmt::Display + Send,
{
    let shared = Arc::new(Shared {
        handle: params.handle,
        settings: params.settings,
        notify: Arc::new(Notify::new()),
        shutdown: params.shutdown,
    });
    let (to_writer, inbox) = mpsc::channel(CONTROL_QUEUE);
    let (mailbox_tx, mailbox_rx) = watch::channel(None);
    // Both halves run on this task, so nothing has to be `'static`. The reader ends first when the
    // client leaves or breaks a limit; dropping its sender then ends the writer, after any send
    // in progress. When the writer ends first (a close it sent, the engine gone), the reader is
    // dropped: nothing is left to read for.
    let reader = reader::run(stream, Arc::clone(&shared), to_writer, mailbox_tx);
    let writer = writer::run(sink, inbox, mailbox_rx, &shared, params.encoding);
    tokio::pin!(reader, writer);
    tokio::select! {
        () = &mut writer => {}
        () = &mut reader => writer.await,
    }
}

/// Errors and close requests the reader may have queued for the writer; a reader that is ahead waits.
const CONTROL_QUEUE: usize = 16;
