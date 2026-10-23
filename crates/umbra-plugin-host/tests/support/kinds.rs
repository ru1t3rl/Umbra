//! Helpers of the ABI 0.2 host rows (M7 WI-2, AT-16): the fixture plugin `test.plugin.kinds` (feature
//! `kinds` of `umbra-test-plugin`, capabilities listed in its `kinds` module; until WI-9 the transitional
//! feature in [`FEATURES`]) loaded into a host, and small terrains and states the rows share. The
//! library is built by the shared build helper (nested `cargo build`, `CARGO_TARGET_TMPDIR`), never read
//! from `target/plugins`.
//!
//! The raw-ABI fixture (M7 WI-8, section "raw-ABI motion model and sensor" below) plays the plugin side
//! of a motion model and a sensor by hand, for the misuse the safe SDK cannot express: a terrain token
//! kept from a finished call, a token of another capability's call or of another service, and a token
//! used from another thread.
#![expect(clippy::expect_used, reason = "helpers build known-valid fixtures")]

use std::sync::Arc;

use umbra_core::{
    Action, ActionInstance, BbType, BbValue, BehaviorContext, Blackboard, BuildError, ComponentId,
    Condition, Contact, ContactBuffer, EntityId, EntityView, Enu, Factory, Geodetic, Intent,
    IntentBuffer, KinematicState, Meters, MotionModel, Observer, Ownership, ParamValue,
    PluginFaultPolicy, Quaternion, Registry, RngStream, Seconds, SenseContext, Sensor, SimRng,
    SimTime, Status, StreamKey, Target, TerrainProvider, TickRate, Vec3,
};
use umbra_plugin_host::{HostMode, HostOptions, PluginHost};

use super::Recorder;
use super::plugins::shared;

/// The fixture plugin loaded into a host and registered.
pub struct Env {
    pub host: PluginHost,
    pub registry: Registry,
    pub log: Arc<Recorder>,
}

/// The cargo features the fixture is built with. Until M7 WI-9 bridges conditions and actions, the
/// transitional `kinds_motion_sensor` build (motion models and sensors only, ASM-718): a host with only
/// the WI-8 bridges refuses the full `kinds` build as `Unsupported` (ASM-708). WI-9 sets `["kinds"]`.
pub const FEATURES: &[&str] = &["kinds_motion_sensor"];

/// Loads `test.plugin.kinds` (the library carries no manifest, which is optional) under the default
/// fault policy (`Abort`).
pub fn env() -> Env {
    env_with(PluginFaultPolicy::Abort)
}

