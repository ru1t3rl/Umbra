//! Shared test support for the M3 `cgf-behavior` tests: stub leaves with a log, a graph builder, a
//! runner that ticks an instance. `cgf-behavior` tests may not use `cgf-templates` or `cgf-motion`
//! (ASM-005), so graphs are built in Rust and leaves are stubs.
//!
//! Stub leaves (all log what they do into one shared [`Log`]):
//! - condition `stub.cond.flag { name }`: returns the flag `name` of [`Controls`]; logs `cond:<name>`.
//! - condition `stub.cond.store { key, value, flag }`: writes `value` to `key`, logs
//!   `store:<key>=<ok>` then `cond:<flag>`, and returns the flag `flag`.
//! - action `stub.action.script { name, script }`: returns the statuses of `script` in order (the
//!   last one repeats); logs `tick:<name>` and `halt:<name>`; emits `Custom { payload: <name> }`.
//! - action `stub.action.bb { op, key, label, value?, status? }`: `set`, `unset` or `read` a
//!   blackboard key and log `set:<label>=ok|<error>`, `read:<label>=<Option<BbValue>>`.
//! - action `stub.action.put { key, slot }`: copies the [`Controls`] slot into the key (unset when
//!   the slot is empty).
//!
//! Each test binary uses a subset of the helpers.
#![expect(
    dead_code,
    reason = "each test binary uses a subset of the shared helpers"
)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

use std::collections::BTreeMap;
use std::num::NonZeroU32;
use std::sync::{Arc, Mutex};

use cgf_behavior::{GraphFactory, register_graphs};
use cgf_core::{
    Action, ActionInstance, BbType, BbValue, Behavior, BehaviorContext, BehaviorId,
    BehaviorInstance, Blackboard, BlackboardError, BuildError, ComponentId, ComponentInfo,
    Composite, Condition, Decorator, Ecef, EntityId, EntityView, Factory, FsmDef, GraphDef,
    InstantiateError, Intent, IntentBuffer, Interface, KinematicState, LeafRef, NodeDef, NodeId,
    NodeKind, OutputPort, ParallelDef, ParallelPolicy, ParamSchema, ParamTemplate, ParamValue,
    Provenance, Quaternion, Registry, RepeatDef, RngStream, Seconds, SimRng, SimTime, Status,
    StreamKey, SubgraphRef, TickRate, TimedDef, Vec3, Version,
};
use serde_json::json;

/// The shared log of everything the stub leaves did.
pub type Log = Arc<Mutex<Vec<String>>>;

pub fn rate() -> TickRate {
    rate_hz(30)
}

pub fn rate_hz(hz: u32) -> TickRate {
    TickRate::new(hz).expect("non-zero rate")
}

pub fn cid(s: &str) -> ComponentId {
    ComponentId::new(s).expect("valid component id")
}

pub fn bid(s: &str) -> BehaviorId {
    BehaviorId::new(s).expect("valid behavior id")
}

pub fn eid(raw: u64) -> EntityId {
    EntityId::new(raw).expect("non-zero entity id")
}

/// Values the tests change between ticks: condition flags and blackboard slots.
#[derive(Default)]
pub struct Controls {
    flags: Mutex<BTreeMap<String, bool>>,
    slots: Mutex<BTreeMap<String, BbValue>>,
}

impl Controls {
    pub fn set_flag(&self, name: &str, value: bool) {
        self.flags
            .lock()
            .expect("flags lock")
            .insert(name.to_string(), value);
    }

    pub fn flag(&self, name: &str) -> bool {
        self.flags
            .lock()
            .expect("flags lock")
            .get(name)
            .copied()
            .unwrap_or(false)
    }

    /// Sets the slot; `None` empties it.
    pub fn set_slot(&self, name: &str, value: Option<BbValue>) {
        let mut slots = self.slots.lock().expect("slots lock");
        match value {
            Some(v) => {
                slots.insert(name.to_string(), v);
            }
            None => {
                slots.remove(name);
            }
        }
    }

    pub fn slot(&self, name: &str) -> Option<BbValue> {
        self.slots.lock().expect("slots lock").get(name).cloned()
    }
}

fn say(log: &Log, line: String) {
    log.lock().expect("log lock").push(line);
}

fn str_param<'a>(p: &'a ParamValue, key: &str) -> Result<&'a str, BuildError> {
    p.get(key)
        .and_then(ParamValue::as_str)
        .ok_or_else(|| BuildError::new(format!("`{key}` must be a string")).at(format!("/{key}")))
}

