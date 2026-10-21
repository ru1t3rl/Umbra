//! The network manager: adapters, ingest, publish, ghost lifecycle, echo suppression
//! (REQ-N-1..5, REQ-K-5, ADR 0030..0032).
//!
//! Everything iterates in scenario order (adapters) or ascending id order (entities, keys), on
//! simulation time only (REQ-K-4).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;

use umbra_core::dead_reckoning::RemoteState;
use umbra_core::net::{
    AdapterCounters, AdapterStart, KeySpace, NetEntityState, NetEntityType, NetError, NetKey,
    NetMessage, NetworkAdapter, NetworkAdapterConfig, OwnedView,
};
use umbra_core::{
    ControlAction, EntityId, KinematicState, Quaternion, RemoteSource, SimEvent, SimTime,
    TemplateId, Vec3,
};

use crate::ghosts::{GhostClock, GhostSpec, GhostWorld};
use crate::identity::{IdentityError, IdentityMap, Role};
use crate::policy::{AdapterSettings, PublishTracker};
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
    /// Ghost operations the world refused (ASM-286).
    pub world_errors: u64,
    /// Inbound messages of a kind this version does not know (ASM-286).
    pub unsupported: u64,
    /// Inbound entity states with a non-finite value, refused (ASM-286).
    pub invalid_state: u64,
    /// Why the adapter was disabled by a fault; `None` unless `faulted` (ASM-288).
    pub fault_reason: Option<String>,
    /// Entity types refused as unmapped, ascending, at most [`MAX_UNMAPPED_TYPES`] distinct; the
    /// engine warns once per token from this (ASM-236, ASM-286). `unmapped` still counts all.
    pub unmapped_types: Vec<NetEntityType>,
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
    /// The adapter faulted and cannot be enabled again.
    #[error("network adapter `{0}` faulted and cannot be enabled")]
    Faulted(String),
}

/// At most this many distinct unmapped entity types are remembered per adapter.
pub const MAX_UNMAPPED_TYPES: usize = 64;

/// Event counts of one adapter (the status fields that are not derived).
#[derive(Debug, Default, Clone, Copy)]
struct Tally {
    published: u64,
    received: u64,
    echo_dropped: u64,
    unmapped: u64,
    keys_exhausted: u64,
    ghosts_capped: u64,
    control_ignored: u64,
    world_errors: u64,
    unsupported: u64,
    invalid_state: u64,
}

/// Whether every number of an inbound state is finite (a NaN would poison the ghost's track).
fn is_finite(s: &NetEntityState) -> bool {
    let v = |v: &Vec3| v.x.is_finite() && v.y.is_finite() && v.z.is_finite();
    v(&s.position.0)
        && v(&s.velocity)
        && v(&s.acceleration)
        && v(&s.angular_velocity)
        && s.orientation.yaw.0.is_finite()
        && s.orientation.pitch.0.is_finite()
        && s.orientation.roll.0.is_finite()
}

/// One adapter with everything the manager keeps for it.
struct Runner {
    settings: AdapterSettings,
    source: RemoteSource,
    key_space: KeySpace,
    /// `None` once stopped or faulted.
    adapter: Option<Box<dyn NetworkAdapter>>,
    enabled: bool,
    faulted: bool,
    fault_reason: Option<String>,
    identity: IdentityMap,
    tracker: PublishTracker,
    type_map: TypeMap,
    entity_types: BTreeMap<TemplateId, NetEntityType>,
    ghosts: GhostClock,
    /// Keys due to expire, reused every tick.
    expired: Vec<NetKey>,
    /// Messages to publish, reused every tick.
    out: Vec<NetMessage>,
    unmapped_types: BTreeSet<NetEntityType>,
    /// Owned entities refused a key, so each is counted once.
    exhausted: BTreeSet<EntityId>,
    inbox: Vec<NetMessage>,
    tally: Tally,
    /// The adapter's counters when it was last stopped.
    last_counters: AdapterCounters,
}