/// [`env`] under `policy`.
pub fn env_with(policy: PluginFaultPolicy) -> Env {
    let lib = shared::library("umbra-test-plugin", FEATURES);
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

// ------------------------------------------------------------- raw-ABI motion model and sensor (WI-8)

use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::mem::size_of;
use std::sync::atomic::{AtomicUsize, Ordering};

use umbra_plugin_api::{
    UMBRA_ABI_VERSION_MAJOR, UMBRA_CAPABILITY_MOTION_MODEL, UMBRA_CAPABILITY_SENSOR,
    UMBRA_ERR_INTERNAL, UMBRA_OK, UmbraCode, UmbraContact, UmbraContactSink, UmbraErrorSink,
    UmbraKinematicState, UmbraMotionModel, UmbraMotionStep, UmbraMotionVTable, UmbraSenseCall,
    UmbraSensor, UmbraSensorVTable, UmbraStr, UmbraToken, UmbraVec3,
};

use super::forge::{self, Forged, Parts, leak_str};

/// The raw motion model capability.
pub const RAW_MOTION: &str = "test.motion.raw";
/// The raw sensor capability.
pub const RAW_SENSOR: &str = "test.sensor.raw";
/// Written into an out-value before a query, so a query that wrote nothing is visible.
pub const UNTOUCHED: f64 = -12_345.0;

/// The terrain `height_at` entry as the plugin sees it.
pub type HeightFn = unsafe extern "C" fn(UmbraToken, *const UmbraVec3, *mut f64) -> UmbraCode;

thread_local! {
    /// A terrain service kept from a call (`keep` modes), used by a later call (`stale` mode).
    pub static KEPT: Cell<Option<(UmbraToken, HeightFn)>> = const { Cell::new(None) };
    /// `(code, value)` of every terrain query the raw capabilities made, in order.
    pub static QUERIES: RefCell<Vec<(i32, f64)>> = const { RefCell::new(Vec::new()) };
}

/// The queries recorded on this thread.
pub fn queries() -> Vec<(i32, f64)> {
    QUERIES.with(|q| q.borrow().clone())
}

fn record(code: i32, value: f64) {
    QUERIES.with(|q| q.borrow_mut().push((code, value)));
}

/// A terrain that counts its `height_at` queries and answers like [`Slope`].
#[derive(Debug, Default)]
pub struct Counting {
    pub slope: f64,
    pub calls: AtomicUsize,
}

impl Counting {
    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl TerrainProvider for Counting {
    fn height_at(&self, at: Geodetic) -> Meters {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Slope {
            per_rad: self.slope,
        }
        .height_at(at)
    }
    fn surface_normal_at(&self, at: Geodetic) -> Enu {
        Slope {
            per_rad: self.slope,
        }
        .surface_normal_at(at)
    }
}

/// The panic message of [`Panicking`].
pub const PROVIDER_PANIC: &str = "terrain provider panics on purpose";

/// A terrain provider whose every query panics with [`PROVIDER_PANIC`] (a host-side bug).
#[derive(Debug)]
pub struct Panicking;

impl TerrainProvider for Panicking {
    fn height_at(&self, _: Geodetic) -> Meters {
        panic!("{PROVIDER_PANIC}")
    }
    fn surface_normal_at(&self, _: Geodetic) -> Enu {
        panic!("{PROVIDER_PANIC}")
    }
}

/// The mode of a raw capability, from its params `{"mode":"<mode>"}` (substring parse: no dependency).
fn mode_of(params: UmbraStr) -> String {
    // SAFETY: the host passes the params borrowed for the duration of `configure`.
    let bytes = unsafe { std::slice::from_raw_parts(params.ptr, params.len) };
    let text = String::from_utf8_lossy(bytes);
    text.split("\"mode\":\"")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .unwrap_or("query")
        .to_owned()
}

/// Queries `height_at(ctx, p)` with `f` on this thread; records and returns the code and the value.
pub fn query(f: HeightFn, ctx: UmbraToken, p: UmbraVec3) -> (i32, f64) {
    let mut out = UNTOUCHED;
    // SAFETY: `f` is the host's `height_at` entry (a process-lifetime function); `p` and `out` are live
    // locals. Whether the token is still valid is what the rows test: the host checks it, never
    // dereferences it.
    let code = unsafe { f(ctx, &raw const p, &raw mut out) };
    record(code, out);
    (code, out)
}

/// Queries from a new thread (no call record there) and records what it got.
fn query_from_another_thread(f: HeightFn, ctx: UmbraToken, p: UmbraVec3) {
    let got = std::thread::spawn(move || {
        let mut out = UNTOUCHED;
        // SAFETY: as in `query`; the locals live on the spawned thread for the call.
        let code = unsafe { f(ctx, &raw const p, &raw mut out) };
        (code, out)
    })
    .join()
    .unwrap_or((UMBRA_ERR_INTERNAL, UNTOUCHED));
    record(got.0, got.1);
}

/// Runs `mode` against the terrain service `ctx`/`height` of the current call at `p`; `other` is the
/// token of another service of the same call (the sensor's contact sink), if any. Returns the code the
/// capability returns: the query's code in mode `query_code`, else `UMBRA_OK`.
fn terrain_mode(
    mode: &str,
    ctx: UmbraToken,
    height: HeightFn,
    p: UmbraVec3,
    other: UmbraToken,
) -> UmbraCode {
    match mode {
        "keep" => {
            KEPT.with(|k| k.set(Some((ctx, height))));
            query(height, ctx, p);
        }
        "stale" => {
            if let Some((kept, f)) = KEPT.with(Cell::get) {
                query(f, kept, p);
            }
        }
        "thread" => query_from_another_thread(height, ctx, p),
        "sink_token" => {
            query(height, other, p);
        }
        "query_code" => return query(height, ctx, p).0,
        _ => {
            query(height, ctx, p);
        }
    }
    UMBRA_OK
}

unsafe extern "C" fn motion_configure(
    params: UmbraStr,
    out: *mut *mut UmbraMotionModel,
    _: *const UmbraErrorSink,
) -> UmbraCode {
    let mode = Box::new(mode_of(params));
    // SAFETY: the host passes a valid out pointer.
    unsafe { *out = Box::into_raw(mode).cast() };
    UMBRA_OK
}

unsafe extern "C" fn motion_destroy(model: *mut UmbraMotionModel) {
    // SAFETY: a handle made by `motion_configure`, destroyed once.
    drop(unsafe { Box::from_raw(model.cast::<String>()) });
}

/// Mode `record`: records `(tick, dt)`, `(rate_hz, 0)` and `(kind, value)` of every intent of the step.
#[expect(
    clippy::cast_possible_wrap,
    reason = "small test values recorded as i32 codes"
)]
fn record_step(step: &UmbraMotionStep) {
    record(step.tick as i32, step.dt);
    record(step.rate_hz as i32, 0.0);
    if step.intent_count == 0 {
        return;
    }
    // SAFETY: the host passes `intent_count` intents for the call, with the plugin's stride.
    let intents = unsafe { std::slice::from_raw_parts(step.intents, step.intent_count) };
    for i in intents {
        record(i.kind as i32, i.value);
    }
}

