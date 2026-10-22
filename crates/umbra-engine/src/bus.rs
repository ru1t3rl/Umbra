//! The snapshot bus (REQ-K-6, REQ-X-7, ADR 0024): bounded per-subscriber frame queues that drop the
//! oldest frame instead of ever blocking the engine thread.
//!
//! Locking rule (ADR 0024 m1): the registry lock is held only to copy or prune the slot list; a push
//! holds only the one slot's mutex; no lock is held across `notify`. Poisoned locks are recovered with
//! `PoisonError::into_inner`, since no code under them can leave the data half-updated.

use std::collections::BTreeMap;
use std::collections::VecDeque;
use std::num::{NonZeroU64, NonZeroUsize};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
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
    /// A network adapter fault under the `abort` policy (M6, ASM-252). The wire reports it as
    /// `network_fault` with the adapter and the reason (ASM-294).
    NetworkFault(crate::net::NetFault),
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
    /// The events of the tick(s) since the previous built frame (none for a pure command frame).
    pub events: Vec<SimEvent>,
    /// The tick each of `events` happened in, same order (ASM-205). May be shorter than `events`
    /// for a frame built by hand; an event without an entry is taken to be of the frame's tick.
    pub event_ticks: Vec<u64>,
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
            event_ticks: Vec::new(),
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
    /// Used while `max_rate_hz` is unset or no scenario is loaded (no tick rate known).
    pub min_tick_stride: NonZeroU64,
    /// The most frames per second the subscriber wants. When set (and finite and above 0; anything
    /// else counts as unset), the stride follows the tick rate the engine told the bus
    /// ([`Bus::set_tick_rate`]): `max(1, floor(tick_rate_hz / max_rate_hz))` ([`stride_for`]), so a
    /// load that changes the tick rate changes the stride before its first tick (ASM-212). Default
    /// `None`.
    pub max_rate_hz: Option<f64>,
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
            max_rate_hz: None,
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

/// The tick stride that delivers about `rate_hz` frames per second at `tick_rate_hz` ticks per
/// second: `max(1, floor(tick_rate_hz / rate_hz))`, integer division of whole hertz (ASM-165).
///
/// A `rate_hz` that is not finite and above 0 gives 1 (every tick); a subscription with such a rate
/// uses its `min_tick_stride` instead (see `SubscribeOptions::max_rate_hz`).
#[must_use]
pub fn stride_for(tick_rate_hz: u32, rate_hz: f64) -> NonZeroU64 {
    if !(rate_hz.is_finite() && rate_hz > 0.0) {
        return NonZeroU64::MIN;
    }
    // Truncation is the point: a quotient below 1 (or any negative) saturates to 0 and becomes 1.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a floor of a small positive quotient"
    )]
    let whole = (f64::from(tick_rate_hz) / rate_hz).floor() as u64;
    NonZeroU64::new(whole).unwrap_or(NonZeroU64::MIN)
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
    /// The tick of each of `carried_events`, same order and length (ASM-205).
    pub carried_ticks: Vec<u64>,
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
    carry_ticks: Vec<u64>,
    truncated: bool,
    dropped: u64,
    stats: SubscriptionStats,
}

/// One subscription's shared half: options, queue and the condvar `wait` sleeps on.
struct Slot {
    id: SlotId,
    capacity: usize,
    stride: u64,
    rate_hz: Option<f64>,
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

