//! The writer half of a connection: owns the sink, sends `hello`, then the newest frame of the
//! subscription at its rate (REQ-X-6, REQ-X-7, ADR 0027).
//!
//! The loop waits on a `select!` whose arms only *decide* what to do; every send happens in the arm
//! body, after the other futures were dropped, so no `select!` ever cancels a send in progress. The
//! frames are built and encoded on `spawn_blocking`; no engine or bus lock is held across an
//! `.await` (the subscription is drained synchronously, then the lock is gone).
//!
//! Slow clients (WI-16): every send, `hello`, frames, errors and the close, is bounded by
//! `send_timeout_s`. A send that exceeds it ends the connection with [`CloseReason::TooSlow`]; the
//! unfinished send is dropped together with the connection, never in favour of another send.

use std::ops::ControlFlow;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{CloseFrame, Message};
use futures_util::{FutureExt, Sink, SinkExt};
use tokio::sync::{mpsc, watch};
use tokio::time::{Instant, MissedTickBehavior, interval_at, sleep_until};
use umbra_api_types as api;

use super::encode::{Delivery, Wanted, build_frame, to_text};
use super::reader::{Pending, register};
use super::{Active, CloseReason, Encoding, MAX_ENTITY_IDS, MAX_MESSAGE_BYTES, Shared, ToWriter};

/// How often an otherwise quiet connection looks at the engine's closed flag. Reading a flag costs
/// nothing; the engine also rings every subscription as it ends (ASM-206).
const LIVENESS_PERIOD: Duration = Duration::from_secs(2);

/// What woke the loop.
enum Wake {
    Shutdown,
    Inbox(Option<ToWriter>),
    /// The reader left a `subscribe` in the mailbox (`false`: the reader is gone).
    Mailbox(bool),
    Notified,
    Due,
    Liveness,
}

/// Per-connection sending state.
struct Writer {
    active: Option<Active>,
    /// Frames offered to the current subscription so far, delivered or dropped: the next `seq`.
    offered: u64,
    /// The earliest time the next frame may be sent (the rate limit).
    next_allowed: Instant,
    /// Whether the subscription has signalled frames since the last send.
    wanted: bool,
}

/// The sink, and how long one send on it may take.
struct Out<Si> {
    sink: Si,
    limit: Duration,
}

/// How a send ended.
enum Sent {
    Done,
    /// The sink reported an error: the client is gone.
    Failed,
    /// The send did not complete within the limit.
    TimedOut,
}

impl<Si> Out<Si>
where
    Si: Sink<Message> + Unpin + Send,
    Si::Error: std::fmt::Display + Send,
{
    /// Sends one message, waiting at most the limit. A send is never given up for any other reason.
    async fn send(&mut self, message: Message) -> Sent {
        match tokio::time::timeout(self.limit, self.sink.send(message)).await {
            Ok(Ok(())) => Sent::Done,
            Ok(Err(error)) => {
                tracing::debug!(%error, "stream send failed");
                Sent::Failed
            }
            Err(_) => Sent::TimedOut,
        }
    }

    /// Sends `message`; the connection goes on unless the send failed or timed out.
    async fn send_or_end(&mut self, message: Message) -> ControlFlow<CloseReason> {
        match self.send(message).await {
            Sent::Done => ControlFlow::Continue(()),
            Sent::Failed => ControlFlow::Break(CloseReason::SendFailed),
            Sent::TimedOut => {
                // One try that waits for nothing: a sink that is merely slow may take the close
                // frame, one that is stalled does not, and then the connection is just dropped
                // (ASM-214).
                if let Some((code, text)) = CloseReason::TooSlow.close_frame() {
                    let frame = CloseFrame {
                        code,
                        reason: text.into(),
                    };
                    drop(self.sink.send(Message::Close(Some(frame))).now_or_never());
                }
                ControlFlow::Break(CloseReason::TooSlow)
            }
        }
    }

    /// Sends the close frame for `reason` (if it has one) and ends the connection. A close that
    /// cannot be sent in time changes nothing: the connection ends for `reason` anyway.
    async fn close(&mut self, reason: CloseReason) -> ControlFlow<CloseReason> {
        if let Some((code, text)) = reason.close_frame() {
            let frame = CloseFrame {
                code,
                reason: text.into(),
            };
            self.send(Message::Close(Some(frame))).await;
        }
        ControlFlow::Break(reason)
    }

    /// An error of ours (encoding, a panicked blocking task): log it and close with 1011.
    async fn internal(&mut self, error: String) -> ControlFlow<CloseReason> {
        tracing::error!(%error, "stream frame could not be produced");
        self.close(CloseReason::Internal).await
    }
}

