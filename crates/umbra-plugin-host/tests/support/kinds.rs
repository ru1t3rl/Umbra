//! Helpers of the ABI 0.2 host rows (M7 WI-2, AT-16): the fixture plugin `test.plugin.kinds` (feature
//! `kinds` of `umbra-test-plugin`, capabilities listed in its `kinds` module) loaded into a host, and
//! small terrains and states the rows share. The library is built by the shared build helper (nested
//! `cargo build`, `CARGO_TARGET_TMPDIR`), never read from `target/plugins`.
#![expect(clippy::expect_used, reason = "helpers build known-valid fixtures")]

use std::sync::Arc;

use umbra_core::{
    Action, ActionInstance, BbType, BbValue, BehaviorContext, Blackboard, BuildError, ComponentId,
    Condition, Contact, ContactBuffer, EntityId, EntityView, Enu, Factory, Geodetic, Intent,
    IntentBuffer, KinematicState, Meters, MotionModel, Observer, Ownership, ParamValue,
    PluginFaultPolicy, Quaternion, Registry, RngStream, Seconds, SenseContext, Sensor, SimRng,
    SimTime, Status, StreamKey, Target, TerrainProvider, TickRate, Vec3,
};
use umbra_plugin_host::{HostOptions, PluginHost};

use super::Recorder;
use super::plugins::shared;

/// The fixture plugin loaded into a host and registered.
pub struct Env {
    pub host: PluginHost,
    pub registry: Registry,
    pub log: Arc<Recorder>,
}

/// Loads `test.plugin.kinds` (the library carries no manifest, which is optional) under the default
/// fault policy (`Abort`).
pub fn env() -> Env {
    env_with(PluginFaultPolicy::Abort)
}

/// [`env`] under `policy`.
pub fn env_with(policy: PluginFaultPolicy) -> Env {
    let lib = shared::library("umbra-test-plugin", &["kinds"]);
    let dir = shared::plugins_dir(&[("kinds", &lib)]);
    let log = Recorder::arc();
    let mut options = HostOptions::default();
    options.log = log.clone();
    let mut host = PluginHost::new(options);
    for package in PluginHost::discover(&[dir]).expect("discover the kinds fixture") {
        host.load(&package)
            .unwrap_or_else(|e| panic!("load the kinds fixture: {e}"));
    }
    host.set_fault_policy(policy);
    let mut registry = Registry::new();
    host.register(&mut registry)
        .unwrap_or_else(|e| panic!("register the kinds fixture: {e}"));
    Env {
        host,
        registry,
        log,
    }
}

fn cid(id: &str) -> ComponentId {
    ComponentId::new(id).expect("valid id")
}

fn params(json: &str) -> ParamValue {
    serde_json::from_str(json).expect("params are JSON")
}

impl Env {
    /// Builds `id` from `json` params through the factory `lookup` finds in the registry, inside the
    /// host's build scope.
    fn build<T: ?Sized>(
        &self,
        what: &str,
        lookup: impl for<'r> Fn(&'r Registry, &ComponentId) -> Option<&'r Arc<dyn Factory<T>>>,
        id: &str,
        json: &str,
    ) -> Result<Arc<T>, BuildError> {
        let factory = lookup(&self.registry, &cid(id))
            .unwrap_or_else(|| panic!("{what} {id} is not registered"));
        let _scope = self.host.build_scope();
        factory.build(&params(json))
    }

    pub fn motion(&self, id: &str, json: &str) -> Result<Arc<dyn MotionModel>, BuildError> {
        self.build("motion model", Registry::motion_model, id, json)
    }

    pub fn sensor(&self, id: &str, json: &str) -> Result<Arc<dyn Sensor>, BuildError> {
        self.build("sensor", Registry::sensor, id, json)
    }

    pub fn condition(&self, id: &str, json: &str) -> Result<Arc<dyn Condition>, BuildError> {
        self.build("condition", Registry::condition, id, json)
    }

    pub fn action(&self, id: &str, json: &str) -> Result<Arc<dyn Action>, BuildError> {
        self.build("action", Registry::action, id, json)
    }
}

/// A terrain whose height is `per_rad` meters per radian of longitude, with a constant tilted
/// normal: every answer is a pure function of the query, so a plugin's echo can be compared exactly.
#[derive(Debug, Clone, Copy)]
pub struct Slope {
    pub per_rad: f64,
}

impl TerrainProvider for Slope {
    fn height_at(&self, at: Geodetic) -> Meters {
        Meters(self.per_rad * at.lon().0)
    }
    fn surface_normal_at(&self, _: Geodetic) -> Enu {
        Enu {
            east: 0.6,
            north: 0.0,
            up: 0.8,
        }
    }
}

/// A terrain with a 300 m wall for longitudes in `[lon_lo, lon_hi]` radians (else height 0), which
/// blocks the default line of sight across it.
#[derive(Debug, Clone, Copy)]
pub struct Wall {
    pub lon_lo: f64,
    pub lon_hi: f64,
}

impl TerrainProvider for Wall {
    fn height_at(&self, at: Geodetic) -> Meters {
        let lon = at.lon().0;
        Meters(if (self.lon_lo..=self.lon_hi).contains(&lon) {
            300.0
        } else {
            0.0
        })
    }
    fn surface_normal_at(&self, _: Geodetic) -> Enu {
        Enu {
            east: 0.0,
            north: 0.0,
            up: 1.0,
        }
    }
}

