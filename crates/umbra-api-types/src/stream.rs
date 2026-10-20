//! Stream messages (REQ-X-6, ADR 0027): `hello`, `frame` and `error` from the server, `subscribe` from
//! the client, all discriminated by `type`.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

use crate::entities::{EntitySummary, GeoBox};
use crate::status::RunStatus;

/// The stream protocol name carried by `hello`.
pub const STREAM_PROTOCOL: &str = "umbra.stream.v1";

/// What a client may ask the stream for.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct StreamFilter {
    /// Only entities inside the box (and events of them).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bbox: Option<GeoBox>,
    /// Only these entities, at most 10,000.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity_ids: Option<Vec<u64>>,
    /// Only events of these kinds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_kinds: Option<Vec<String>>,
}

/// The body of a `subscribe` message; a new one replaces the previous one. Unknown fields are
/// refused.
// Reading goes through the private strict mirror `SubscriptionWire`, so the derived schema carries
// no `additionalProperties: false`: that would forbid the `type` tag next to it, and no message
// would satisfy it. `umbra_server::contract` adds `unevaluatedProperties: false` to the tagged
// wrapper instead (ASM-208).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(from = "SubscriptionWire", into = "SubscriptionWire")]
pub struct Subscription {
    /// The filter; default: everything.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter: Option<StreamFilter>,
    /// The most frames per second, in `(0, 60]`; default 30.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_rate_hz: Option<f64>,
    /// Whether frames carry `state_hash`; default false.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_hash: Option<bool>,
}

/// The serde form of [`Subscription`]: the same fields, strict.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SubscriptionWire {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    filter: Option<StreamFilter>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    max_rate_hz: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    state_hash: Option<bool>,
}

impl From<SubscriptionWire> for Subscription {
    fn from(wire: SubscriptionWire) -> Self {
        Self {
            filter: wire.filter,
            max_rate_hz: wire.max_rate_hz,
            state_hash: wire.state_hash,
        }
    }
}

impl From<Subscription> for SubscriptionWire {
    fn from(subscription: Subscription) -> Self {
        Self {
            filter: subscription.filter,
            max_rate_hz: subscription.max_rate_hz,
            state_hash: subscription.state_hash,
        }
    }
}

/// Client -> server messages. There is no unsubscribe: a client that is done closes the socket.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    /// Starts or replaces the subscription.
    Subscribe(Subscription),
}

/// The stream settings in force, reported by `hello`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct StreamSettings {
    /// The rate used when `subscribe` gives none.
    pub default_rate_hz: f64,
    /// The highest rate a subscription may ask for.
    pub max_rate_hz: f64,
    /// Frames queued per connection before the oldest is dropped.
    pub queue_frames: u32,
    /// Events per frame before `events_truncated` is set.
    pub max_events_per_frame: u32,
    /// A send that takes longer closes the connection (1008 `too_slow`).
    pub send_timeout_s: f64,
    /// The longest accepted client message (1009 beyond it).
    pub max_message_bytes: u32,
    /// The most `entity_ids` in one subscription.
    pub max_entity_ids: u32,
}

/// The first message of a connection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Hello {
    /// [`STREAM_PROTOCOL`].
    pub protocol: String,
    /// The contract version.
    pub api_version: String,
    /// The negotiated encoding (`json`).
    pub encoding: String,
    /// The settings in force.
    pub settings: StreamSettings,
}

/// The kind of a frame. Only full snapshots exist in v1; `delta` is reserved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum FrameKind {
    /// The complete filtered state of one tick.
    Snapshot,
}

/// What made the engine publish a frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum FrameCause {
    /// A tick ran.
    Tick,
    /// A state-changing command, a step completion, a pause, a stop, or priming.
    Control,
}

/// An event on the stream (best-effort; `dropped` and `events_truncated` make loss visible).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct EventDto {
    /// The tick it happened in.
    pub tick: u64,
    /// The kind: `entity_created`, `entity_removed`, `fire`, `custom` or `simulation_control`.
    pub kind: String,
    /// The main entity: created, removed, shooter or source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity_id: Option<u64>,
    /// The target of a `fire`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_id: Option<u64>,
    /// The template of an `entity_created`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
    /// The weapon of a `fire`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weapon: Option<String>,
    /// A `custom` event: `{"kind": <component id>, "payload": <the payload>}`. A
    /// `simulation_control`: `{"action": "start" | "pause" | "resume" | "stop" | "step"}`, with
    /// `"count"` for `step`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<Object>)]
    pub detail: Option<Value>,
}

/// One frame: the newest state of the subscription.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Frame {
    /// [`FrameKind::Snapshot`].
    pub kind: FrameKind,
    /// What caused it.
    pub cause: FrameCause,
    /// The session number.
    pub session: u64,
    /// The number of the frame in its subscription, from 0 at the priming frame. Between two sent
    /// frames it grows by `dropped + 1`, so a gap is a drop.
    pub seq: u64,
    /// The tick of the snapshot.
    pub tick: u64,
    /// Simulation time in seconds.
    pub time_s: f64,
    /// The run status.
    pub run: RunStatus,
    /// Frames the connection never sent since the previous frame.
    pub dropped: u64,
    /// The filtered entities, ascending by id.
    pub entities: Vec<EntitySummary>,
    /// The events since the previous frame, in order.
    pub events: Vec<EventDto>,
    /// Whether events were lost beyond the per-frame cap.
    pub events_truncated: bool,
    /// The state hash (16 lower-case hex digits), when the subscription asked for it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_hash: Option<String>,
}

/// Stable codes of stream `error` messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum StreamErrorCode {
    /// The message is not valid JSON of a known shape.
    InvalidMessage,
    /// The `type` is not known; the message is ignored.
    UnknownMessageType,
    /// A field is out of range (rate, `entity_ids`).
    InvalidSubscription,
}

/// A problem with a client message; the connection stays open.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct StreamError {
    /// The stable code.
    pub code: StreamErrorCode,
    /// A message for a person.
    pub message: String,
}

/// Server -> client messages.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    /// The first message.
    Hello(Hello),
    /// A frame.
    Frame(Frame),
    /// A problem with a client message.
    Error(StreamError),
}

/// Any stream message; the root of `api/stream.schema.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(untagged)]
pub enum StreamMessage {
    /// A client message.
    Client(ClientMessage),
    /// A server message.
    Server(ServerMessage),
}