/// Mode `record` of the sensor: records `(tick, rate_hz)`, then `(ownership, id)` of the observer and
/// of every candidate.
#[expect(
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    reason = "small test values recorded as i32 codes and f64 values"
)]
fn record_sense(call: &UmbraSenseCall) {
    record(call.tick as i32, f64::from(call.rate_hz));
    // SAFETY: the host passes a valid observer for the call.
    let observer = unsafe { &*call.observer };
    record(observer.ownership as i32, observer.id as f64);
    if call.candidate_count == 0 {
        return;
    }
    // SAFETY: the host passes `candidate_count` targets for the call, with the plugin's stride.
    let candidates = unsafe { std::slice::from_raw_parts(call.candidates, call.candidate_count) };
    for c in candidates {
        record(c.ownership as i32, c.id as f64);
    }
}

/// Runs its mode against the step's terrain and returns `UMBRA_OK` whatever the host answered (except
/// mode `query_code`), so a refusal faults the capability through the host's own record, not through
/// the code.
unsafe extern "C" fn motion_step(
    model: *mut UmbraMotionModel,
    step: *const UmbraMotionStep,
    _: *mut UmbraKinematicState,
    _: *const UmbraErrorSink,
) -> UmbraCode {
    // SAFETY: a live handle; the host passes a valid step record with a valid terrain and state.
    let (mode, step) = unsafe { (&*model.cast::<String>(), &*step) };
    if mode == "record" {
        record_step(step);
        return UMBRA_OK;
    }
    // SAFETY: as above.
    let (terrain, state) = unsafe { (&*step.terrain, &*step.state) };
    let Some(height) = terrain.height_at else {
        return UMBRA_ERR_INTERNAL;
    };
    terrain_mode(mode, terrain.ctx, height, state.position, 0)
}

unsafe extern "C" fn sensor_configure(
    params: UmbraStr,
    out: *mut *mut UmbraSensor,
    _: *const UmbraErrorSink,
) -> UmbraCode {
    let mode = Box::new(mode_of(params));
    // SAFETY: the host passes a valid out pointer.
    unsafe { *out = Box::into_raw(mode).cast() };
    UMBRA_OK
}

unsafe extern "C" fn sensor_destroy(sensor: *mut UmbraSensor) {
    // SAFETY: a handle made by `sensor_configure`, destroyed once.
    drop(unsafe { Box::from_raw(sensor.cast::<String>()) });
}

/// Mode `push_then_fail`: pushes a valid contact for the first candidate (recording the sink's code
/// with the value 0), then fails the call with `UMBRA_ERR_INTERNAL`.
fn push_then_fail(call: &UmbraSenseCall, sink: &UmbraContactSink) -> UmbraCode {
    let (Some(push), false) = (sink.push, call.candidates.is_null()) else {
        return UMBRA_ERR_INTERNAL;
    };
    // SAFETY: a non-null candidate array of `candidate_count >= 1` targets, valid for the call.
    let first = unsafe { &*call.candidates };
    let contact = UmbraContact {
        abi_version: abi2(),
        struct_size: size_of::<UmbraContact>() as u32,
        entity: first.id,
        position: first.position,
        velocity: first.velocity,
        force: first.force,
        range_m: 1.0,
    };
    // SAFETY: the host's sink entry and token of this call; `contact` lives across the call.
    record(unsafe { push(sink.ctx, &raw const contact) }, 0.0);
    UMBRA_ERR_INTERNAL
}

/// Runs its mode against the call's terrain at the observer and pushes nothing (except in mode
/// `push_then_fail`); returns `UMBRA_OK` (except modes `push_then_fail` and `query_code`).
unsafe extern "C" fn sensor_sense(
    sensor: *mut UmbraSensor,
    call: *const UmbraSenseCall,
    sink: *const UmbraContactSink,
    _: *const UmbraErrorSink,
) -> UmbraCode {
    // SAFETY: a live handle; the host passes a valid call record, observer, terrain and sink.
    let (mode, call, sink) = unsafe { (&*sensor.cast::<String>(), &*call, &*sink) };
    if mode == "push_then_fail" {
        return push_then_fail(call, sink);
    }
    if mode == "record" {
        record_sense(call);
        return UMBRA_OK;
    }
    // SAFETY: as above.
    let (terrain, observer) = unsafe { (&*call.terrain, &*call.observer) };
    let Some(height) = terrain.height_at else {
        return UMBRA_ERR_INTERNAL;
    };
    terrain_mode(mode, terrain.ctx, height, observer.position, sink.ctx)
}

