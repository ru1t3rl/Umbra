//! Shared helpers for the `umbra-net` integration tests (M6 step 4): a fake ghost world, a loopback
//! rig with one manager endpoint and one raw peer endpoint, and message builders.
#![expect(
    dead_code,
    reason = "each test binary uses a subset of the shared helpers"
)]
#![expect(clippy::unwrap_used, reason = "test code")]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

use std::collections::BTreeMap;
use std::sync::Arc;

use umbra_core::dead_reckoning::{DeadReckoning, RemoteState};
use umbra_core::net::{
    AdapterStart, KeySpace, NetEntityState, NetEntityType, NetKey, NetMessage, NetworkAdapter,
    NetworkAdapterConfig, OwnedView,
};
use umbra_core::{
    ComponentId, Ecef, EntityId, Force, KinematicState, Quaternion, Radians, RemoteSource, Seconds,
    SimEvent, SimTime, TemplateId, TickRate, Vec3, WorldEuler,
};
use umbra_net::{
    AdapterSettings, AdapterSlot, GhostError, GhostPolicy, GhostSpec, GhostWorld, LoopbackHub,
    NetworkManager, PublishPolicy, TypeMap,
};

pub const A: f64 = 6_378_137.0;

pub fn rate() -> TickRate {
    TickRate::new(30).expect("non-zero rate")
}

pub fn tick(n: u64) -> SimTime {
    SimTime::at_tick(rate(), n)
}

pub fn eid(raw: u64) -> EntityId {
    EntityId::new(raw).expect("non-zero")
}

pub fn tid(s: &str) -> TemplateId {
    TemplateId::new(s).expect("valid template id")
}

pub fn cid(s: &str) -> ComponentId {
    ComponentId::new(s).expect("valid component id")
}

pub fn ty(s: &str) -> NetEntityType {
    NetEntityType::new(s)
}

pub fn at(y_m: f64) -> Ecef {
    Ecef(Vec3::new(A, y_m, 0.0))
}

pub fn kin(y_m: f64) -> KinematicState {
    KinematicState {
        position: at(y_m),
        velocity: Vec3::ZERO,
        orientation: Quaternion::IDENTITY,
    }
}

pub fn view(id: EntityId, y_m: f64) -> OwnedView {
    OwnedView {
        id,
        state: kin(y_m),
        force: Force::new("blue").ok(),
    }
}

/// An inbound entity state at `y_m` metres east, moving 5 m/s east, with the given type token.
pub fn state(key: u64, entity_type: Option<&str>, y_m: f64) -> NetMessage {
    NetMessage::EntityState(NetEntityState {
        key: NetKey(key),
        entity_type: entity_type.map(ty),
        force: Force::new("red").ok(),
        name: Some(Arc::from(format!("peer-{key}"))),
        position: at(y_m),
        velocity: Vec3::new(0.0, 5.0, 0.0),
        acceleration: Vec3::ZERO,
        angular_velocity: Vec3::ZERO,
        orientation: WorldEuler {
            yaw: Radians(0.0),
            pitch: Radians(0.0),
            roll: Radians(0.0),
        },
        dead_reckoning: DeadReckoning::Velocity,
    })
}

/// A slot without entity types.
pub fn slot(
    settings: AdapterSettings,
    config: Arc<dyn NetworkAdapterConfig>,
    types: TypeMap,
) -> AdapterSlot {
    AdapterSlot::new(settings, config, types, BTreeMap::new())
}

pub fn publish_policy() -> PublishPolicy {
    PublishPolicy {
        position_threshold_m: 1.0,
        orientation_threshold: Radians(3.0_f64.to_radians()),
        heartbeat: Seconds(5.0),
        min_interval: Seconds(0.0),
        model: DeadReckoning::Velocity,
        control: false,
    }
}

pub fn ghost_policy() -> GhostPolicy {
    GhostPolicy {
        timeout: Seconds(1.0),
        max: 100,
        max_inbound_per_tick: 100,
    }
}

pub fn settings(name: &str) -> AdapterSettings {
    AdapterSettings {
        name: name.to_owned(),
        enabled: true,
        publish: publish_policy(),
        ghosts: ghost_policy(),
    }
}

/// `tank` maps to `tank.mbt`; nothing else maps.
pub fn type_map() -> TypeMap {
    TypeMap::new(&[(tid("tank.mbt"), ty("tank"))], &[], None).0
}

pub fn type_map_with_default(default: &str) -> TypeMap {
    TypeMap::new(&[(tid("tank.mbt"), ty("tank"))], &[], Some(tid(default))).0
}