fn status_of(name: &str) -> Status {
    match name {
        "success" => Status::Success,
        "failure" => Status::Failure,
        _ => Status::Running,
    }
}

fn value_of(v: &ParamValue) -> BbValue {
    if let Some(b) = v.as_bool() {
        BbValue::Bool(b)
    } else if let Some(i) = v.as_i64() {
        BbValue::Integer(i)
    } else if let Some(f) = v.as_f64() {
        BbValue::Number(f)
    } else if let Some(s) = v.as_str() {
        BbValue::String(s.into())
    } else if let Some(n) = v.get("entity").and_then(ParamValue::as_u64) {
        BbValue::Entity(eid(n))
    } else {
        panic!("unsupported stub value {v}")
    }
}

// --- stub condition ------------------------------------------------------------------------------

struct FlagCondition {
    id: ComponentId,
    name: String,
    log: Log,
    controls: Arc<Controls>,
}

impl Condition for FlagCondition {
    fn id(&self) -> &ComponentId {
        &self.id
    }

    fn check(&self, _: &mut BehaviorContext<'_>, _: &EntityView<'_>, _: &mut Blackboard) -> bool {
        say(&self.log, format!("cond:{}", self.name));
        self.controls.flag(&self.name)
    }
}

/// Writes `value` to `key` on every check, then returns the flag (a condition with a side effect).
struct StoreCondition {
    id: ComponentId,
    key: String,
    value: BbValue,
    flag: String,
    log: Log,
    controls: Arc<Controls>,
}

impl Condition for StoreCondition {
    fn id(&self) -> &ComponentId {
        &self.id
    }

    fn check(&self, _: &mut BehaviorContext<'_>, _: &EntityView<'_>, bb: &mut Blackboard) -> bool {
        let result = bb.set(&self.key, self.value.clone());
        say(&self.log, format!("store:{}={}", self.key, result.is_ok()));
        say(&self.log, format!("cond:{}", self.flag));
        self.controls.flag(&self.flag)
    }
}

fn register_store(registry: &mut Registry, log: &Log, controls: &Arc<Controls>) {
    let (l, c) = (Arc::clone(log), Arc::clone(controls));
    registry
        .register_condition(
            Arc::new(StubFactory::<dyn Condition>::new(
                "stub.cond.store",
                Box::new(move |id, p| {
                    Ok(Arc::new(StoreCondition {
                        id: id.clone(),
                        key: str_param(p, "key")?.to_string(),
                        value: value_of(p.get("value").unwrap_or(&ParamValue::Null)),
                        flag: str_param(p, "flag")?.to_string(),
                        log: Arc::clone(&l),
                        controls: Arc::clone(&c),
                    }))
                }),
            )),
            info(Provenance::Plugin),
        )
        .expect("register store");
}

// --- stub actions --------------------------------------------------------------------------------

struct ScriptAction {
    id: ComponentId,
    name: String,
    script: Vec<Status>,
    log: Log,
}

struct ScriptInstance {
    kind: ComponentId,
    name: String,
    script: Vec<Status>,
    next: usize,
    log: Log,
}

impl Action for ScriptAction {
    fn id(&self) -> &ComponentId {
        &self.id
    }

    fn instantiate(&self, _: TickRate) -> Result<Box<dyn ActionInstance>, InstantiateError> {
        Ok(Box::new(ScriptInstance {
            kind: self.id.clone(),
            name: self.name.clone(),
            script: self.script.clone(),
            next: 0,
            log: Arc::clone(&self.log),
        }))
    }
}

impl ActionInstance for ScriptInstance {
    fn tick(
        &mut self,
        _: &mut BehaviorContext<'_>,
        _: &EntityView<'_>,
        _: &mut Blackboard,
        out: &mut IntentBuffer<'_>,
    ) -> Status {
        say(&self.log, format!("tick:{}", self.name));
        out.push(Intent::Custom {
            kind: self.kind.clone(),
            payload: ParamValue::String(self.name.clone()),
        });
        let status = self
            .script
            .get(self.next)
            .or(self.script.last())
            .copied()
            .unwrap_or(Status::Success);
        self.next += 1;
        status
    }

    fn halt(&mut self) {
        say(&self.log, format!("halt:{}", self.name));
        self.next = 0;
    }
}

