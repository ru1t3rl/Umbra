//! The snapshot bus (REQ-K-6, REQ-X-7, ADR 0024): bounded per-subscriber frame queues that drop the
//! oldest frame instead of ever blocking the engine thread.
//!
//! Locking rule (ADR 0024 m1): the registry lock is held only to copy or prune the slot list; a push
//! holds only the one slot's mutex; no lock is held across `notify`. Poisoned locks are recovered with
//! `PoisonError::into_inner`, since no code under them can leave the data half-updated.

use std::collections::BTreeMap;
use std::collections::VecDeque;
use std::num::{NonZeroU64, NonZeroUsize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError, Weak};
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

/// The queue and counters of one slot, behind the slot's mutex.
#[derive(Default)]
struct SlotState {
    queue: VecDeque<Arc<Frame>>,
    carry: Vec<SimEvent>,
    truncated: bool,
    dropped: u64,
    stats: SubscriptionStats,
}

/// One subscription's shared half: options, queue and the condvar `wait` sleeps on.
struct Slot {
    id: SlotId,
    capacity: usize,
    stride: u64,
    max_carry: usize,
    with_state_hash: bool,
    notify: Option<Arc<dyn Fn() + Send + Sync>>,
    state: Mutex<SlotState>,
    ready: Condvar,
}

impl Slot {
    fn lock(&self) -> MutexGuard<'_, SlotState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn due(&self, tick: u64) -> bool {
        tick.is_multiple_of(self.stride)
    }

    /// Pushes `frame`, dropping the oldest frames over the capacity; never waits for space.
    fn push(&self, frame: Arc<Frame>) {
        {
            let mut state = self.lock();
            state.queue.push_back(frame);
            state.stats.pushed_total += 1;
            while state.queue.len() > self.capacity {
                if let Some(old) = state.queue.pop_front() {
                    state.dropped += 1;
                    state.stats.dropped_total += 1;
                    self.carry_events(&mut state, &old);
                }
            }
            state.stats.max_queue_len = state.stats.max_queue_len.max(state.queue.len());
        }
        self.ready.notify_all();
        if let Some(notify) = &self.notify {
            notify();
        }
    }

    fn carry_events(&self, state: &mut SlotState, old: &Frame) {
        let room = self.max_carry.saturating_sub(state.carry.len());
        state.carry.extend(old.events.iter().take(room).cloned());
        if old.events.len() > room {
            state.truncated = true;
        }
    }
}

impl SlotState {
    fn take(&mut self) -> Drained {
        Drained {
            frames: self.queue.drain(..).collect(),
            dropped: std::mem::take(&mut self.dropped),
            carried_events: std::mem::take(&mut self.carry),
            events_truncated: std::mem::take(&mut self.truncated),
        }
    }
}

/// A registered subscriber; unregisters on drop. Many threads may drain different subscriptions of
/// one bus; the engine thread publishes.
pub struct Subscription {
    slot: Arc<Slot>,
    bus: Weak<Bus>,
}

impl std::fmt::Debug for Subscription {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Subscription")
            .field("id", &self.slot.id)
            .finish_non_exhaustive()
    }
}

impl Subscription {
    /// This subscription's slot.
    #[must_use]
    pub fn id(&self) -> SlotId {
        self.slot.id
    }

    /// Takes everything queued, without waiting.
    #[must_use]
    pub fn drain(&self) -> Drained {
        self.slot.lock().take()
    }

    /// Waits up to `timeout` for a frame, then drains.
    #[must_use]
    pub fn wait(&self, timeout: Duration) -> Drained {
        let guard = self.slot.lock();
        let (mut state, _) = self
            .slot
            .ready
            .wait_timeout_while(guard, timeout, |s| s.queue.is_empty())
            .unwrap_or_else(PoisonError::into_inner);
        state.take()
    }