impl Runner {
    fn new(slot: AdapterSlot, adapter: Box<dyn NetworkAdapter>) -> Self {
        let key_space = slot.config.key_space();
        Self {
            source: RemoteSource::new(&slot.settings.name),
            key_space,
            adapter: Some(adapter),
            enabled: slot.settings.enabled,
            faulted: false,
            fault_reason: None,
            identity: IdentityMap::new(key_space),
            tracker: PublishTracker::new(slot.settings.publish),
            type_map: slot.type_map,
            entity_types: slot.entity_types,
            ghosts: GhostClock::default(),
            expired: Vec::new(),
            out: Vec::new(),
            unmapped_types: BTreeSet::new(),
            exhausted: BTreeSet::new(),
            inbox: Vec::new(),
            tally: Tally::default(),
            last_counters: AdapterCounters::default(),
            settings: slot.settings,
        }
    }

    /// Whether the adapter is polled and published to.
    fn live(&self) -> bool {
        self.enabled && self.adapter.is_some()
    }

    /// Stops the adapter (once) and keeps its last counters.
    fn stop(&mut self) {
        if let Some(mut adapter) = self.adapter.take() {
            adapter.stop();
            self.last_counters = adapter.counters();
        }
    }

    /// An adapter error disables the adapter; its ghosts time out (ASM-252).
    fn fault(&mut self, why: &NetError) {
        self.stop();
        self.faulted = true;
        self.fault_reason = Some(why.to_string());
        self.enabled = false;
    }

    /// Whether `e` is mirrored from this adapter.
    fn holds_ghost(&self, e: EntityId) -> bool {
        self.identity
            .key(e)
            .and_then(|k| self.identity.entity(k))
            .is_some_and(|(_, role)| role == Role::Remote)
    }

    /// The key of `e` if this engine owns it and has published it here.
    fn owned_key(&self, e: EntityId) -> Option<NetKey> {
        let key = self.identity.key(e)?;
        (self.identity.entity(key)?.1 == Role::Owned).then_some(key)
    }

    /// ADR 0031 layers 2 and 3: a key of the own space, or bound to an owned entity.
    fn is_echo(&self, key: NetKey) -> bool {
        self.key_space.contains(key) || matches!(self.identity.entity(key), Some((_, Role::Owned)))
    }

    /// The local entity a peer's key stands for, whoever simulates it.
    fn entity_of(&self, key: NetKey) -> Option<EntityId> {
        self.identity.entity(key).map(|(e, _)| e)
    }

    fn poll(&mut self, now: SimTime) {
        let max = self.settings.ghosts.max_inbound_per_tick;
        let mut inbox = std::mem::take(&mut self.inbox);
        inbox.clear();
        let polled = match self.adapter.as_mut() {
            Some(adapter) if self.enabled => adapter.poll(now, max, &mut inbox),
            _ => Ok(()),
        };
        if let Err(e) = polled {
            self.fault(&e);
        }
        self.tally.received += inbox.len() as u64;
        self.inbox = inbox;
    }

    fn apply_inbox(
        &mut self,
        now: SimTime,
        world: &mut dyn GhostWorld,
        events: &mut Vec<SimEvent>,
    ) {
        let inbox = std::mem::take(&mut self.inbox);
        for msg in &inbox {
            self.apply(now, msg, world, events);
        }
        self.inbox = inbox;
    }

    fn apply(
        &mut self,
        now: SimTime,
        msg: &NetMessage,
        world: &mut dyn GhostWorld,
        events: &mut Vec<SimEvent>,
    ) {
        match msg {
            NetMessage::EntityState(s) => self.on_state(now, s, world, events),
            NetMessage::EntityRemoved { key } => {
                if self.is_echo(*key) {
                    self.tally.echo_dropped += 1;
                } else if let Some(e) = self.entity_of(*key) {
                    self.drop_ghost(*key, e, world, events);
                }
            }
            NetMessage::Fire {
                shooter,
                target,
                weapon,
                ..
            } => {
                if self.is_echo(*shooter) {
                    self.tally.echo_dropped += 1;
                    return;
                }
                let shooter = self.entity_of(*shooter);
                let target = target.and_then(|k| self.entity_of(k));
                if let (Some(shooter), Some(target)) = (shooter, target) {
                    events.push(SimEvent::Fire {
                        shooter,
                        target,
                        weapon: weapon.clone(),
                    });
                }
            }
            NetMessage::Detonation {
                shooter,
                target,
                position,
            } => {
                if shooter.is_some_and(|k| self.is_echo(k)) {
                    self.tally.echo_dropped += 1;
                    return;
                }
                events.push(SimEvent::Detonation {
                    shooter: shooter.and_then(|k| self.entity_of(k)),
                    target: target.and_then(|k| self.entity_of(k)),
                    position: *position,
                });
            }
            NetMessage::SimulationControl(_) => self.tally.control_ignored += 1,
            _ => self.tally.unsupported += 1,
        }
    }