#[derive(Clone)]
struct BbAction {
    id: ComponentId,
    op: String,
    key: String,
    label: String,
    value: Option<BbValue>,
    status: Status,
    log: Log,
}

struct BbInstance(BbAction);

impl Action for BbAction {
    fn id(&self) -> &ComponentId {
        &self.id
    }

    fn instantiate(&self, _: TickRate) -> Result<Box<dyn ActionInstance>, InstantiateError> {
        Ok(Box::new(BbInstance(self.clone())))
    }
}

impl ActionInstance for BbInstance {
    fn tick(
        &mut self,
        _: &mut BehaviorContext<'_>,
        _: &EntityView<'_>,
        bb: &mut Blackboard,
        _: &mut IntentBuffer<'_>,
    ) -> Status {
        let a = &self.0;
        match a.op.as_str() {
            "set" => {
                let result = bb.set(&a.key, a.value.clone().expect("`set` needs a value"));
                let shown = result.map_or_else(|e| format!("{e:?}"), |()| "ok".to_string());
                say(&a.log, format!("set:{}={shown}", a.label));
            }
            "unset" => {
                let result = bb.unset(&a.key);
                let shown = result.map_or_else(|e| format!("{e:?}"), |()| "ok".to_string());
                say(&a.log, format!("unset:{}={shown}", a.label));
            }
            _ => say(&a.log, format!("read:{}={:?}", a.label, bb.get(&a.key))),
        }
        a.status
    }

    fn halt(&mut self) {
        say(&self.0.log, format!("halt:{}", self.0.label));
    }
}

#[derive(Clone)]
struct PutAction {
    id: ComponentId,
    key: String,
    slot: String,
    log: Log,
    controls: Arc<Controls>,
}

struct PutInstance(PutAction);

impl Action for PutAction {
    fn id(&self) -> &ComponentId {
        &self.id
    }

    fn instantiate(&self, _: TickRate) -> Result<Box<dyn ActionInstance>, InstantiateError> {
        Ok(Box::new(PutInstance(self.clone())))
    }
}

impl ActionInstance for PutInstance {
    fn tick(
        &mut self,
        _: &mut BehaviorContext<'_>,
        _: &EntityView<'_>,
        bb: &mut Blackboard,
        _: &mut IntentBuffer<'_>,
    ) -> Status {
        let a = &self.0;
        match a.controls.slot(&a.slot) {
            Some(v) => {
                let _ = bb.set(&a.key, v);
            }
            None => {
                let _ = bb.unset(&a.key);
            }
        }
        say(&a.log, format!("put:{}", a.slot));
        Status::Success
    }

    fn halt(&mut self) {}
}

// --- factories -----------------------------------------------------------------------------------

type BuildFn<T> =
    Box<dyn Fn(&ComponentId, &ParamValue) -> Result<Arc<T>, BuildError> + Send + Sync>;

/// A factory with a permissive schema that builds through a closure.
pub struct StubFactory<T: ?Sized> {
    id: ComponentId,
    schema: ParamSchema,
    build: BuildFn<T>,
}

impl<T: ?Sized> StubFactory<T> {
    pub fn new(id: &str, build: BuildFn<T>) -> Self {
        Self {
            id: cid(id),
            schema: ParamSchema::from_json_str(r#"{"type":"object"}"#).expect("valid schema"),
            build,
        }
    }
}

impl<T: ?Sized + Send + Sync> Factory<T> for StubFactory<T> {
    fn id(&self) -> &ComponentId {
        &self.id
    }

    fn param_schema(&self) -> &ParamSchema {
        &self.schema
    }

    fn build(&self, params: &ParamValue) -> Result<Arc<T>, BuildError> {
        (self.build)(&self.id, params)
    }
}

pub fn info(source: Provenance) -> ComponentInfo {
    ComponentInfo::new("stub", source, Version::new(1, 0, 0))
}

/// A stub behavior that stands in for a plugin: it logs `tick:leaf`, emits the same `Custom`
/// intent as a `stub.action.script` named `leaf` and returns `running`, then `success` (like the
/// script `["running", "success"]`).
pub struct PluginBehavior {
    id: BehaviorId,
    log: Log,
}

struct PluginInstance {
    log: Log,
    next: usize,
}

impl Behavior for PluginBehavior {
    fn id(&self) -> &BehaviorId {
        &self.id
    }