/// `major << 16 | 2`.
const fn abi2() -> u32 {
    (UMBRA_ABI_VERSION_MAJOR << 16) | 2
}

/// The raw plugin `test.plugin.raw_kinds` (ABI 0.2) as forge parts: the motion model [`RAW_MOTION`]
/// and the sensor [`RAW_SENSOR`], both with the param `mode`.
pub fn raw_parts() -> Parts {
    let schema = r#"{"type":"object","properties":{"mode":{"type":"string"}}}"#;
    let motion: &'static UmbraMotionVTable = Box::leak(Box::new(UmbraMotionVTable {
        abi_version: abi2(),
        struct_size: size_of::<UmbraMotionVTable>() as u32,
        configure: Some(motion_configure),
        destroy: Some(motion_destroy),
        step: Some(motion_step),
    }));
    let sensor: &'static UmbraSensorVTable = Box::leak(Box::new(UmbraSensorVTable {
        abi_version: abi2(),
        struct_size: size_of::<UmbraSensorVTable>() as u32,
        configure: Some(sensor_configure),
        destroy: Some(sensor_destroy),
        sense: Some(sensor_sense),
    }));
    let mut p = forge::parts();
    p.desc.abi_version = abi2();
    p.desc.id = leak_str("test.plugin.raw_kinds");
    p.caps.clear();
    for (id, kind, vtable) in [
        (
            RAW_MOTION,
            UMBRA_CAPABILITY_MOTION_MODEL,
            std::ptr::from_ref(motion).cast::<c_void>(),
        ),
        (
            RAW_SENSOR,
            UMBRA_CAPABILITY_SENSOR,
            std::ptr::from_ref(sensor).cast::<c_void>(),
        ),
    ] {
        let mut cap = forge::capability(id);
        cap.abi_version = abi2();
        cap.kind = kind;
        cap.param_schema = leak_str(schema);
        cap.vtable = vtable;
        p.caps.push(cap);
    }
    p.desc.capabilities = p.caps.as_ptr();
    p.desc.capability_count = p.caps.len();
    p
}

/// The raw kinds plugin loaded into a host and registered. Build it with
/// `RawEnv::load(forged!(raw_parts()), policy)` inside the test function: one `forged!` call site per
/// test (see `forge.rs`).
pub struct RawEnv {
    pub host: PluginHost,
    pub registry: Registry,
    pub log: Arc<Recorder>,
    pub f: Forged,
}

impl RawEnv {
    /// Loads the forged plugin into a new run-mode host, sets `policy` and registers it.
    pub fn load(f: Forged, policy: PluginFaultPolicy) -> Self {
        Self::load_in(f, policy, HostMode::Run)
    }

    /// [`RawEnv::load`] into a host of `mode`.
    pub fn load_in(f: Forged, policy: PluginFaultPolicy, mode: HostMode) -> Self {
        QUERIES.with(|q| q.borrow_mut().clear());
        KEPT.with(|k| k.set(None));
        let log = Recorder::arc();
        let mut options = HostOptions::default();
        options.log = log.clone();
        options.mode = mode;
        let mut host = PluginHost::new(options);
        host.load_static("raw-kinds-origin", f.entry)
            .expect("load the raw kinds plugin");
        host.set_fault_policy(policy);
        let mut registry = Registry::new();
        host.register(&mut registry)
            .expect("register the raw kinds plugin");
        Self {
            host,
            registry,
            log,
            f,
        }
    }

    fn params(mode: &str) -> ParamValue {
        params(&format!(r#"{{"mode":"{mode}"}}"#))
    }

    /// The raw motion model in `mode`.
    pub fn motion(&self, mode: &str) -> Arc<dyn MotionModel> {
        let factory = self
            .registry
            .motion_model(&cid(RAW_MOTION))
            .expect("the raw motion model is registered");
        let _scope = self.host.build_scope();
        factory.build(&Self::params(mode)).expect("build")
    }

    /// The raw sensor in `mode`.
    pub fn sensor(&self, mode: &str) -> Arc<dyn Sensor> {
        let factory = self
            .registry
            .sensor(&cid(RAW_SENSOR))
            .expect("the raw sensor is registered");
        let _scope = self.host.build_scope();
        factory.build(&Self::params(mode)).expect("build")
    }

    /// The faults as `(capability, call, message)`.
    pub fn faults(&self) -> Vec<(String, &'static str, String)> {
        self.host
            .faults()
            .into_iter()
            .map(|f| (f.capability.as_str().to_owned(), f.call, f.message))
            .collect()
    }
}