/// One ghost as the fake world stores it.
#[derive(Debug, Clone, PartialEq)]
pub struct FakeGhost {
    pub source: RemoteSource,
    pub template: TemplateId,
    pub force: Option<Force>,
    pub name: Option<Arc<str>>,
    pub remote: RemoteState,
}

/// A `GhostWorld` over a map; ids count up from 100.
#[derive(Default)]
pub struct FakeWorld {
    pub ghosts: BTreeMap<EntityId, FakeGhost>,
    pub created: usize,
    pub updated: usize,
    /// When set, `update_ghost` refuses (the ghost stays in `ghosts` until removed).
    pub refuse_updates: bool,
    next: u64,
}

impl GhostWorld for FakeWorld {
    fn create_ghost(&mut self, spec: GhostSpec<'_>) -> Result<EntityId, GhostError> {
        self.next += 1;
        let id = eid(100 + self.next);
        self.created += 1;
        self.ghosts.insert(
            id,
            FakeGhost {
                source: spec.source,
                template: spec.template.clone(),
                force: spec.force,
                name: spec.name,
                remote: spec.remote,
            },
        );
        Ok(id)
    }

    fn update_ghost(&mut self, id: EntityId, remote: RemoteState) -> Result<(), GhostError> {
        if self.refuse_updates {
            return Err(GhostError::World("updates refused".to_owned()));
        }
        let ghost = self
            .ghosts
            .get_mut(&id)
            .ok_or_else(|| GhostError::World(format!("no ghost {id}")))?;
        ghost.remote = remote;
        self.updated += 1;
        Ok(())
    }

    fn remove_ghost(&mut self, id: EntityId) -> Result<(), GhostError> {
        self.ghosts
            .remove(&id)
            .map(|_| ())
            .ok_or_else(|| GhostError::World(format!("no ghost {id}")))
    }
}

/// A manager with one loopback adapter named `lb` (keys 1001..=1100) and a raw peer endpoint on
/// the same hub (keys 2001..=2100).
pub struct Rig {
    pub manager: NetworkManager,
    pub peer: Box<dyn NetworkAdapter>,
    pub world: FakeWorld,
    pub hub: Arc<LoopbackHub>,
}

pub const OWN_BASE: u64 = 1000;
pub const PEER_BASE: u64 = 2000;

pub fn start_ctx() -> AdapterStart {
    AdapterStart { now: tick(0) }
}

pub fn config(
    hub: &Arc<LoopbackHub>,
    id: &str,
    base: u64,
    params: &serde_json::Value,
) -> Arc<dyn NetworkAdapterConfig> {
    // The factory input is the ADR 0030 wrapper; `params` is the adapter's own table.
    let input = serde_json::json!({"name": "test", "params": params, "templates": {}});
    hub.factory(cid(id), KeySpace { base, count: 100 })
        .build(&input)
        .unwrap()
}

pub fn peer_on(hub: &Arc<LoopbackHub>, id: &str) -> Box<dyn NetworkAdapter> {
    config(hub, id, PEER_BASE, &serde_json::json!({}))
        .start(&start_ctx())
        .unwrap()
}

impl Rig {
    pub fn new() -> Self {
        Self::with(&serde_json::json!({}), settings("lb"), type_map())
    }

    pub fn with(params: &serde_json::Value, settings: AdapterSettings, types: TypeMap) -> Self {
        let hub = LoopbackHub::new(1024);
        let cfg = config(&hub, "test.net.loopback", OWN_BASE, params);
        let manager = NetworkManager::new(vec![slot(settings, cfg, types)], tick(0)).unwrap();
        let peer = peer_on(&hub, "test.net.peer");
        Self {
            manager,
            peer,
            world: FakeWorld::default(),
            hub,
        }
    }

    pub fn peer_sends(&mut self, now: u64, msgs: &[NetMessage]) {
        self.peer.publish(tick(now), msgs).unwrap();
    }

    /// Ingests at tick `now`, returning the events.
    pub fn ingest(&mut self, now: u64) -> Vec<SimEvent> {
        let mut events = Vec::new();
        self.manager.ingest(tick(now), &mut self.world, &mut events);
        events
    }

    /// Publishes `owned` at tick `now` with no meta and no entity types.
    pub fn publish(&mut self, now: u64, owned: &[OwnedView]) {
        self.manager
            .publish(tick(now), owned, &BTreeMap::new(), &[]);
    }

    /// Everything the peer has received so far.
    pub fn peer_polls(&mut self, now: u64) -> Vec<NetMessage> {
        let mut into = Vec::new();
        self.peer.poll(tick(now), 10_000, &mut into).unwrap();
        into
    }

    pub fn status(&self) -> umbra_net::AdapterStatus {
        self.manager.status().remove(0)
    }
}