    fn instantiate(&self, _: TickRate) -> Result<Box<dyn BehaviorInstance>, InstantiateError> {
        Ok(Box::new(PluginInstance {
            log: Arc::clone(&self.log),
            next: 0,
        }))
    }
}

impl BehaviorInstance for PluginInstance {
    fn tick(
        &mut self,
        _: &mut BehaviorContext<'_>,
        _: &EntityView<'_>,
        out: &mut IntentBuffer<'_>,
    ) -> Status {
        say(&self.log, "tick:leaf".to_string());
        out.push(Intent::Custom {
            kind: cid("stub.action.script"),
            payload: ParamValue::String("leaf".into()),
        });
        let status = if self.next == 0 {
            Status::Running
        } else {
            Status::Success
        };
        self.next += 1;
        status
    }

    fn halt(&mut self) {
        say(&self.log, "halt:leaf".to_string());
        self.next = 0;
    }

    fn output(&self, _: &str) -> Option<BbValue> {
        None
    }

    fn set_input(&mut self, param: &str, value: Option<BbValue>) -> Result<(), BlackboardError> {
        say(&self.log, format!("input:{param}={value:?}"));
        Ok(())
    }
}

// --- the bench -----------------------------------------------------------------------------------

/// A registry with the stub leaves, the shared log and the controls.
pub struct Bench {
    pub registry: Registry,
    pub log: Log,
    pub controls: Arc<Controls>,
}

impl Default for Bench {
    fn default() -> Self {
        Self::new()
    }
}

impl Bench {
    pub fn new() -> Self {
        let log: Log = Arc::default();
        let controls: Arc<Controls> = Arc::default();
        let mut registry = Registry::new();

        let (l, c) = (Arc::clone(&log), Arc::clone(&controls));
        registry
            .register_condition(
                Arc::new(StubFactory::<dyn Condition>::new(
                    "stub.cond.flag",
                    Box::new(move |id, p| {
                        Ok(Arc::new(FlagCondition {
                            id: id.clone(),
                            name: str_param(p, "name")?.to_string(),
                            log: Arc::clone(&l),
                            controls: Arc::clone(&c),
                        }))
                    }),
                )),
                info(Provenance::Plugin),
            )
            .expect("register flag");

        register_store(&mut registry, &log, &controls);

        let l = Arc::clone(&log);
        registry
            .register_action(
                Arc::new(StubFactory::<dyn Action>::new(
                    "stub.action.script",
                    Box::new(move |id, p| {
                        let script = p
                            .get("script")
                            .and_then(ParamValue::as_array)
                            .map(|a| a.iter().filter_map(|s| s.as_str()).map(status_of).collect())
                            .unwrap_or_default();
                        Ok(Arc::new(ScriptAction {
                            id: id.clone(),
                            name: str_param(p, "name")?.to_string(),
                            script,
                            log: Arc::clone(&l),
                        }))
                    }),
                )),
                info(Provenance::Plugin),
            )
            .expect("register script");

        let l = Arc::clone(&log);
        registry
            .register_action(
                Arc::new(StubFactory::<dyn Action>::new(
                    "stub.action.bb",
                    Box::new(move |id, p| {
                        let a: Arc<dyn Action> = Arc::new(BbAction {
                            id: id.clone(),
                            op: str_param(p, "op")?.to_string(),
                            key: str_param(p, "key")?.to_string(),
                            label: str_param(p, "label")?.to_string(),
                            value: p.get("value").map(value_of),
                            status: p
                                .get("status")
                                .and_then(ParamValue::as_str)
                                .map_or(Status::Success, status_of),
                            log: Arc::clone(&l),
                        });
                        Ok(a)
                    }),
                )),
                info(Provenance::Plugin),
            )
            .expect("register bb");

        let (l, c) = (Arc::clone(&log), Arc::clone(&controls));
        registry
            .register_action(
                Arc::new(StubFactory::<dyn Action>::new(
                    "stub.action.put",
                    Box::new(move |id, p| {
                        let a: Arc<dyn Action> = Arc::new(PutAction {
                            id: id.clone(),
                            key: str_param(p, "key")?.to_string(),
                            slot: str_param(p, "slot")?.to_string(),
                            log: Arc::clone(&l),
                            controls: Arc::clone(&c),
                        });
                        Ok(a)
                    }),
                )),
                info(Provenance::Plugin),
            )
            .expect("register put");

        Self {
            registry,
            log,
            controls,
        }
    }