/// A state at latitude 52 deg, `lon_deg`, `alt_m` above the ellipsoid, moving `v` m/s in ECEF x,
/// level orientation.
pub fn state_at(lon_deg: f64, alt_m: f64, v: f64) -> KinematicState {
    KinematicState {
        position: Geodetic::from_degrees(52.0, lon_deg, alt_m)
            .expect("valid position")
            .to_ecef(),
        velocity: Vec3 {
            x: v,
            y: 0.0,
            z: 0.0,
        },
        orientation: Quaternion::IDENTITY,
    }
}

// ------------------------------------------------------------------------------------- drivers

/// The plugin id of the fixture.
pub const PLUGIN: &str = "test.plugin.kinds";

/// One tick at 30 Hz.
pub const DT: Seconds = Seconds(1.0 / 30.0);

pub fn eid(n: u64) -> EntityId {
    EntityId::new(n).expect("non-zero")
}

pub fn at(tick: u64) -> SimTime {
    SimTime::at_tick(TickRate::new(30).expect("30 Hz"), tick)
}

/// Asserts exactly one fault, of `capability`, in `call`, whose message contains `needle`.
pub fn assert_one_fault(env: &Env, capability: &str, call: &str, needle: &str) {
    let faults = env.host.faults();
    assert_eq!(faults.len(), 1, "{faults:?}");
    assert_eq!(faults[0].plugin.as_str(), PLUGIN);
    assert_eq!(faults[0].capability.as_str(), capability);
    assert_eq!(faults[0].call, call);
    assert!(
        faults[0].message.to_lowercase().contains(needle),
        "{needle:?} in {:?}",
        faults[0].message
    );
}

/// One motion step of `DT`.
pub fn step(
    model: &dyn MotionModel,
    state: &KinematicState,
    intents: &[Intent],
    terrain: &dyn TerrainProvider,
) -> KinematicState {
    model.step(state, intents, DT, terrain)
}

/// The observer's stream (entity `n`, seed 7).
pub fn stream(n: u64) -> RngStream {
    SimRng::new(7).stream(StreamKey::entity(eid(n)))
}

/// The observer (id 1, at 4.000 deg) and candidates at 4.001, 4.002 and 4.003 deg (ids 2..=4), all at
/// 50 m, pre-tick states.
pub fn world() -> Vec<(EntityId, KinematicState)> {
    [(1, 4.000), (2, 4.001), (3, 4.002), (4, 4.003)]
        .into_iter()
        .map(|(id, lon)| (eid(id), state_at(lon, 50.0, 0.0)))
        .collect()
}

/// Runs `sensor` for the observer (the first entity of `world`) against the others; returns its
/// contacts and the observer's stream afterwards.
pub fn sense(
    sensor: &dyn Sensor,
    world: &[(EntityId, KinematicState)],
    terrain: &dyn TerrainProvider,
) -> (Vec<Contact>, RngStream) {
    let owned = Ownership::Owned;
    let observer = Observer::new(world[0].0, &world[0].1, None);
    let candidates: Vec<Target<'_>> = world[1..]
        .iter()
        .map(|(id, state)| Target::new(*id, state, None, &owned))
        .collect();
    let mut rng = stream(world[0].0.get());
    let mut found = Vec::new();
    {
        let mut ctx = SenseContext::new(at(5), &mut rng, terrain);
        sensor.sense(
            &mut ctx,
            &observer,
            &candidates,
            &mut ContactBuffer::new(&mut found),
        );
    }
    (found, rng)
}

/// The ids of `contacts`.
pub fn ids(contacts: &[Contact]) -> Vec<u64> {
    contacts.iter().map(|c| c.entity.get()).collect()
}

/// Terrain at height 0 everywhere.
pub fn flat() -> Slope {
    Slope { per_rad: 0.0 }
}

/// One entity's call context pieces.
pub struct Ctx {
    pub id: EntityId,
    pub state: KinematicState,
    pub rng: RngStream,
}

impl Ctx {
    pub fn new(n: u64) -> Self {
        Self {
            id: eid(n),
            state: state_at(4.0, 50.0, 0.0),
            rng: stream(n),
        }
    }
}

/// A blackboard with the given bool keys, set where a value is given.
pub fn board(keys: &[(&str, Option<bool>)]) -> Blackboard {
    let mut bb = Blackboard::default();
    for (key, value) in keys {
        bb.declare(key, BbType::Bool);
        if let Some(v) = value {
            bb.set(key, BbValue::Bool(*v)).expect("declared bool key");
        }
    }
    bb
}

/// One condition check.
pub fn check(condition: &dyn Condition, ctx: &mut Ctx, bb: &mut Blackboard, tick: u64) -> bool {
    let mut context = BehaviorContext::new(at(tick), &mut ctx.rng);
    condition.check(&mut context, &EntityView::new(ctx.id, &ctx.state), bb)
}

/// One action tick; returns the status and the intents of the call.
pub fn tick(
    instance: &mut dyn ActionInstance,
    ctx: &mut Ctx,
    bb: &mut Blackboard,
    tick_no: u64,
) -> (Status, Vec<Intent>) {
    let mut out = Vec::new();
    let status = {
        let mut context = BehaviorContext::new(at(tick_no), &mut ctx.rng);
        instance.tick(
            &mut context,
            &EntityView::new(ctx.id, &ctx.state),
            bb,
            &mut IntentBuffer::new(&mut out),
        )
    };
    (status, out)
}

/// A fresh instance of the action `id` built with `params`.
pub fn instance(env: &Env, id: &str, params: &str) -> Box<dyn ActionInstance> {
    env.action(id, params)
        .expect("build")
        .instantiate(TickRate::new(30).expect("30 Hz"))
        .expect("instantiate")
}