    fn on_state(
        &mut self,
        now: SimTime,
        s: &NetEntityState,
        world: &mut dyn GhostWorld,
        events: &mut Vec<SimEvent>,
    ) {
        if self.is_echo(s.key) {
            self.tally.echo_dropped += 1;
            return;
        }
        if !is_finite(s) {
            self.tally.invalid_state += 1;
            return;
        }
        let remote = RemoteState {
            state: KinematicState {
                position: s.position,
                velocity: s.velocity,
                orientation: Quaternion::from_world_euler(s.orientation),
            },
            acceleration: s.acceleration,
            angular_velocity: s.angular_velocity,
            model: s.dead_reckoning,
        };
        if let Some(e) = self.entity_of(s.key) {
            if world.update_ghost(e, remote).is_ok() {
                self.ghosts.heard(s.key, now);
            } else {
                // The world refused the update: do not keep a ghost it may still hold.
                self.tally.world_errors += 1;
                self.drop_ghost(s.key, e, world, events);
            }
            return;
        }
        let Some(template) = self.type_map.resolve(s.entity_type.as_ref()).cloned() else {
            self.tally.unmapped += 1;
            if let (Some(t), true) = (
                &s.entity_type,
                self.unmapped_types.len() < MAX_UNMAPPED_TYPES,
            ) {
                self.unmapped_types.insert(t.clone());
            }
            return;
        };
        if self.ghosts.len() >= self.settings.ghosts.max {
            self.tally.ghosts_capped += 1;
            return;
        }
        let spec = GhostSpec {
            source: self.source.clone(),
            template: &template,
            force: s.force.clone(),
            name: s.name.clone(),
            remote,
        };
        let Ok(id) = world.create_ghost(spec) else {
            self.tally.world_errors += 1;
            return;
        };
        if self.identity.bind_remote(s.key, id).is_err() {
            // Unreachable after the checks above; never leave an unbound ghost behind.
            self.tally.world_errors += 1;
            let _ = world.remove_ghost(id);
            return;
        }
        self.ghosts.heard(s.key, now);
        events.push(SimEvent::EntityCreated {
            entity: id,
            template: Some(template),
        });
    }

    fn drop_ghost(
        &mut self,
        key: NetKey,
        e: EntityId,
        world: &mut dyn GhostWorld,
        events: &mut Vec<SimEvent>,
    ) {
        self.identity.forget(e);
        self.ghosts.forget(key);
        match world.remove_ghost(e) {
            Ok(()) => events.push(SimEvent::EntityRemoved { entity: e }),
            Err(_) => self.tally.world_errors += 1,
        }
    }

    fn expire(&mut self, now: SimTime, world: &mut dyn GhostWorld, events: &mut Vec<SimEvent>) {
        let mut expired = std::mem::take(&mut self.expired);
        self.ghosts
            .expired_into(now, self.settings.ghosts.timeout, &mut expired);
        for &key in &expired {
            match self.entity_of(key) {
                Some(e) => self.drop_ghost(key, e, world, events),
                None => self.ghosts.forget(key),
            }
        }
        self.expired = expired;
    }

