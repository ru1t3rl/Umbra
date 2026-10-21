//! Publish policy and adapter settings (ADR 0032, REQ-N-2, ASM-231, ASM-239, ASM-240).
//!
//! Signature stub of M6 step 4; the bodies arrive with WI-6. `PublishTracker` is the unit-level
//! interface of the policy (ASM-269): the plan names the settings only, and the manager of WI-7
//! owns one tracker per adapter.
#![expect(
    clippy::todo,
    dead_code,
    unused_variables,
    reason = "M6 step-4 stubs, bodies from WI-6"
)]

use std::collections::BTreeMap;

use umbra_core::dead_reckoning::RemoteState;
use umbra_core::{EntityId, KinematicState, Seconds, SimTime};

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

/// Decides, tick by tick and entity by entity, what must be sent.
#[derive(Debug)]
pub struct PublishTracker {
    policy: PublishPolicy,
    sent: BTreeMap<EntityId, RemoteState>,
}

impl PublishTracker {
    /// A tracker that has sent nothing yet.
    #[must_use]
    pub fn new(policy: PublishPolicy) -> Self {
        Self {
            policy,
            sent: BTreeMap::new(),
        }
    }

    /// Call once per tick for every owned entity. Returns the basis to send when an update is due:
    /// on first sight, when the receiver's extrapolation of the last sent basis is further than the
    /// position threshold from `state`, when the orientation moved past its threshold, or at the
    /// heartbeat; never sooner than `min_interval` after the last send.
    pub fn evaluate(
        &mut self,
        now: SimTime,
        id: EntityId,
        state: &KinematicState,
    ) -> Option<RemoteState> {
        todo!("WI-6: PublishTracker::evaluate")
    }

    /// The entity is gone. Returns whether it had been published (then `EntityRemoved` is due, once);
    /// the entity is forgotten either way.
    pub fn removed(&mut self, id: EntityId) -> bool {
        todo!("WI-6: PublishTracker::removed")
    }
}
