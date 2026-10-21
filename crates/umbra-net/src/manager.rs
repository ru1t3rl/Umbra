//! The network manager: adapters, ingest, publish, ghost lifecycle, echo suppression
//! (REQ-N-1..5, REQ-K-5, ADR 0030..0032).
//!
//! Signature stub of M6 step 4; the bodies arrive with WI-7.
#![expect(
    clippy::todo,
    dead_code,
    clippy::needless_pass_by_value,
    clippy::ptr_arg,
    unused_variables,
    reason = "M6 step-4 stubs, bodies from WI-7"
)]

use std::collections::BTreeMap;
use std::sync::Arc;

use umbra_core::net::{
    AdapterCounters, KeySpace, NetEntityType, NetError, NetworkAdapterConfig, OwnedView,
};
use umbra_core::{ControlAction, EntityId, SimEvent, SimTime, TemplateId};

use crate::policy::AdapterSettings;
use crate::typemap::TypeMap;

/// Per owned entity data the engine keeps for publishing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnedMeta {
    /// The template the entity was built from, if any.
    pub template: Option<TemplateId>,
    /// Its display name, if any.
    pub name: Option<Arc<str>>,
}

/// Everything the manager needs for one adapter (ASM-276). Non-exhaustive: build it with
/// [`AdapterSlot::new`].
#[derive(Clone)]
#[non_exhaustive]
pub struct AdapterSlot {
    /// Name, enablement and the publish and ghost policies.
    pub settings: AdapterSettings,
    /// The configured, not yet started adapter.
    pub config: Arc<dyn NetworkAdapterConfig>,
    /// Entity type token to template mapping for inbound entities.
    pub type_map: TypeMap,
    /// The adapter's `network.<name>.entity_type` of each template, resolved once at load; a
    /// template absent here is published without a type (ASM-239).
    pub entity_types: BTreeMap<TemplateId, NetEntityType>,
}

impl AdapterSlot {
    /// A slot from its parts.
    #[must_use]
    pub fn new(
        settings: AdapterSettings,
        config: Arc<dyn NetworkAdapterConfig>,
        type_map: TypeMap,
        entity_types: BTreeMap<TemplateId, NetEntityType>,
    ) -> Self {
        Self {
            settings,
            config,
            type_map,
            entity_types,
        }
    }
}

/// A counter snapshot of one adapter (the engine prints it at the end of a run, ASM-250).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct AdapterStatus {
    /// The adapter's scenario name.
    pub name: String,
    /// Whether it is polled and published to.
    pub enabled: bool,
    /// Whether a fault disabled it (ASM-252).
    pub faulted: bool,
    /// The keys it issues.
    pub key_space: KeySpace,
    /// Live ghosts from this adapter.
    pub ghosts: usize,
    /// Messages handed to the adapter.
    pub published: u64,
    /// Messages taken from the adapter.
    pub received: u64,
    /// Inbound messages dropped as echoes (own key space or an owned entity's key).
    pub echo_dropped: u64,
    /// Inbound entities whose type maps to no template (ASM-236).
    pub unmapped: u64,
    /// Local keys refused because the own key space is used up (ASM-277).
    pub keys_exhausted: u64,
    /// Ghosts refused because `max` was reached.
    pub ghosts_capped: u64,
    /// Inbound run-control messages ignored (ASM-235).
    pub control_ignored: u64,
    /// The adapter's own counters.
    pub counters: AdapterCounters,
}

/// A manager operation failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ManagerError {
    /// No adapter has this name.
    #[error("unknown network adapter `{0}`")]
    UnknownAdapter(String),
}

/// Owns the started adapters in scenario order and runs the per-tick ingest and publish steps.
#[derive(Debug)]
pub struct NetworkManager {
    slots: usize,
}

impl NetworkManager {
    /// Starts every adapter, in order.
    ///
    /// # Errors
    /// The first adapter's [`NetError`]; adapters started before it are stopped.
    pub fn new(slots: Vec<AdapterSlot>, now: SimTime) -> Result<Self, NetError> {
        todo!("WI-7: NetworkManager::new")
    }

    /// Polls the enabled adapters in scenario order and applies the messages in delivery order:
    /// creates, updates and removes ghosts, times stale ghosts out, and appends the resulting
    /// events (ASM-248) to `events`.
    pub fn ingest(
        &mut self,
        now: SimTime,
        world: &mut dyn crate::ghosts::GhostWorld,
        events: &mut Vec<SimEvent>,
    ) {
        todo!("WI-7: NetworkManager::ingest")
    }