    /// The owned entities in `order` (ascending id) that are due, then the events. Cleanup of
    /// removed entities runs even when the adapter is disabled; only sending needs it live.
    fn publish(
        &mut self,
        now: SimTime,
        owned: &[OwnedView],
        order: &[usize],
        meta: &BTreeMap<EntityId, OwnedMeta>,
        events: &[SimEvent],
    ) {
        let emit = self.live();
        let mut out = std::mem::take(&mut self.out);
        out.clear();
        if emit {
            for &i in order {
                if let Some(v) = owned.get(i) {
                    self.publish_state(now, v, meta, &mut out);
                }
            }
        }
        for ev in events {
            self.publish_event(owned, order, ev, emit, &mut out);
        }
        if let (false, Some(adapter)) = (out.is_empty(), self.adapter.as_mut()) {
            match adapter.publish(now, &out) {
                Ok(()) => self.tally.published += out.len() as u64,
                Err(e) => self.fault(&e),
            }
        }
        self.out = out;
    }

    fn publish_state(
        &mut self,
        now: SimTime,
        v: &OwnedView,
        meta: &BTreeMap<EntityId, OwnedMeta>,
        out: &mut Vec<NetMessage>,
    ) {
        let key = match self.identity.local_key(v.id) {
            Ok(key) => key,
            Err(IdentityError::KeysExhausted) => {
                if self.exhausted.insert(v.id) {
                    self.tally.keys_exhausted += 1;
                }
                return;
            }
            Err(_) => return,
        };
        let Some(basis) = self.tracker.evaluate(now, v.id, &v.state) else {
            return;
        };
        let m = meta.get(&v.id);
        out.push(NetMessage::EntityState(NetEntityState {
            key,
            entity_type: m
                .and_then(|m| m.template.as_ref())
                .and_then(|t| self.entity_types.get(t))
                .cloned(),
            force: v.force.clone(),
            name: m.and_then(|m| m.name.clone()),
            position: basis.state.position,
            velocity: basis.state.velocity,
            acceleration: basis.acceleration,
            angular_velocity: basis.angular_velocity,
            orientation: basis.state.orientation.to_world_euler(),
            dead_reckoning: basis.model,
        }));
    }

    fn publish_event(
        &mut self,
        owned: &[OwnedView],
        order: &[usize],
        ev: &SimEvent,
        emit: bool,
        out: &mut Vec<NetMessage>,
    ) {
        match ev {
            SimEvent::EntityRemoved { entity } => {
                // Guard: a removal message goes out only for an entity this tracker sent, so an
                // entity that never reached the peers (or a key refused as exhausted) is silent.
                let was_published = self.tracker.removed(*entity);
                self.exhausted.remove(entity);
                if let Some(key) = self.owned_key(*entity) {
                    self.identity.forget(*entity);
                    if was_published && emit {
                        out.push(NetMessage::EntityRemoved { key });
                    }
                } else if let Some(key) = self.identity.key(*entity) {
                    // The kernel removed a ghost of this adapter itself: drop its bookkeeping,
                    // tell nobody (it is not ours to announce).
                    self.identity.forget(*entity);
                    self.ghosts.forget(key);
                }
            }
            SimEvent::Fire {
                shooter,
                target,
                weapon,
            } if emit => {
                let position = order
                    .binary_search_by_key(&Some(*shooter), |&i| owned.get(i).map(|v| v.id))
                    .ok()
                    .and_then(|at| owned.get(*order.get(at)?))
                    .map(|v| v.state.position);
                if let (Some(shooter), Some(position)) = (self.owned_key(*shooter), position) {
                    out.push(NetMessage::Fire {
                        shooter,
                        target: self.identity.key(*target),
                        weapon: weapon.clone(),
                        position,
                    });
                }
            }
            SimEvent::Detonation {
                shooter: Some(shooter),
                target,
                position,
            } if emit => {
                if let Some(shooter) = self.owned_key(*shooter) {
                    out.push(NetMessage::Detonation {
                        shooter: Some(shooter),
                        target: target.and_then(|t| self.identity.key(t)),
                        position: *position,
                    });
                }
            }
            _ => {}
        }
    }

    fn status(&self) -> AdapterStatus {
        let t = self.tally;
        AdapterStatus {
            name: self.settings.name.clone(),
            enabled: self.enabled,
            faulted: self.faulted,
            key_space: self.key_space,
            ghosts: self.ghosts.len(),
            published: t.published,
            received: t.received,
            echo_dropped: t.echo_dropped,
            unmapped: t.unmapped,
            keys_exhausted: t.keys_exhausted,
            ghosts_capped: t.ghosts_capped,
            control_ignored: t.control_ignored,
            world_errors: t.world_errors,
            unsupported: t.unsupported,
            invalid_state: t.invalid_state,
            fault_reason: self.fault_reason.clone(),
            unmapped_types: self.unmapped_types.iter().cloned().collect(),
            counters: self
                .adapter
                .as_ref()
                .map_or(self.last_counters, |a| a.counters()),
        }
    }
}