    /// Test counters.
    #[doc(hidden)]
    #[must_use]
    pub fn stats(&self) -> SubscriptionStats {
        self.slot.lock().stats
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        if let Some(bus) = self.bus.upgrade() {
            // The slot is still alive here (this struct holds it), so prune every other dead entry
            // and this one by id.
            let id = self.slot.id;
            bus.registry()
                .retain(|w| w.upgrade().is_some_and(|s| s.id != id));
        }
    }
}

/// The registry of subscriptions and the publisher side (engine thread only).
///
/// It is public so the bus can be tested on its own (AT-3); embedders use
/// [`EngineHandle::subscribe`](crate::handle::EngineHandle::subscribe).
#[derive(Default)]
pub struct Bus {
    registry: Mutex<Vec<Weak<Slot>>>,
    next_slot: AtomicU64,
}

impl std::fmt::Debug for Bus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Bus").finish_non_exhaustive()
    }
}

impl Bus {
    /// An empty bus.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn registry(&self) -> MutexGuard<'_, Vec<Weak<Slot>>> {
        self.registry.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The live slots, copied under the registry lock, which is released before the caller uses them.
    fn live(&self) -> Vec<Arc<Slot>> {
        self.registry().iter().filter_map(Weak::upgrade).collect()
    }

    /// Registers a subscription. Frames pushed before it existed are not delivered.
    #[must_use]
    pub fn subscribe(self: &Arc<Self>, options: SubscribeOptions) -> Subscription {
        let slot = Arc::new(Slot {
            id: SlotId(self.next_slot.fetch_add(1, Ordering::Relaxed)),
            capacity: options.capacity.get(),
            stride: options.min_tick_stride.get(),
            max_carry: options.max_carry_events,
            with_state_hash: options.with_state_hash,
            notify: options.notify,
            state: Mutex::new(SlotState::default()),
            ready: Condvar::new(),
        });
        self.registry().push(Arc::downgrade(&slot));
        Subscription {
            slot,
            bus: Arc::downgrade(self),
        }
    }

    /// Whether any subscription is due a tick frame at `tick` by its stride.
    #[must_use]
    pub fn wants_frame(&self, tick: u64) -> bool {
        self.live().iter().any(|s| s.due(tick))
    }

    /// Whether a subscription due a tick frame at `tick` asked for the state hash.
    #[must_use]
    pub fn wants_state_hash(&self, tick: u64) -> bool {
        self.live().iter().any(|s| s.due(tick) && s.with_state_hash)
    }

    /// Pushes `frame` to every subscription, or only to `only` (priming). Never waits for space: a
    /// full queue drops its oldest frame and counts it; the dropped frame's events are carried.
    pub fn publish(&self, frame: Frame, only: Option<SlotId>) {
        let frame = Arc::new(frame);
        for slot in self.live() {
            if only.is_none_or(|id| id == slot.id) {
                slot.push(Arc::clone(&frame));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(seq: u64) -> Frame {
        Frame::new(
            seq,
            FrameCause::Tick,
            RunStatus::new(RunState::Running, ClockMode::Stepped, 1),
        )
    }

    /// m1: a panic while the registry lock is held poisons it; every bus operation recovers.
    #[test]
    fn poisoned_registry_recovers() {
        let bus = Arc::new(Bus::new());
        let before = bus.subscribe(SubscribeOptions::new());
        let hostile = Arc::clone(&bus);
        let outcome = std::thread::spawn(move || {
            let _held = hostile.registry.lock();
            #[expect(clippy::panic, reason = "poisons the lock on purpose")]
            {
                panic!("poison the registry");
            }
        })
        .join();
        assert!(outcome.is_err());
        assert!(bus.registry.is_poisoned(), "the test must poison the lock");

        let after = bus.subscribe(SubscribeOptions::new());
        assert!(bus.wants_frame(1));
        bus.publish(frame(0), None);
        assert_eq!(before.drain().frames.len(), 1);
        assert_eq!(after.drain().frames.len(), 1);
        drop(before);
        assert_eq!(
            bus.registry().len(),
            1,
            "drop unregistered under a poisoned lock"
        );
        bus.publish(frame(1), None);
        assert_eq!(after.drain().frames.len(), 1);
    }
}
