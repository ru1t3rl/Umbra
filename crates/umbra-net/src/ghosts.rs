//! The world seam the manager mirrors peers' entities into, and the per-adapter record of when
//! each ghost was last heard (ADR 0030, ADR 0031, REQ-N-4).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use umbra_core::dead_reckoning::RemoteState;
use umbra_core::net::NetKey;
use umbra_core::{EntityId, Force, RemoteSource, Seconds, SimTime, TemplateId};

/// Everything needed to create one ghost.
#[derive(Debug, Clone)]
pub struct GhostSpec<'a> {
    /// The adapter the ghost comes from.
    pub source: RemoteSource,
    /// The mapped template.
    pub template: &'a TemplateId,
    /// The peer's force, if sent.
    pub force: Option<Force>,
    /// The peer's display name, if sent.
    pub name: Option<Arc<str>>,
    /// The first dead-reckoning basis.
    pub remote: RemoteState,
}

/// A ghost operation failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum GhostError {
    /// The world refused the operation.
    #[error("ghost world error: {0}")]
    World(String),
}

/// Where ghosts live. The engine implements it over `Kernel::spawn_remote`, `update_remote` and
/// `remove`; tests use a map.
pub trait GhostWorld {
    /// Creates a ghost.
    ///
    /// # Errors
    /// [`GhostError`] if the world refuses.
    fn create_ghost(&mut self, spec: GhostSpec<'_>) -> Result<EntityId, GhostError>;

    /// Replaces a ghost's basis.
    ///
    /// # Errors
    /// [`GhostError`] if the world refuses.
    fn update_ghost(&mut self, id: EntityId, remote: RemoteState) -> Result<(), GhostError>;

    /// Removes a ghost.
    ///
    /// # Errors
    /// [`GhostError`] if the world refuses.
    fn remove_ghost(&mut self, id: EntityId) -> Result<(), GhostError>;
}

/// The tick number of `t` (ticks since the run's start; the rate is fixed within a run).
fn tick_of(t: SimTime) -> u64 {
    t.ticks_since(SimTime::start(t.rate())).unwrap_or(0)
}

/// When each of one adapter's ghosts was last updated, in simulation time (ASM-233), indexed by
/// expiry so a tick visits only the ghosts that are stale, not every ghost.
#[derive(Debug, Default)]
pub(crate) struct GhostClock {
    last_heard: BTreeMap<NetKey, u64>,
    /// `(tick last heard, key)`, oldest first.
    by_age: BTreeSet<(u64, NetKey)>,
}

impl GhostClock {
    /// Live ghosts.
    pub(crate) fn len(&self) -> usize {
        self.last_heard.len()
    }

    /// The ghost of `key` was created or updated at `now`.
    pub(crate) fn heard(&mut self, key: NetKey, now: SimTime) {
        let tick = tick_of(now);
        if let Some(old) = self.last_heard.insert(key, tick) {
            self.by_age.remove(&(old, key));
        }
        self.by_age.insert((tick, key));
    }

    /// The ghost of `key` is gone.
    pub(crate) fn forget(&mut self, key: NetKey) {
        if let Some(old) = self.last_heard.remove(&key) {
            self.by_age.remove(&(old, key));
        }
    }

    /// Replaces the content of `into` with the keys of ghosts not heard from for strictly more
    /// than `timeout`, oldest first and by key within one tick (a timeout that is not positive
    /// expires a ghost at the next tick). A clock that went backwards expires nothing.
    pub(crate) fn expired_into(&self, now: SimTime, timeout: Seconds, into: &mut Vec<NetKey>) {
        into.clear();
        let limit = crate::policy::ticks_of(now.rate(), timeout);
        let before = tick_of(now).saturating_sub(limit);
        into.extend(
            self.by_age
                .range(..(before, NetKey(0)))
                .map(|&(_, key)| key),
        );
    }
}

#[cfg(test)]
mod tests {
    use umbra_core::TickRate;

    use super::*;

    fn at(tick: u64) -> SimTime {
        let rate = TickRate::new(30).unwrap();
        SimTime::at_tick(rate, tick)
    }

    fn expired(clock: &GhostClock, now: SimTime, timeout: f64) -> Vec<NetKey> {
        let mut into = vec![NetKey(77)];
        clock.expired_into(now, Seconds(timeout), &mut into);
        into
    }

    #[test]
    fn expires_only_strictly_past_the_timeout_in_key_order() {
        let mut clock = GhostClock::default();
        clock.heard(NetKey(9), at(0));
        clock.heard(NetKey(3), at(0));
        clock.heard(NetKey(5), at(10));
        assert_eq!(expired(&clock, at(30), 1.0), vec![]);
        assert_eq!(expired(&clock, at(31), 1.0), vec![NetKey(3), NetKey(9)]);
        // Oldest first, then by key.
        assert_eq!(
            expired(&clock, at(41), 1.0),
            vec![NetKey(3), NetKey(9), NetKey(5)]
        );
        clock.heard(NetKey(3), at(40));
        assert_eq!(expired(&clock, at(41), 1.0), vec![NetKey(9), NetKey(5)]);
        clock.heard(NetKey(3), at(0));
        clock.forget(NetKey(3));
        assert_eq!(clock.len(), 2);
    }

    #[test]
    fn a_backwards_clock_and_a_zero_timeout() {
        let mut clock = GhostClock::default();
        clock.heard(NetKey(1), at(10));
        assert_eq!(expired(&clock, at(5), 1.0), vec![]);
        assert_eq!(expired(&clock, at(10), 0.0), vec![]);
        assert_eq!(expired(&clock, at(11), 0.0), vec![NetKey(1)]);
    }
}
