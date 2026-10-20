//! The reader half of a connection: parses client messages, enforces the size limits and registers
//! the engine subscription (REQ-X-6, ADR 0027 m9).
//!
//! It never writes to the socket. Replies, errors and close requests go to the writer, which owns
//! the sink, so a send in progress is never cancelled or interleaved.

use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::Message;
use futures_util::{Stream, StreamExt};
use serde_json::Value;
use tokio::sync::{mpsc, watch};
use umbra_api_types as api;
use umbra_engine::bus::SubscribeOptions;

use super::filter::Filter;
use super::{
    Active, CloseReason, MAX_ENTITY_IDS, MAX_MESSAGE_BYTES, ReadFailure, Shared, ToWriter,
};

/// Reads messages until the client leaves, a limit is exceeded or the writer is gone. Dropping the
/// sender on return tells the writer the client is done.
pub(super) async fn run<St, E>(
    mut stream: St,
    shared: Arc<Shared>,
    to_writer: mpsc::Sender<ToWriter>,
    mailbox: watch::Sender<Option<Pending>>,
) where
    St: Stream<Item = Result<Message, E>> + Unpin + Send,
    E: Into<ReadFailure> + Send,
{
    while let Some(item) = stream.next().await {
        let message = match item {
            Ok(message) => message,
            Err(error) => {
                match error.into() {
                    ReadFailure::TooBig => close(&to_writer, CloseReason::MessageTooBig).await,
                    other => {
                        tracing::debug!(error = %other, "stream read failed");
                        false
                    }
                };
                return;
            }
        };
        let keep_going = match message {
            Message::Text(text) => on_text(text.as_str(), &shared, &to_writer, &mailbox).await,
            Message::Binary(_) => {
                reply_error(
                    &to_writer,
                    api::StreamErrorCode::InvalidMessage,
                    "binary messages are not supported; send JSON text",
                )
                .await
            }
            Message::Ping(_) | Message::Pong(_) => true,
            Message::Close(_) => false,
        };
        if !keep_going {
            return;
        }
    }
}

/// Handles one text message; `false` when the connection should stop reading.
async fn on_text(
    text: &str,
    shared: &Shared,
    to_writer: &mpsc::Sender<ToWriter>,
    mailbox: &watch::Sender<Option<Pending>>,
) -> bool {
    if text.len() > MAX_MESSAGE_BYTES {
        return close(to_writer, CloseReason::MessageTooBig).await;
    }
    let subscription = match parse(text) {
        Ok(subscription) => subscription,
        Err((code, message)) => return reply_error(to_writer, code, &message).await,
    };
    let prepared = match prepare(&subscription, shared) {
        Ok(prepared) => prepared,
        Err(message) => {
            return reply_error(
                to_writer,
                api::StreamErrorCode::InvalidSubscription,
                &message,
            )
            .await;
        }
    };
    // The newest checked `subscribe` replaces an older one the writer has not taken yet.
    mailbox.send_replace(Some(Pending { prepared }));
    true
}

/// Asks the writer to close the connection; always `false`: the reader stops.
async fn close(to_writer: &mpsc::Sender<ToWriter>, reason: CloseReason) -> bool {
    // If the writer is gone the connection is closing anyway.
    drop(to_writer.send(ToWriter::Close(reason)).await);
    false
}

async fn reply_error(
    to_writer: &mpsc::Sender<ToWriter>,
    code: api::StreamErrorCode,
    message: &str,
) -> bool {
    let error = api::StreamError {
        code,
        message: message.to_owned(),
    };
    to_writer.send(ToWriter::Error(error)).await.is_ok()
}