    /// Registers a stub behavior with `Provenance::Plugin` that behaves like the script
    /// `["running", "success"]` of an action named `leaf`.
    pub fn register_plugin_behavior(&mut self, id: &str) {
        self.register_plugin_behavior_with(id, info(Provenance::Plugin));
    }

    /// Like [`Self::register_plugin_behavior`] with the given metadata (for example declared
    /// `param_types`). Its `set_input` logs `input:<param>=<value>`.
    pub fn register_plugin_behavior_with(&mut self, id: &str, component_info: ComponentInfo) {
        let log = Arc::clone(&self.log);
        self.registry
            .register_behavior(
                Arc::new(StubFactory::<dyn Behavior>::new(
                    id,
                    Box::new(move |id, _| {
                        Ok(Arc::new(PluginBehavior {
                            id: bid(id.as_str()),
                            log: Arc::clone(&log),
                        }))
                    }),
                )),
                component_info,
            )
            .expect("register plugin behavior");
    }

    /// Registers graphs as behaviors (`register_graphs`: dependency order is its business).
    pub fn register(&mut self, graphs: Vec<GraphDef>) {
        let graphs: Vec<Arc<GraphDef>> = graphs.into_iter().map(Arc::new).collect();
        register_graphs(&mut self.registry, graphs.iter()).expect("register graphs");
    }

    /// The behavior for `def` built directly with a `GraphFactory` (no registration).
    pub fn behavior_of(&self, def: GraphDef, params: &ParamValue) -> Arc<dyn Behavior> {
        GraphFactory::new(Arc::new(def), &self.registry)
            .expect("graph factory")
            .build(params)
            .expect("build behavior")
    }

    /// The registered behavior `id`, built with `params`.
    pub fn registered(&self, id: &str, params: &ParamValue) -> Arc<dyn Behavior> {
        self.registry
            .behavior(&cid(id))
            .expect("behavior is registered")
            .build(params)
            .expect("build behavior")
    }

    /// A runner over a fresh instance of `def` (30 Hz, empty params).
    pub fn run(&self, def: GraphDef) -> Runner {
        self.runner(&self.behavior_of(def, &json!({})))
    }

    /// A runner over a fresh instance of the registered behavior `id` (30 Hz, `params`).
    pub fn run_registered(&self, id: &str, params: &ParamValue) -> Runner {
        self.runner(&self.registered(id, params))
    }

    pub fn runner(&self, behavior: &Arc<dyn Behavior>) -> Runner {
        self.runner_at(behavior, rate(), 1)
    }

    /// A runner whose instance uses entity id `entity` for its RNG stream.
    pub fn runner_at(&self, behavior: &Arc<dyn Behavior>, rate: TickRate, entity: u64) -> Runner {
        let instance = behavior.instantiate(rate).expect("instantiate");
        Runner::new(instance, rate, &self.log, entity)
    }
}

/// What one tick of an instance did.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub status: Status,
    /// Log lines written during the tick, in order.
    pub trace: Vec<String>,
    pub intents: Vec<Intent>,
}

/// Ticks one behavior instance with a fixed entity view, a hashed RNG stream and a tick counter.
pub struct Runner {
    pub instance: Box<dyn BehaviorInstance>,
    pub rng: RngStream,
    pub rate: TickRate,
    pub tick: u64,
    pub contacts: Vec<cgf_core::Contact>,
    pub state: KinematicState,
    log: Log,
}

pub fn state() -> KinematicState {
    KinematicState {
        position: Ecef(Vec3::new(6_378_137.0, 0.0, 0.0)),
        velocity: Vec3::ZERO,
        orientation: Quaternion::IDENTITY,
    }
}

impl Runner {
    pub fn new(
        instance: Box<dyn BehaviorInstance>,
        rate: TickRate,
        log: &Log,
        entity: u64,
    ) -> Self {
        Self {
            instance,
            rng: SimRng::new(42).stream(StreamKey::entity(eid(entity))),
            rate,
            tick: 0,
            contacts: Vec::new(),
            state: state(),
            log: Arc::clone(log),
        }
    }

    /// Ticks once at `now = tick`; returns what happened.
    pub fn step(&mut self) -> Step {
        self.log.lock().expect("log lock").clear();
        let mut intents = Vec::new();
        let status = {
            let now = SimTime::at_tick(self.rate, self.tick);
            let mut ctx = BehaviorContext::new(now, &mut self.rng);
            let view = EntityView::new(eid(1), &self.state).with_contacts(&self.contacts);
            let mut out = IntentBuffer::new(&mut intents);
            self.instance.tick(&mut ctx, &view, &mut out)
        };
        self.tick += 1;
        let trace = std::mem::take(&mut *self.log.lock().expect("log lock"));
        Step {
            status,
            trace,
            intents,
        }
    }

