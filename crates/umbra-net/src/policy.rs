//! Publish policy and adapter settings (ADR 0032, REQ-N-2, ASM-231, ASM-239, ASM-240).
//!
//! `PublishTracker` is the unit-level interface of the policy (ASM-269): the manager (from WI-7)
//! owns one tracker per adapter. Simulation time only; no wall clock.

use std::collections::BTreeMap;

use umbra_core::dead_reckoning::{DeadReckoning, RemoteState, extrapolate};
use umbra_core::{EntityId, KinematicState, Seconds, SimTime, TickRate, Vec3};

/// When an owned entity's state is sent: the scenario's publish settings (ASM-275).
pub use umbra_core::net::PublishSettings as PublishPolicy;

/// How ghosts are kept.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GhostPolicy {
    /// A ghost not updated for longer than this is removed.
    pub timeout: Seconds,
    /// At most this many ghosts per adapter.
    pub max: usize,
    /// At most this many inbound messages are applied per tick; the rest stay queued.
    pub max_inbound_per_tick: usize,
}

/// One adapter's settings as the engine hands them to the manager.
#[derive(Debug, Clone, PartialEq)]
pub struct AdapterSettings {
    /// The scenario's adapter name (also the ghosts' source label).
    pub name: String,
    /// Whether the adapter starts enabled.
    pub enabled: bool,
    /// Publish policy.
    pub publish: PublishPolicy,
    /// Ghost policy.
    pub ghosts: GhostPolicy,
}

/// What the tracker remembers about one published entity.
#[derive(Debug)]
struct Tracked {
    /// The last basis sent and the time it was sent.
    sent: RemoteState,
    sent_at: SimTime,
    /// Velocity samples of the previous two consecutive evaluations (`prev` is the latest), each
    /// with its time. Both are cleared when the evaluations stop being one tick apart.
    prev: Option<(SimTime, Vec3)>,
    prev2: Option<(SimTime, Vec3)>,
}

/// The heartbeat and minimum interval in whole ticks at one rate (computed once per rate).
#[derive(Debug, Clone, Copy)]
struct IntervalTicks {
    rate: TickRate,
    min_interval: u64,
    heartbeat: u64,
}

/// Decides, tick by tick and entity by entity, what must be sent.
#[derive(Debug)]
pub struct PublishTracker {
    policy: PublishPolicy,
    tracked: BTreeMap<EntityId, Tracked>,
    intervals: Option<IntervalTicks>,
}

/// Whole ticks of `d` at `rate` (`ceil(d * hz)`, ASM-269); zero for a non-positive duration.
fn ticks_of(rate: TickRate, d: Seconds) -> u64 {
    rate.ticks_for(d).map_or(
        // A duration that is not positive means "no limit" for the interval and is rejected by the
        // scenario reader for the heartbeat; one too large for u64 never elapses.
        if d.0 > 0.0 { u64::MAX } else { 0 },
        std::num::NonZeroU64::get,
    )
}

fn distance(a: Vec3, b: Vec3) -> f64 {
    Vec3::new(a.x - b.x, a.y - b.y, a.z - b.z).norm()
}

impl PublishTracker {
    /// A tracker that has sent nothing yet.
    #[must_use]
    pub fn new(policy: PublishPolicy) -> Self {
        Self {
            policy,
            tracked: BTreeMap::new(),
            intervals: None,
        }
    }

    /// The interval tick counts at `rate`, recomputed only when the rate changes.
    fn intervals(&mut self, rate: TickRate) -> IntervalTicks {
        match self.intervals {
            Some(i) if i.rate == rate => i,
            _ => {
                let i = IntervalTicks {
                    rate,
                    min_interval: ticks_of(rate, self.policy.min_interval),
                    heartbeat: ticks_of(rate, self.policy.heartbeat),
                };
                self.intervals = Some(i);
                i
            }
        }
    }

    /// Call once per tick for every owned entity. Returns the basis to send when an update is due:
    /// on first sight, when the receiver's extrapolation of the last sent basis is further than the
    /// position threshold from `state`, when the orientation moved past its threshold, or at the
    /// heartbeat; never sooner than `min_interval` after the last send.
    ///
    /// The basis carries the entity's state, the configured model and, for
    /// [`DeadReckoning::Acceleration`], the two-tick finite difference of velocity
    /// `(v_n - v_(n-2)) / (2 dt)`. That is zero unless this and the two previous calls were on
    /// consecutive ticks at one rate; a gap, a clock going backwards or a rate change restarts the
    /// history. Other models send zero acceleration. Rotating and body-frame models are not
    /// published (ASM-231), so `angular_velocity` is always zero.
    pub fn evaluate(
        &mut self,
        now: SimTime,
        id: EntityId,
        state: &KinematicState,
    ) -> Option<RemoteState> {
        let model = self.policy.model;
        let intervals = self.intervals(now.rate());
        // Restart the velocity history unless the previous sample is exactly one tick back.
        let mut history = self
            .tracked
            .get(&id)
            .map_or((None, None), |t| match t.prev {
                Some((at, _)) if now.ticks_since(at) == Ok(1) => (t.prev, t.prev2),
                _ => (None, None),
            });
        let acceleration = match (model, history.1) {
            (DeadReckoning::Acceleration, Some((_, v2))) => {
                let k = 1.0 / (2.0 * now.dt().0);
                Vec3::new(
                    (state.velocity.x - v2.x) * k,
                    (state.velocity.y - v2.y) * k,
                    (state.velocity.z - v2.z) * k,
                )
            }
            _ => Vec3::ZERO,
        };
        history = (Some((now, state.velocity)), history.0);
        let due = self
            .tracked
            .get(&id)
            .is_none_or(|t| self.update_due(now, intervals, t, state));
        let basis = RemoteState {
            state: *state,
            acceleration,
            angular_velocity: Vec3::ZERO,
            model,
        };
        match self.tracked.get_mut(&id) {
            Some(t) => {
                (t.prev, t.prev2) = history;
                if due {
                    t.sent = basis;
                    t.sent_at = now;
                }
            }
            None => {
                self.tracked.insert(
                    id,
                    Tracked {
                        sent: basis,
                        sent_at: now,
                        prev: history.0,
                        prev2: history.1,
                    },
                );
            }
        }
        due.then_some(basis)
    }

    /// Whether a published entity is due an update now.
    fn update_due(
        &self,
        now: SimTime,
        intervals: IntervalTicks,
        t: &Tracked,
        state: &KinematicState,
    ) -> bool {
        // A clock that went backwards or changed rate cannot be reasoned about: send.
        let Ok(ticks) = now.ticks_since(t.sent_at) else {
            return true;
        };
        if ticks < intervals.min_interval {
            return false;
        }
        if ticks >= intervals.heartbeat {
            return true;
        }
        // What the receiver shows now, by the function it uses (ADR 0032).
        let elapsed = SimTime::at_tick(now.rate(), ticks).as_seconds();
        let seen = extrapolate(&t.sent, elapsed);
        distance(seen.position.0, state.position.0) > self.policy.position_threshold_m
            || seen.orientation.angle_between(state.orientation).0
                > self.policy.orientation_threshold.0
    }

    /// The entity is gone. Returns whether it had been published (then `EntityRemoved` is due, once);
    /// the entity is forgotten either way.
    pub fn removed(&mut self, id: EntityId) -> bool {
        self.tracked.remove(&id).is_some()
    }
}