/// Owns the started adapters in scenario order and runs the per-tick ingest and publish steps.
pub struct NetworkManager {
    runners: Vec<Runner>,
    /// Indices into `owned` in ascending id, reused every tick.
    order: Vec<usize>,
}

impl fmt::Debug for NetworkManager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NetworkManager")
            .field(
                "adapters",
                &self
                    .runners
                    .iter()
                    .map(|r| r.settings.name.as_str())
                    .collect::<Vec<_>>(),
            )
            .finish_non_exhaustive()
    }
}

impl NetworkManager {
    /// Starts every adapter, in order.
    ///
    /// # Errors
    /// The first adapter's [`NetError`]; adapters started before it are stopped.
    pub fn new(slots: Vec<AdapterSlot>, now: SimTime) -> Result<Self, NetError> {
        let mut runners: Vec<Runner> = Vec::with_capacity(slots.len());
        for slot in slots {
            match slot.config.start(&AdapterStart { now }) {
                Ok(adapter) => runners.push(Runner::new(slot, adapter)),
                Err(e) => {
                    runners.iter_mut().for_each(Runner::stop);
                    return Err(e);
                }
            }
        }
        Ok(Self {
            runners,
            order: Vec::new(),
        })
    }

    /// Polls the enabled adapters in scenario order and applies the messages in delivery order:
    /// creates, updates and removes ghosts, times stale ghosts out (also those of disabled
    /// adapters), and appends the resulting events (ASM-248) to `events`.
    pub fn ingest(&mut self, now: SimTime, world: &mut dyn GhostWorld, events: &mut Vec<SimEvent>) {
        for r in &mut self.runners {
            r.poll(now);
            r.apply_inbox(now, world, events);
            r.expire(now, world, events);
        }
    }

    /// Publishes the owned entities that are due on every enabled adapter, and the `Fire` and
    /// `Detonation` events in `events` whose source is an owned entity (ASM-278). Entities that
    /// are ghosts of any adapter are skipped even when present in `owned` (REQ-K-5). Owned
    /// entities are visited in `EntityId` order whatever the order of `owned`. An
    /// `EntityRemoved` in `events` for a published entity sends its removal and frees its
    /// bookkeeping on every adapter, enabled or not.
    pub fn publish(
        &mut self,
        now: SimTime,
        owned: &[OwnedView],
        meta: &BTreeMap<EntityId, OwnedMeta>,
        events: &[SimEvent],
    ) {
        let mut order = std::mem::take(&mut self.order);
        order.clear();
        order.extend((0..owned.len()).filter(|&i| {
            owned
                .get(i)
                .is_some_and(|v| !self.runners.iter().any(|r| r.holds_ghost(v.id)))
        }));
        order.sort_by_key(|&i| owned.get(i).map(|v| v.id));
        for r in &mut self.runners {
            r.publish(now, owned, &order, meta, events);
        }
        self.order = order;
    }

    /// Publishes a run-control action on adapters whose policy has `control` set (ASM-244).
    pub fn publish_control(&mut self, now: SimTime, action: ControlAction) {
        let msg = [NetMessage::SimulationControl(action)];
        for r in &mut self.runners {
            if !(r.live() && r.settings.publish.control) {
                continue;
            }
            if let Some(adapter) = r.adapter.as_mut() {
                match adapter.publish(now, &msg) {
                    Ok(()) => r.tally.published += 1,
                    Err(e) => r.fault(&e),
                }
            }
        }
    }