    /// Whether a tick frame is due at `tick`; `tick_rate_hz` is the bus's current tick rate, 0 if
    /// none is loaded.
    fn due(&self, tick: u64, tick_rate_hz: u32) -> bool {
        let stride = match self.rate_hz {
            Some(rate) if tick_rate_hz > 0 => stride_for(tick_rate_hz, rate).get(),
            _ => self.stride,
        };
        tick.is_multiple_of(stride)
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
        debug_assert!(
            old.event_ticks.len() <= old.events.len(),
            "no tick without an event"
        );
        let room = self.max_carry.saturating_sub(state.carry.len());
        state.carry.extend(old.events.iter().take(room).cloned());
        let at = old.snapshot.as_ref().map_or(0, |s| s.time.tick());
        for index in 0..old.events.len().min(room) {
            state
                .carry_ticks
                .push(old.event_ticks.get(index).copied().unwrap_or(at));
        }
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
            carried_ticks: std::mem::take(&mut self.carry_ticks),
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
    /// The tick rate of the loaded scenario ([`Bus::set_tick_rate`]), 0 while none is loaded.
    tick_rate_hz: AtomicU32,
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

    /// Tells the bus the tick rate of the loaded scenario, `None` once it is unloaded. The engine
    /// thread calls it on load and unload, before any frame is built or any interest is checked, so
    /// a rate-following stride (`SubscribeOptions::max_rate_hz`) is right from the first tick of a
    /// session, with or without a subscriber at the time of the load (ASM-212).
    pub fn set_tick_rate(&self, rate: Option<TickRate>) {
        self.tick_rate_hz
            .store(rate.map_or(0, TickRate::hz), Ordering::Release);
    }

    fn tick_rate(&self) -> u32 {
        self.tick_rate_hz.load(Ordering::Acquire)
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
            rate_hz: options
                .max_rate_hz
                .filter(|rate| rate.is_finite() && *rate > 0.0),
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
        let hz = self.tick_rate();
        self.live().iter().any(|s| s.due(tick, hz))
    }

    /// Whether a subscription due a tick frame at `tick` asked for the state hash.
    #[must_use]
    pub fn wants_state_hash(&self, tick: u64) -> bool {
        let hz = self.tick_rate();
        self.live()
            .iter()
            .any(|s| s.due(tick, hz) && s.with_state_hash)
    }

    /// `(frame due, state hash due)` at `tick` from one registry snapshot: the answers of
    /// [`wants_frame`](Self::wants_frame) and [`wants_state_hash`](Self::wants_state_hash) together.
    #[must_use]
    pub fn due(&self, tick: u64) -> (bool, bool) {
        let hz = self.tick_rate();
        self.live().iter().fold((false, false), |(frame, hash), s| {
            let due = s.due(tick, hz);
            (frame || due, hash || (due && s.with_state_hash))
        })
    }

    /// `(any subscription exists, any of them asks for state hashes)`, for frames built whatever the
    /// stride (control frames, step completion, stop).
    #[must_use]
    pub fn interest(&self) -> (bool, bool) {
        self.live().iter().fold((false, false), |(_, hash), s| {
            (true, hash || s.with_state_hash)
        })
    }

    /// Rings every subscription's `notify` and wakes every `wait`, without pushing a frame: the
    /// publisher is gone and subscribers should look at why. Like a push, no lock is held while the
    /// callbacks run.
    pub fn wake_all(&self) {
        for slot in self.live() {
            slot.ready.notify_all();
            if let Some(notify) = &slot.notify {
                notify();
            }
        }
    }

    /// Pushes `frame` to every subscription, or only to `only` (priming). Never waits for space: a
    /// full queue drops its oldest frame and counts it; the dropped frame's events are carried.
    pub fn publish(&self, frame: Frame, only: Option<SlotId>) {
        debug_assert!(
            frame.event_ticks.len() <= frame.events.len(),
            "no tick without an event"
        );
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

    #[test]
    fn stride_is_the_integer_quotient_and_at_least_one() {
        let cases = [
            (30, 10.0, 3),
            (30, 15.0, 2),
            (30, 60.0, 1),
            (60, 30.0, 2),
            (60, 25.0, 2),
            (60, 7.5, 8),
            (60, 0.0, 1),
            (60, -2.0, 1),
            (60, f64::NAN, 1),
            (60, f64::INFINITY, 1),
        ];
        for (ticks, rate, expected) in cases {
            assert_eq!(
                stride_for(ticks, rate).get(),
                expected,
                "{ticks} Hz at {rate}"
            );
        }
    }

    /// ASM-212: a rate-following stride tracks the tick rate set on the bus, with no frame published.
    #[test]
    fn rate_following_stride_tracks_the_tick_rate_the_engine_sets() {
        let bus = Arc::new(Bus::new());
        let mut options = SubscribeOptions::new();
        options.max_rate_hz = Some(10.0);
        let _sub = bus.subscribe(options);
        // No tick rate known: every tick (the fixed stride, 1).
        assert!(bus.wants_frame(1));
        bus.set_tick_rate(TickRate::new(60));
        assert!(
            !bus.wants_frame(5) && bus.wants_frame(6),
            "60 Hz at 10 Hz: every 6th"
        );
        bus.set_tick_rate(TickRate::new(30));
        assert!(
            !bus.wants_frame(2) && bus.wants_frame(3),
            "30 Hz at 10 Hz: every 3rd"
        );
        bus.set_tick_rate(None);
        assert!(bus.wants_frame(1), "unloaded: back to the fixed stride");
        // A rate that is not a positive number is no rate: the fixed stride stays.
        for bad in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            let mut options = SubscribeOptions::new();
            options.max_rate_hz = Some(bad);
            options.min_tick_stride = NonZeroU64::new(4).expect("non zero");
            let sub = bus.subscribe(options);
            bus.set_tick_rate(TickRate::new(60));
            assert!(
                !sub.slot.due(3, 60) && sub.slot.due(4, 60) && sub.slot.due(8, 60),
                "rate {bad}: the fixed stride of 4"
            );
        }
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