/// Runs the writer until the connection closes, then returns why; the caller stops the reader.
pub(super) async fn run<Si>(
    sink: Si,
    mut inbox: mpsc::Receiver<ToWriter>,
    mut mailbox: watch::Receiver<Option<Pending>>,
    shared: &Shared,
    encoding: Encoding,
) -> CloseReason
where
    Si: Sink<Message> + Unpin + Send,
    Si::Error: std::fmt::Display + Send,
{
    let mut out = Out {
        sink,
        limit: shared.settings.send_timeout,
    };
    if let ControlFlow::Break(reason) = out.send_or_end(hello(shared, encoding)).await {
        return reason;
    }
    let mut state = Writer {
        active: None,
        offered: 0,
        next_allowed: Instant::now(),
        wanted: false,
    };
    let mut liveness = interval_at(Instant::now() + LIVENESS_PERIOD, LIVENESS_PERIOD);
    liveness.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut mailbox_open = true;
    loop {
        let wake = {
            let pace = async {
                if state.wanted {
                    sleep_until(state.next_allowed).await;
                } else {
                    std::future::pending::<()>().await;
                }
            };
            tokio::select! {
                biased;
                () = shared.shutdown.triggered() => Wake::Shutdown,
                item = inbox.recv() => Wake::Inbox(item),
                changed = mailbox.changed(), if mailbox_open => Wake::Mailbox(changed.is_ok()),
                // Not while a frame is already wanted: a busy engine would ring again before the
                // pacing timer is polled, and the biased order would starve it.
                () = shared.notify.notified(), if !state.wanted => Wake::Notified,
                () = pace => Wake::Due,
                _ = liveness.tick() => Wake::Liveness,
            }
        };
        let flow = match wake {
            Wake::Shutdown => out.close(CloseReason::GoingAway).await,
            // The reader is gone: the client closed, or a read failed. Finish the handshake.
            Wake::Inbox(None) => {
                drop(tokio::time::timeout(out.limit, out.sink.close()).await);
                ControlFlow::Break(CloseReason::ClientGone)
            }
            Wake::Inbox(Some(message)) => on_inbox(&mut out, message).await,
            Wake::Mailbox(open) => {
                mailbox_open = open;
                // The newest `subscribe` only; one still in the mailbox when we got here is the one
                // that counts. The old subscription drops when the new one replaces it.
                let pending = mailbox.borrow_and_update().clone();
                if let Some(pending) = pending {
                    subscribe(&mut state, pending, shared);
                }
                ControlFlow::Continue(())
            }
            Wake::Notified => {
                state.wanted = true;
                check_engine(&mut out, shared).await
            }
            Wake::Due => match check_engine(&mut out, shared).await {
                ControlFlow::Continue(()) => send_newest(&mut out, &mut state, shared).await,
                stop @ ControlFlow::Break(_) => stop,
            },
            Wake::Liveness => check_engine(&mut out, shared).await,
        };
        if let ControlFlow::Break(reason) = flow {
            return reason;
        }
    }
}

/// Registers the `subscribe` taken from the mailbox. The first frame must not wait for the old
/// subscription's pacing, and its numbering starts again.
fn subscribe(state: &mut Writer, pending: Pending, shared: &Shared) {
    let active = register(pending, shared);
    let current = Arc::clone(&active.subscription);
    // The new subscription first: the counters still hold the old one until `replace` folds it in,
    // so nothing is lost between the two steps.
    state.active = Some(active);
    shared.live.replace(Some(current));
    state.offered = 0;
    state.next_allowed = Instant::now();
    state.wanted = true;
}