/// The `subscribe` a text message holds.
fn parse(text: &str) -> Result<api::Subscription, (api::StreamErrorCode, String)> {
    let invalid = |message: String| (api::StreamErrorCode::InvalidMessage, message);
    let value: Value = serde_json::from_str(text)
        .map_err(|error| invalid(format!("the message is not valid JSON: {error}")))?;
    match value.get("type").and_then(Value::as_str) {
        Some("subscribe") => match serde_json::from_value::<api::ClientMessage>(value) {
            Ok(api::ClientMessage::Subscribe(subscription)) => Ok(subscription),
            Err(error) => Err(invalid(format!("not a valid `subscribe`: {error}"))),
        },
        Some(other) => Err((
            api::StreamErrorCode::UnknownMessageType,
            format!("unknown message type `{other}`; the only client message is `subscribe`"),
        )),
        None => Err(invalid(
            "a message is a JSON object with a string `type`".to_owned(),
        )),
    }
}

/// A checked `subscribe`, waiting for the writer to register it with the engine.
#[derive(Clone)]
pub(super) struct Pending {
    prepared: Prepared,
}

/// A checked `subscribe`.
#[derive(Clone)]
struct Prepared {
    filter: Arc<Filter>,
    rate_hz: f64,
    state_hash: bool,
}

/// Checks the rate and the filter against the limits in force. The message names the field and the
/// value the client wrote, or says the rate is the configured default.
fn prepare(subscription: &api::Subscription, shared: &Shared) -> Result<Prepared, String> {
    let settings = &shared.settings;
    let rate_hz = match subscription.max_rate_hz {
        Some(rate) if rate.is_finite() && rate > 0.0 && rate <= settings.max_rate_hz => rate,
        Some(rate) => {
            return Err(format!(
                "`max_rate_hz={rate}` is out of range: it must be above 0 and at most {}",
                settings.max_rate_hz
            ));
        }
        None => settings.default_rate_hz,
    };
    let filter = Filter::from_wire(subscription.filter.as_ref(), MAX_ENTITY_IDS)?;
    Ok(Prepared {
        filter: Arc::new(filter),
        rate_hz,
        state_hash: subscription.state_hash.unwrap_or(false),
    })
}

/// The engine subscription for a checked `subscribe`: capacity `queue_frames`, the rate-following
/// stride `max(1, tick_rate_hz / rate)` the bus derives from the tick rate of the frames it
/// publishes (ASM-165, ASM-212), carry cap `max_events_per_frame`. The engine primes it with a
/// control frame.
pub(super) fn register(pending: Pending, shared: &Shared) -> Active {
    let Pending { prepared } = pending;
    let settings = &shared.settings;
    let mut options = SubscribeOptions::new();
    // A zero in a hand-built `StreamConfig` (the file reader refuses it) would be a capacity of one.
    options.capacity = NonZeroUsize::new(settings.queue_frames).unwrap_or(NonZeroUsize::MIN);
    options.max_rate_hz = Some(prepared.rate_hz);
    options.max_carry_events = settings.max_events_per_frame;
    options.with_state_hash = prepared.state_hash;
    let notify = Arc::clone(&shared.notify);
    options.notify = Some(Arc::new(move || notify.notify_one()));
    Active {
        subscription: Arc::new(shared.handle.subscribe(options)),
        filter: prepared.filter,
        min_interval: Duration::from_secs_f64(1.0 / prepared.rate_hz),
        state_hash: prepared.state_hash,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_tells_invalid_json_unknown_types_and_bad_subscribes_apart() {
        let code = |text: &str| parse(text).expect_err(text).0;
        assert_eq!(code("nope"), api::StreamErrorCode::InvalidMessage);
        assert_eq!(code("[1]"), api::StreamErrorCode::InvalidMessage);
        assert_eq!(code(r#"{"type":1}"#), api::StreamErrorCode::InvalidMessage);
        assert_eq!(
            code(r#"{"type":"bogus"}"#),
            api::StreamErrorCode::UnknownMessageType
        );
        assert_eq!(
            code(r#"{"type":"subscribe","max_rate_hz":"fast"}"#),
            api::StreamErrorCode::InvalidMessage
        );
        assert_eq!(
            code(r#"{"type":"subscribe","extra":1}"#),
            api::StreamErrorCode::InvalidMessage,
            "unknown fields are refused"
        );
        let ok = parse(r#"{"type":"subscribe","max_rate_hz":10}"#).expect("subscribe");
        assert_eq!(ok.max_rate_hz, Some(10.0));
    }
}