    /// Ticks `n` times.
    pub fn steps(&mut self, n: usize) -> Vec<Step> {
        (0..n).map(|_| self.step()).collect()
    }

    /// Halts the instance; returns the log lines written by the halt.
    pub fn halt(&mut self) -> Vec<String> {
        self.log.lock().expect("log lock").clear();
        self.instance.halt();
        std::mem::take(&mut *self.log.lock().expect("log lock"))
    }
}

/// The trace lines of a step list, one `Vec` per tick.
pub fn traces(steps: &[Step]) -> Vec<Vec<String>> {
    steps.iter().map(|s| s.trace.clone()).collect()
}

pub fn statuses(steps: &[Step]) -> Vec<Status> {
    steps.iter().map(|s| s.status).collect()
}

/// `["a", "b"]` as owned strings, for comparing traces.
pub fn lines(l: &[&str]) -> Vec<String> {
    l.iter().map(|s| (*s).to_string()).collect()
}

// --- graph builder -------------------------------------------------------------------------------

/// A permissive empty interface (no parameters).
pub fn no_interface() -> Interface {
    iface(&json!({}))
}

/// An interface from `{ name: { type, default?, ... } }` fragments (ADR 0014): JSON-typed
/// fragments go into the object schema (required unless they have a default); `entity` and
/// `position` fragments are binding-only and only appear in `types`.
pub fn iface(fragments: &ParamValue) -> Interface {
    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();
    let mut types = Vec::new();
    let mut defaults = Vec::new();
    for (name, fragment) in fragments.as_object().expect("fragment object") {
        let ty = fragment["type"].as_str().expect("fragment type");
        let bb = match ty {
            "boolean" => BbType::Bool,
            "number" => BbType::Number,
            "integer" => BbType::Integer,
            "string" => BbType::String,
            "entity" => BbType::Entity,
            "position" => BbType::Position,
            other => panic!("unknown fragment type {other}"),
        };
        types.push((name.clone(), bb));
        if let Some(d) = fragment.get("default") {
            defaults.push((name.clone(), d.clone()));
        }
        if matches!(ty, "entity" | "position") {
            continue;
        }
        if fragment.get("default").is_none() {
            required.push(name.clone());
        }
        properties.insert(name.clone(), fragment.clone());
    }
    let schema = json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false,
    });
    let mut i =
        Interface::new(ParamSchema::from_json_str(&schema.to_string()).expect("interface schema"));
    for (name, ty) in types {
        i = i.with_type(name, ty);
    }
    for (name, d) in defaults {
        i = i.with_default(name, d);
    }
    i
}

/// Builds a node arena; nodes are added children first, then the root last.
#[derive(Default)]
pub struct B {
    nodes: Vec<NodeDef>,
}

impl B {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn node(&mut self, name: &str, kind: NodeKind) -> NodeId {
        let id = NodeId(u32::try_from(self.nodes.len()).expect("few nodes"));
        self.nodes.push(NodeDef::new(name, kind));
        id
    }

    /// `stub.cond.flag` named `name`.
    pub fn cond(&mut self, name: &str) -> NodeId {
        self.leaf_cond(name, "stub.cond.flag", json!({ "name": name }))
    }

    pub fn leaf_cond(&mut self, node: &str, id: &str, params: ParamValue) -> NodeId {
        self.node(
            node,
            NodeKind::Condition(LeafRef::new(cid(id), ParamTemplate::new(params))),
        )
    }

    /// `stub.action.script` named `name` returning `script` (`"success"`, `"failure"`,
    /// `"running"`).
    pub fn act(&mut self, name: &str, script: &[&str]) -> NodeId {
        self.leaf_action(
            name,
            "stub.action.script",
            json!({ "name": name, "script": script }),
        )
    }

    pub fn leaf_action(&mut self, node: &str, id: &str, params: ParamValue) -> NodeId {
        self.node(
            node,
            NodeKind::Action(LeafRef::new(cid(id), ParamTemplate::new(params))),
        )
    }

