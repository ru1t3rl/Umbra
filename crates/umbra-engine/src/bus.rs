//! The snapshot bus (REQ-K-6, REQ-X-7, ADR 0024): bounded per-subscriber frame queues that drop the
//! oldest frame instead of ever blocking the engine thread.
//!
//! WI-6 declares the types and the signatures; the real types ([`RunState`], [`RunStatus`],
//! [`Frame`], [`SubscribeOptions`], [`Drained`], [`SubscriptionStats`]) are final, the bodies of
//! [`Bus`] and [`Subscription`] are `todo!` until WI-7.

#![expect(
    clippy::todo,
    clippy::needless_pass_by_value,
    unused_variables,
    reason = "WI-6 signature stubs; the bodies arrive in WI-7"
)]

use std::collections::BTreeMap;
use std::num::{NonZeroU64, NonZeroUsize};
use std::sync::Arc;
use std::time::Duration;

use umbra_core::{EntityId, SimEvent, SimTime, StateHash, TickRate};
use umbra_kernel::{ClockMode, Snapshot};
use umbra_templates::TickRateSource;

use crate::plugins::PluginFault;
use crate::scenario::EntityInfo;

/// The run lifecycle state (ADR 0026).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RunState {
    /// No scenario is loaded.
    Idle,
    /// Loaded, paused at tick 0.
    Ready,
    /// Ticking.
    Running,
    /// Paused after running.
    Paused,
    /// Stopped; see [`StopReason`].
    Stopped,
}

/// Why a run is [`RunState::Stopped`].
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum StopReason {
    /// A `stop` command.
    Command,
    /// The scenario's duration was reached.
    Duration,
    /// A plugin fault under the `abort` policy (ADR 0021).
    PluginFault(PluginFault),
    /// A kernel error.
    Error(String),
}

/// The run status carried by every frame and reply.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct RunStatus {
    /// The lifecycle state.
    pub state: RunState,
    /// Why the run stopped; `Some` only when stopped.
    pub stop_reason: Option<StopReason>,
    /// The clock in force.
    pub clock: ClockMode,
    /// The session number; changes with every load, 0 before the first.
    pub session: u64,
    /// Simulation time; `None` without a scenario.
    pub time: Option<SimTime>,
    /// Whether a `step` is waiting for its ticks.
    pub step_pending: bool,
    /// The scenario's tick rate; `None` without a scenario.
    pub tick_rate: Option<TickRate>,
    /// Where the tick rate came from.
    pub tick_rate_source: Option<TickRateSource>,
}

impl RunStatus {
    /// A status in `state` with `clock` and `session`, and no stop reason, time, pending step or tick
    /// rate; set the public fields for the rest.
    #[must_use]
    pub fn new(state: RunState, clock: ClockMode, session: u64) -> Self {
        Self {
            state,
            stop_reason: None,
            clock,
            session,
            time: None,
            step_pending: false,
            tick_rate: None,
            tick_rate_source: None,
        }
    }
}

/// What made the engine thread publish a frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum FrameCause {
    /// A tick ran.
    Tick,
    /// A state change, a step completion, a pause, a stop, or priming.
    Control,
}

/// One published state of the world.
#[derive(Debug)]
#[non_exhaustive]
pub struct Frame {
    /// Frame counter of the bus; increases, so a subscriber sees gaps where it lost frames.
    pub seq: u64,
    /// What caused the frame.
    pub cause: FrameCause,
    /// The run status at publication.
    pub run: RunStatus,
    /// The world; `None` without a scenario.
    pub snapshot: Option<Snapshot>,
    /// Name, template and group of every entity, shared between frames while unchanged.
    pub directory: Arc<BTreeMap<EntityId, EntityInfo>>,
    /// The events since the previous frame of this cause (a tick's events for a tick frame).
    pub events: Vec<SimEvent>,
    /// The state hash, computed only when a due subscription asked for it.
    pub state_hash: Option<StateHash>,
}

impl Frame {
    /// A frame without world, directory, events or hash; set the public fields for the rest.
    #[must_use]
    pub fn new(seq: u64, cause: FrameCause, run: RunStatus) -> Self {
        Self {
            seq,
            cause,
            run,
            snapshot: None,
            directory: Arc::new(BTreeMap::new()),
            events: Vec::new(),
            state_hash: None,
        }
    }
}