async fn on_inbox<Si>(out: &mut Out<Si>, message: ToWriter) -> ControlFlow<CloseReason>
where
    Si: Sink<Message> + Unpin + Send,
    Si::Error: std::fmt::Display + Send,
{
    match message {
        ToWriter::Error(error) => match to_text(&api::ServerMessage::Error(error)) {
            Ok(text) => out.send_or_end(Message::Text(text.into())).await,
            Err(error) => out.internal(error.to_string()).await,
        },
        ToWriter::Close(reason) => out.close(reason).await,
    }
}

/// Drains the subscription and sends its newest frame, with the events of the others.
async fn send_newest<Si>(
    out: &mut Out<Si>,
    state: &mut Writer,
    shared: &Shared,
) -> ControlFlow<CloseReason>
where
    Si: Sink<Message> + Unpin + Send,
    Si::Error: std::fmt::Display + Send,
{
    state.wanted = false;
    let Some(active) = &state.active else {
        return ControlFlow::Continue(());
    };
    let drained = active.subscription.drain();
    if drained.frames.is_empty() {
        return ControlFlow::Continue(());
    }
    let skipped = u64::try_from(drained.frames.len() - 1).unwrap_or(u64::MAX);
    shared.live.add_skipped(skipped);
    let dropped = drained.dropped.saturating_add(skipped);
    let seq = state.offered.saturating_add(dropped);
    state.offered = seq.saturating_add(1);
    let delivery = Delivery {
        frames: drained.frames,
        carried_events: drained.carried_events,
        carried_ticks: drained.carried_ticks,
        bus_truncated: drained.events_truncated,
        dropped,
        seq,
    };
    let wanted = Wanted {
        filter: std::sync::Arc::clone(&active.filter),
        max_events: shared.settings.max_events_per_frame,
        state_hash: active.state_hash,
    };
    let interval = active.min_interval;
    // Filtering, mapping and JSON encoding of a frame that may be megabytes: off the runtime.
    let encoded = tokio::task::spawn_blocking(move || {
        build_frame(&delivery, &wanted).map(|frame| to_text(&api::ServerMessage::Frame(frame)))
    })
    .await;
    match encoded {
        Ok(Some(Ok(text))) => {
            let flow = out.send_or_end(Message::Text(text.into())).await;
            if flow.is_continue() {
                shared.live.add_sent();
            }
            state.next_allowed = Instant::now() + interval;
            flow
        }
        Ok(Some(Err(error))) => out.internal(error.to_string()).await,
        Ok(None) => ControlFlow::Continue(()),
        Err(error) => out.internal(error.to_string()).await,
    }
}

/// Closes with 1011 if the engine thread has ended. It reads a flag: no command, no waiting.
async fn check_engine<Si>(out: &mut Out<Si>, shared: &Shared) -> ControlFlow<CloseReason>
where
    Si: Sink<Message> + Unpin + Send,
    Si::Error: std::fmt::Display + Send,
{
    if shared.handle.is_closed() {
        out.close(CloseReason::EngineUnavailable).await
    } else {
        ControlFlow::Continue(())
    }
}

fn hello(shared: &Shared, encoding: Encoding) -> Message {
    let settings = &shared.settings;
    let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    let message = api::ServerMessage::Hello(api::Hello {
        protocol: api::STREAM_PROTOCOL.to_owned(),
        api_version: api::API_VERSION.to_owned(),
        encoding: encoding.as_str().to_owned(),
        settings: api::StreamSettings {
            default_rate_hz: settings.default_rate_hz,
            max_rate_hz: settings.max_rate_hz,
            queue_frames: count(settings.queue_frames),
            max_events_per_frame: count(settings.max_events_per_frame),
            send_timeout_s: settings.send_timeout.as_secs_f64(),
            max_message_bytes: count(MAX_MESSAGE_BYTES),
            max_entity_ids: count(MAX_ENTITY_IDS),
        },
    });
    // These types serialise to JSON without failing; the fallback is a text no client accepts, and
    // is never reached.
    Message::Text(to_text(&message).unwrap_or_default().into())
}
