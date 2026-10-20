//! The WebSocket stream (REQ-X-6, ADR 0027): the upgrade, encoding negotiation, and the connection
//! loop over a generic sink and stream, split into a reader half and a writer half.
//!
//! The slow-client policy (the bounded queue, the send timeout and the `too_slow` close, the
//! counters behind `Server::stream_stats`) lives in `connection` and `writer` (WI-16).

mod connection;
mod encode;
mod filter;
mod reader;
pub(crate) mod upgrade;
mod writer;

use std::sync::Arc;

use tokio::sync::Notify;
use umbra_api_types as api;
use umbra_engine::StreamConfig;
use umbra_engine::bus::Subscription;
use umbra_engine::handle::EngineHandle;

pub(crate) use connection::LiveStats;
pub use connection::{CloseReason, ConnectionParams, ConnectionStats, ReadFailure, run_connection};

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
    /// Close the connection for this reason.
    Close(CloseReason),
}

/// The subscription a connection currently has.
struct Active {
    /// Shared with the connection's [`LiveStats`], which reads its counters.
    subscription: Arc<Subscription>,
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
    /// The connection's counters.
    live: Arc<LiveStats>,
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