    /// Enables or disables the adapter called `name`. Enabling restarts its publish tracker, so
    /// every owned entity is sent afresh (the peers' ghosts of it may have timed out).
    ///
    /// # Errors
    /// [`ManagerError::UnknownAdapter`]; [`ManagerError::Faulted`] when enabling an adapter that
    /// faulted (disabling one is allowed).
    pub fn set_enabled(
        &mut self,
        name: &str,
        enabled: bool,
    ) -> Result<AdapterStatus, ManagerError> {
        let r = self
            .runners
            .iter_mut()
            .find(|r| r.settings.name == name)
            .ok_or_else(|| ManagerError::UnknownAdapter(name.to_owned()))?;
        if enabled && r.faulted {
            return Err(ManagerError::Faulted(name.to_owned()));
        }
        if enabled && !r.enabled {
            r.tracker = PublishTracker::new(r.settings.publish);
        }
        r.enabled = enabled;
        Ok(r.status())
    }

    /// One status per adapter, in scenario order.
    #[must_use]
    pub fn status(&self) -> Vec<AdapterStatus> {
        self.runners.iter().map(Runner::status).collect()
    }

    /// How many adapters a fault has disabled so far (never decreases). The engine compares it
    /// before and after `ingest` and `publish` to see a new fault within the tick (ASM-252).
    #[must_use]
    pub fn faulted_count(&self) -> usize {
        self.runners.iter().filter(|r| r.faulted).count()
    }

    /// Stops every adapter. Idempotent; also done on drop.
    pub fn stop(&mut self) {
        self.runners.iter_mut().for_each(Runner::stop);
    }
}

impl Drop for NetworkManager {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
impl NetworkManager {
    /// Binds `key` to owned entity `e` in adapter `index`'s identity map, outside its key space.
    fn bind_owned_for_test(&mut self, index: usize, key: umbra_core::net::NetKey, e: EntityId) {
        assert!(self.runners[index].identity.bind_owned_for_test(key, e));
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
    /// A started pair of loopback adapters `on` (enabled) and `off` (disabled after publishing).
    fn two_slot_manager() -> NetworkManager {
        let now = SimTime::start(TickRate::new(30).unwrap());
        let hub = LoopbackHub::new(16);
        let slot = |name: &str, base: u64| {
            let config = hub
                .factory(
                    ComponentId::new(&format!("test.net.{name}")).unwrap(),
                    KeySpace { base, count: 100 },
                )
                .build(&serde_json::json!({}))
                .unwrap();
            let settings = AdapterSettings {
                name: name.to_owned(),
                enabled: true,
                publish: umbra_core::net::PublishSettings::default(),
                ghosts: crate::policy::GhostPolicy {
                    timeout: umbra_core::Seconds(1.0),
                    max: 10,
                    max_inbound_per_tick: 10,
                },
            };
            AdapterSlot::new(settings, config, TypeMap::default(), BTreeMap::new())
        };
        NetworkManager::new(vec![slot("on", 1000), slot("off", 2000)], now).unwrap()
    }

    /// A removed owned entity leaves nothing behind in the tracker or identity map of any
    /// adapter, including one that is disabled when the removal is reported.
    #[test]
    fn removal_clears_the_tracker_and_identity_of_every_adapter_even_disabled() {
        let rate = TickRate::new(30).unwrap();
        let e = EntityId::new(1).unwrap();
        let view = umbra_core::net::OwnedView {
            id: e,
            state: umbra_core::KinematicState {
                position: umbra_core::Ecef(umbra_core::Vec3::new(6_378_137.0, 0.0, 0.0)),
                velocity: umbra_core::Vec3::ZERO,
                orientation: umbra_core::Quaternion::IDENTITY,
            },
            force: None,
        };
        let mut manager = two_slot_manager();
        manager.publish(SimTime::at_tick(rate, 0), &[view], &BTreeMap::new(), &[]);
        assert_eq!(manager.runners[0].tracker.tracked_len(), 1);
        assert_eq!(manager.runners[1].tracker.tracked_len(), 1);
        manager.set_enabled("off", false).unwrap();

        manager.publish(
            SimTime::at_tick(rate, 1),
            &[],
            &BTreeMap::new(),
            &[SimEvent::EntityRemoved { entity: e }],
        );

        for r in &manager.runners {
            assert_eq!(r.tracker.tracked_len(), 0, "{}", r.settings.name);
            assert_eq!(r.identity.key(e), None, "{}", r.settings.name);
        }
    }
}
