//! The WebSocket stream (REQ-X-6, ADR 0027): encoding negotiation, the connection loop over a generic
//! sink and stream, and its test probes.

#![expect(
    clippy::todo,
    unused_variables,
    reason = "WI-6 signature stubs; the bodies arrive in WI-15 and WI-16"
)]

use axum::extract::ws::Message;
use futures_util::{Sink, Stream};
use umbra_engine::StreamConfig;
use umbra_engine::handle::EngineHandle;

use crate::shutdown::Shutdown;

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
/// never cancelled mid-flight; one that exceeds `send_timeout` closes with 1008 `too_slow`.
pub async fn run_connection<Si, St, E>(sink: Si, stream: St, params: ConnectionParams)
where
    Si: Sink<Message> + Unpin + Send + 'static,
    Si::Error: std::fmt::Display + Send,
    St: Stream<Item = Result<Message, E>> + Unpin + Send + 'static,
    E: std::fmt::Display + Send,
{
    todo!("WI-15")
}