/// How a subscriber wants its frames.
#[non_exhaustive]
pub struct SubscribeOptions {
    /// Frames queued before the oldest is dropped; default 4.
    pub capacity: NonZeroUsize,
    /// Tick frames are built only every this many ticks; control frames are always built; default 1.
    pub min_tick_stride: NonZeroU64,
    /// Events carried over from dropped frames before `events_truncated`; default 4096.
    pub max_carry_events: usize,
    /// Whether frames need a state hash; default false.
    pub with_state_hash: bool,
    /// Called after every push, outside every lock; must not block.
    pub notify: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl SubscribeOptions {
    /// The defaults of ADR 0024.
    #[must_use]
    pub fn new() -> Self {
        Self {
            capacity: NonZeroUsize::MIN.saturating_add(3),
            min_tick_stride: NonZeroU64::MIN,
            max_carry_events: 4096,
            with_state_hash: false,
            notify: None,
        }
    }
}

impl Default for SubscribeOptions {
    fn default() -> Self {
        Self::new()
    }
}

/// Identifies a subscription slot on the bus, to target a frame at one subscriber (priming).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SlotId(u64);

impl SlotId {
    /// The slot number, for logs and tests.
    #[must_use]
    pub fn get(self) -> u64 {
        self.0
    }
}

/// What a [`Subscription`] hands out when drained.
#[derive(Debug)]
#[non_exhaustive]
pub struct Drained {
    /// The queued frames, oldest first (at most the capacity).
    pub frames: Vec<Arc<Frame>>,
    /// Frames dropped since the previous drain.
    pub dropped: u64,
    /// Events of dropped frames, to be delivered before the events of `frames`.
    pub carried_events: Vec<SimEvent>,
    /// Whether `carried_events` hit `max_carry_events` and lost events.
    pub events_truncated: bool,
}

/// Counters for tests (`#[doc(hidden)]` probe).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct SubscriptionStats {
    /// The longest the queue has been.
    pub max_queue_len: usize,
    /// Frames dropped over the subscription's life.
    pub dropped_total: u64,
    /// Frames pushed over the subscription's life.
    pub pushed_total: u64,
}

/// A registered subscriber; unregisters on drop (from WI-7). Many threads may drain different subscriptions of
/// one bus; the engine thread publishes.
pub struct Subscription {
    id: SlotId,
}

impl Subscription {
    /// This subscription's slot.
    #[must_use]
    pub fn id(&self) -> SlotId {
        self.id
    }

    /// Takes everything queued, without waiting.
    #[must_use]
    pub fn drain(&self) -> Drained {
        todo!("WI-7")
    }

    /// Waits up to `timeout` for a frame, then drains.
    #[must_use]
    pub fn wait(&self, timeout: Duration) -> Drained {
        todo!("WI-7")
    }

    /// Test counters.
    #[doc(hidden)]
    #[must_use]
    pub fn stats(&self) -> SubscriptionStats {
        todo!("WI-7")
    }
}

/// The registry of subscriptions and the publisher side (engine thread only).
///
/// It is public so the bus can be tested on its own (AT-3); embedders use
/// [`EngineHandle::subscribe`](crate::handle::EngineHandle::subscribe).
#[derive(Debug, Default)]
pub struct Bus {}

impl Bus {
    /// An empty bus.
    #[must_use]
    pub fn new() -> Self {
        Self {}
    }

    /// Registers a subscription. Frames pushed before it existed are not delivered.
    #[must_use]
    pub fn subscribe(self: &Arc<Self>, options: SubscribeOptions) -> Subscription {
        todo!("WI-7")
    }

    /// Whether any subscription is due a tick frame at `tick` by its stride.
    #[must_use]
    pub fn wants_frame(&self, tick: u64) -> bool {
        todo!("WI-7")
    }

    /// Whether a subscription due a tick frame at `tick` asked for the state hash.
    #[must_use]
    pub fn wants_state_hash(&self, tick: u64) -> bool {
        todo!("WI-7")
    }

    /// Pushes `frame` to every subscription, or only to `only` (priming). Never waits for space: a
    /// full queue drops its oldest frame and counts it; the dropped frame's events are carried.
    pub fn publish(&self, frame: Frame, only: Option<SlotId>) {
        todo!("WI-7")
    }
}