    /// Publishes the owned entities that are due on every enabled adapter, and the `Fire` and
    /// `Detonation` events in `events` whose source is an owned entity (ASM-278). Entities that
    /// are ghosts of any adapter are skipped even when present in `owned` (REQ-K-5).
    pub fn publish(
        &mut self,
        now: SimTime,
        owned: &[OwnedView],
        meta: &BTreeMap<EntityId, OwnedMeta>,
        events: &[SimEvent],
    ) {
        todo!("WI-7: NetworkManager::publish")
    }

    /// Publishes a run-control action on adapters whose policy has `control` set (ASM-244).
    pub fn publish_control(&mut self, now: SimTime, action: ControlAction) {
        todo!("WI-7: NetworkManager::publish_control")
    }

    /// Enables or disables the adapter called `name`.
    ///
    /// # Errors
    /// [`ManagerError::UnknownAdapter`].
    pub fn set_enabled(
        &mut self,
        name: &str,
        enabled: bool,
    ) -> Result<AdapterStatus, ManagerError> {
        todo!("WI-7: NetworkManager::set_enabled")
    }

    /// One status per adapter, in scenario order.
    #[must_use]
    pub fn status(&self) -> Vec<AdapterStatus> {
        todo!("WI-7: NetworkManager::status")
    }

    /// Stops every adapter. Idempotent; also done on drop. The stub body is empty so `Drop` can
    /// never panic; WI-7 implements it.
    pub fn stop(&mut self) {}
}

impl Drop for NetworkManager {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
impl NetworkManager {
    /// Binds `key` to owned entity `e` in adapter `index`'s identity map, outside its key space.
    #[expect(clippy::todo, reason = "M6 step-4 stub, body from WI-7")]
    fn bind_owned_for_test(&mut self, index: usize, key: umbra_core::net::NetKey, e: EntityId) {
        todo!("WI-7: bind_owned_for_test")
    }
}

#[cfg(test)]
mod tests {
    use umbra_core::dead_reckoning::RemoteState;
    use umbra_core::net::{AdapterStart, KeySpace, NetKey, NetMessage};
    use umbra_core::{ComponentId, EntityId, SimTime, TickRate};

    use super::*;
    use crate::ghosts::{GhostError, GhostSpec, GhostWorld};
    use crate::loopback::LoopbackHub;

    /// A world that must never be asked for a ghost.
    struct NoGhosts;

    impl GhostWorld for NoGhosts {
        fn create_ghost(&mut self, _: GhostSpec<'_>) -> Result<EntityId, GhostError> {
            panic!("a ghost was created");
        }
        fn update_ghost(&mut self, _: EntityId, _: RemoteState) -> Result<(), GhostError> {
            panic!("a ghost was updated");
        }
        fn remove_ghost(&mut self, _: EntityId) -> Result<(), GhostError> {
            panic!("a ghost was removed");
        }
    }

    /// ADR 0031 layer 3 (defence in depth): a key bound to an owned entity is dropped as an echo
    /// even when it lies outside the own key space, where layer 1 cannot see it.
    #[test]
    #[ignore = "WI-7"]
    fn a_key_bound_as_owned_outside_the_own_space_is_dropped_as_an_echo() {
        let now = SimTime::start(TickRate::new(30).unwrap());
        let hub = LoopbackHub::new(16);
        let id = |s: &str| ComponentId::new(s).unwrap();
        let config = hub
            .factory(
                id("test.net.a"),
                KeySpace {
                    base: 1000,
                    count: 100,
                },
            )
            .build(&serde_json::json!({}))
            .unwrap();
        let peer_config = hub
            .factory(
                id("test.net.peer"),
                KeySpace {
                    base: 2000,
                    count: 100,
                },
            )
            .build(&serde_json::json!({}))
            .unwrap();
        let settings = AdapterSettings {
            name: "lb".to_owned(),
            enabled: true,
            publish: umbra_core::net::PublishSettings::default(),
            ghosts: crate::policy::GhostPolicy {
                timeout: umbra_core::Seconds(1.0),
                max: 10,
                max_inbound_per_tick: 10,
            },
        };
        let mut manager = NetworkManager::new(
            vec![AdapterSlot::new(
                settings,
                config,
                TypeMap::default(),
                BTreeMap::new(),
            )],
            now,
        )
        .unwrap();
        let mut peer = peer_config.start(&AdapterStart { now }).unwrap();
        manager.bind_owned_for_test(0, NetKey(5000), EntityId::new(1).unwrap());
        peer.publish(now, &[NetMessage::EntityRemoved { key: NetKey(5000) }])
            .unwrap();

        manager.ingest(now, &mut NoGhosts, &mut Vec::new());

        assert_eq!(manager.status()[0].echo_dropped, 1);
    }
}