    /// `stub.action.bb` that reads `key` and logs it as `read:<label>=...`.
    pub fn read(&mut self, label: &str, key: &str) -> NodeId {
        self.leaf_action(
            label,
            "stub.action.bb",
            json!({ "op": "read", "key": key, "label": label }),
        )
    }

    /// `stub.action.bb` that sets `key` to `value` and logs `set:<label>=ok|<error>`.
    pub fn set(&mut self, label: &str, key: &str, value: impl Into<ParamValue>) -> NodeId {
        let mut params = json!({ "op": "set", "key": key, "label": label });
        params["value"] = value.into();
        self.leaf_action(label, "stub.action.bb", params)
    }

    /// `stub.action.put` copying the control slot `slot` into `key` every tick.
    pub fn put(&mut self, label: &str, key: &str, slot: &str) -> NodeId {
        self.leaf_action(
            label,
            "stub.action.put",
            json!({ "key": key, "slot": slot }),
        )
    }

    pub fn seq(&mut self, name: &str, memory: bool, children: &[NodeId]) -> NodeId {
        self.node(
            name,
            NodeKind::Sequence(Composite::new(children.to_vec()).with_memory(memory)),
        )
    }

    pub fn sel(&mut self, name: &str, memory: bool, children: &[NodeId]) -> NodeId {
        self.node(
            name,
            NodeKind::Selector(Composite::new(children.to_vec()).with_memory(memory)),
        )
    }

    pub fn par(&mut self, name: &str, policy: ParallelPolicy, children: &[NodeId]) -> NodeId {
        self.node(
            name,
            NodeKind::Parallel(ParallelDef::new(children.to_vec()).with_policy(policy)),
        )
    }

    pub fn inverter(&mut self, name: &str, child: NodeId) -> NodeId {
        self.node(name, NodeKind::Inverter(Decorator::new(child)))
    }

    pub fn succeed_always(&mut self, name: &str, child: NodeId) -> NodeId {
        self.node(name, NodeKind::SucceedAlways(Decorator::new(child)))
    }

    pub fn repeat(&mut self, name: &str, child: NodeId, count: Option<u32>) -> NodeId {
        let mut def = RepeatDef::new(child);
        if let Some(n) = count {
            def = def.with_count(NonZeroU32::new(n).expect("count >= 1"));
        }
        self.node(name, NodeKind::Repeat(def))
    }

    pub fn cooldown(&mut self, name: &str, child: NodeId, seconds: f64) -> NodeId {
        self.node(
            name,
            NodeKind::Cooldown(TimedDef::new(child, Seconds(seconds))),
        )
    }

    pub fn timeout(&mut self, name: &str, child: NodeId, seconds: f64) -> NodeId {
        self.node(
            name,
            NodeKind::Timeout(TimedDef::new(child, Seconds(seconds))),
        )
    }

    pub fn subgraph(&mut self, name: &str, call: SubgraphRef) -> NodeId {
        self.node(name, NodeKind::Subgraph(call))
    }

    pub fn fsm(&mut self, name: &str, def: FsmDef) -> NodeId {
        self.node(name, NodeKind::Fsm(def))
    }

    /// The graph `id` 1.0.0 with `root` and no interface.
    pub fn graph(self, id: &str, root: NodeId) -> GraphDef {
        self.graph_with(id, no_interface(), root)
    }

    pub fn graph_with(self, id: &str, interface: Interface, root: NodeId) -> GraphDef {
        self.graph_as(id, interface, root, Provenance::File)
    }

    pub fn graph_as(
        self,
        id: &str,
        interface: Interface,
        root: NodeId,
        source: Provenance,
    ) -> GraphDef {
        GraphDef::new(
            bid(id),
            Version::new(1, 0, 0),
            source,
            interface,
            root,
            self.nodes,
        )
    }
}

/// A call to subgraph `id` with literal `params`.
pub fn call(id: &str, params: ParamValue) -> SubgraphRef {
    SubgraphRef::new(cid(id), ParamTemplate::new(params))
}

/// A graph with one `stub.action.script` leaf named `name`.
pub fn script_graph(id: &str, name: &str, script: &[&str]) -> GraphDef {
    let mut b = B::new();
    let root = b.act(name, script);
    b.graph(id, root)
}

pub fn output(ty: BbType, from: &str) -> OutputPort {
    OutputPort::new(ty, from)
}

/// The log line `stub.action.bb` writes for a read of `value`.
pub fn read_line(label: &str, value: Option<&BbValue>) -> String {
    format!("read:{label}={value:?}")
}
