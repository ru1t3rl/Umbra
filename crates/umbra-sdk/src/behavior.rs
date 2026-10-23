//! The safe behavior traits and value types (REQ-SDK-1, ADR 0020).
//!
//! A plugin implements [`BehaviorFactory`] (params to a configured behavior), [`Behavior`] (one
//! instance per entity) and [`BehaviorInstance`] (tick, halt, ports). The views and outputs a tick sees
//! ([`TickContext`], [`EntityView`], [`Intents`]) are built by the SDK from the host's batch and are
//! valid only during that tick (REQ-P-11). Randomness comes only from [`TickContext::rng`], which draws
//! the entity's own stream through the host API (REQ-K-4); time is [`TickContext::tick`] and
//! [`TickContext::rate_hz`]. A plugin must not use its own RNG or the wall clock.

use std::cell::RefCell;
use std::fmt;

use crate::abi::{UMBRA_ERR_STATE, UMBRA_OK, UmbraCode, UmbraToken, UmbraVec3};
use crate::convert::{IntentSink, ViewIn};
use crate::error::Error;
use crate::host::HostCopy;

/// Result of one behavior tick (ADR 0013); the host maps it to `UMBRA_STATUS_*`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    /// Finished successfully.
    Success,
    /// Finished unsuccessfully.
    Failure,
    /// Still running.
    Running,
}

/// A value crossing a port or binding (`BbValue` in core).
#[non_exhaustive]
#[derive(Clone, PartialEq, Debug)]
pub enum Value {
    /// Boolean.
    Bool(bool),
    /// Finite number.
    Number(f64),
    /// Integer.
    Integer(i64),
    /// UTF-8 text.
    String(String),
    /// Entity id (non-zero).
    Entity(u64),
    /// ECEF position in meters.
    Position([f64; 3]),
}

/// The type of a [`Value`], used in the capability metadata.
#[non_exhaustive]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ValueType {
    /// Boolean.
    Bool,
    /// Number.
    Number,
    /// Integer.
    Integer,
    /// Text.
    String,
    /// Entity id.
    Entity,
    /// Position.
    Position,
}

/// Log levels of [`log`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Level {
    /// Error.
    Error,
    /// Warning.
    Warn,
    /// Info.
    Info,
    /// Debug.
    Debug,
    /// Trace.
    Trace,
}

/// Declares the ports of a behavior: bindable params, outputs and the result text (metadata JSON).
#[derive(Default, Debug, Clone)]
pub struct Interface {
    params: Vec<(String, ValueType)>,
    outputs: Vec<(String, ValueType)>,
    result: Option<String>,
}

impl Interface {
    /// Declares a bindable param `name` of type `ty`.
    #[must_use]
    pub fn param(mut self, name: &str, ty: ValueType) -> Self {
        self.params.push((name.to_owned(), ty));
        self
    }

    /// Declares an output port.
    #[must_use]
    pub fn output(mut self, name: &str, ty: ValueType) -> Self {
        self.outputs.push((name.to_owned(), ty));
        self
    }

    /// Declares the result text.
    #[must_use]
    pub fn result(mut self, text: &str) -> Self {
        self.result = Some(text.to_owned());
        self
    }

    /// Whether any bindable param is declared (the vtable then carries `set_input`, ADR 0020).
    pub(crate) fn has_params(&self) -> bool {
        !self.params.is_empty()
    }

    /// Whether any output is declared (the vtable then carries `output`, ADR 0020).
    pub(crate) fn has_outputs(&self) -> bool {
        !self.outputs.is_empty()
    }

    /// The capability metadata JSON of ADR 0020 (`param_types`, `outputs`, `result`, each only when
    /// declared); empty text when nothing is declared. A name declared twice keeps its last type.
    pub(crate) fn metadata(&self) -> String {
        let ports = |list: &[(String, ValueType)]| {
            list.iter()
                .map(|(name, ty)| (name.clone(), serde_json::Value::from(ty.name())))
                .collect::<serde_json::Map<_, _>>()
        };
        let mut meta = serde_json::Map::new();
        if self.has_params() {
            meta.insert("param_types".into(), ports(&self.params).into());
        }
        if self.has_outputs() {
            meta.insert("outputs".into(), ports(&self.outputs).into());
        }
        if let Some(text) = &self.result {
            meta.insert("result".into(), text.clone().into());
        }
        if meta.is_empty() {
            String::new()
        } else {
            serde_json::Value::Object(meta).to_string()
        }
    }
}

impl ValueType {
    /// The type's name in the capability metadata (`BbType` names, ADR 0014).
    fn name(self) -> &'static str {
        match self {
            Self::Bool => "bool",
            Self::Number => "number",
            Self::Integer => "integer",
            Self::String => "string",
            Self::Entity => "entity",
            Self::Position => "position",
        }
    }
}

/// The configure params as JSON text.
#[derive(Debug)]
pub struct Params<'a> {
    json: &'a str,
}

impl Params<'_> {
    /// The raw JSON text.
    #[must_use]
    pub fn json(&self) -> &str {
        self.json
    }

    /// The parsed JSON value.
    ///
    /// # Errors
    /// `Error::params` when the text is not JSON.
    pub fn value(&self) -> Result<serde_json::Value, Error> {
        serde_json::from_str(self.json)
            .map_err(|e| Error::params(format!("params are not JSON: {e}")))
    }
}

impl<'a> Params<'a> {
    /// Params over the JSON text of a `configure` call.
    pub(crate) fn new(json: &'a str) -> Self {
        Self { json }
    }
}

/// One contact of the pre-tick view.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Contact<'a> {
    /// The contact's entity id.
    pub entity: u64,
    /// ECEF position.
    pub position: [f64; 3],
    /// ECEF velocity.
    pub velocity: [f64; 3],
    /// Force, if any.
    pub force: Option<&'a str>,
    /// Range in meters.
    pub range_m: f64,
}

/// The entity's route as the plugin sees it: its ECEF waypoints, the index of the next one and whether
/// the last one was reached (`RouteView` in core).
#[derive(Clone, Copy)]
pub struct Route<'a> {
    waypoints: &'a [UmbraVec3],
    next: usize,
    complete: bool,
}

impl<'a> Route<'a> {
    /// A route over host-written waypoints, valid for the tick.
    pub(crate) fn new(waypoints: &'a [UmbraVec3], next: usize, complete: bool) -> Self {
        Self {
            waypoints,
            next,
            complete,
        }
    }

    /// Number of waypoints.
    #[must_use]
    pub fn len(&self) -> usize {
        self.waypoints.len()
    }

    /// Whether the route has no waypoints.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.waypoints.is_empty()
    }

    /// Waypoint `index` (ECEF, m), if it exists.
    #[must_use]
    pub fn waypoint(&self, index: usize) -> Option<[f64; 3]> {
        self.waypoints.get(index).map(|w| [w.x, w.y, w.z])
    }

    /// The waypoints in order (ECEF, m).
    pub fn waypoints(&self) -> impl Iterator<Item = [f64; 3]> + 'a {
        self.waypoints.iter().map(|w| [w.x, w.y, w.z])
    }

    /// Index of the next waypoint.
    #[must_use]
    pub fn next_index(&self) -> usize {
        self.next
    }

    /// Whether the last waypoint has been reached.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.complete
    }
}

impl fmt::Debug for Route<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Route")
            .field("waypoints", &self.waypoints().collect::<Vec<_>>())
            .field("next", &self.next)
            .field("complete", &self.complete)
            .finish()
    }
}

/// Read-only view of the entity being ticked.
#[derive(Debug, Clone, Copy)]
pub struct EntityView<'a> {
    /// Entity id (non-zero).
    pub id: u64,
    /// ECEF position.
    pub position: [f64; 3],
    /// ECEF velocity.
    pub velocity: [f64; 3],
    /// Orientation quaternion `[w, x, y, z]`.
    pub orientation: [f64; 4],
    /// Force, if any.
    pub force: Option<&'a str>,
    /// Route, if any.
    pub route: Option<Route<'a>>,
    /// Read through [`EntityView::contacts`].
    contacts: &'a [Contact<'a>],
}

impl<'a> EntityView<'a> {
    /// The contacts of the last Perception phase, sorted by id.
    #[must_use]
    pub fn contacts(&self) -> &[Contact<'a>] {
        self.contacts
    }

    /// The view of a validated host view, with its contacts already converted.
    pub(crate) fn new(view: &ViewIn<'a>, contacts: &'a [Contact<'a>]) -> Self {
        Self {
            id: view.id,
            position: view.position,
            velocity: view.velocity,
            orientation: view.orientation,
            force: view.force,
            route: view.route,
            contacts,
        }
    }
}

/// An intent a behavior can emit (ADR 0016 set v1).
#[non_exhaustive]
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Intent<'a> {
    /// Go to an ECEF position.
    MoveTo([f64; 3]),
    /// Follow the entity's route.
    FollowRoute,
    /// Cruise speed, m/s.
    SetSpeed(f64),
    /// True heading, rad.
    SetHeading(f64),
    /// Stop.
    Hold,
    /// Fire a weapon.
    Fire {
        /// Target entity id (non-zero).
        target: u64,
        /// Weapon id.
        weapon: &'a str,
    },
    /// Spawn from a template.
    Spawn {
        /// Template id.
        template: &'a str,
        /// ECEF position.
        position: [f64; 3],
        /// Heading, rad.
        heading: f64,
    },
    /// Remove the entity.
    Despawn,
    /// A component-defined intent.
    Custom {
        /// Component id.
        kind: &'a str,
        /// JSON payload.
        payload_json: &'a str,
    },
}

/// The first host refusal of one call (an RNG draw or intent of a tick, a message of a network
/// `poll`, a service call or pushed contact of an ABI 0.2 kind): the SDK ends the call with its code.
/// The call text names what was refused (for a blackboard access, its key).
#[derive(Debug, Default)]
pub(crate) struct Fault(RefCell<Option<(UmbraCode, String)>>);

impl Fault {
    /// Records `code` from `call` unless a fault is already recorded.
    pub(crate) fn record(&self, code: UmbraCode, call: &str) {
        let mut first = self.0.borrow_mut();
        if first.is_none() {
            *first = Some((code, call.to_owned()));
        }
    }

    /// The recorded fault, if any.
    pub(crate) fn get(&self) -> Option<(UmbraCode, String)> {
        self.0.borrow().clone()
    }
}

/// Append-only intent output of one batch item.
#[derive(Debug)]
pub struct Intents<'a> {
    sink: IntentSink,
    item: usize,
    fault: &'a Fault,
}

impl<'a> Intents<'a> {
    /// The intents of batch item `item`.
    pub(crate) fn new(sink: IntentSink, item: usize, fault: &'a Fault) -> Self {
        Self { sink, item, fault }
    }
}

impl Intents<'_> {
    /// Pushes an intent.
    ///
    /// # Errors
    /// The host refused it (the capability is then faulted, and this call of `tick_batch` returns the
    /// host's code whether or not the behavior looks at the error).
    pub fn push(&mut self, intent: Intent<'_>) -> Result<(), Error> {
        let code = self.sink.push(self.item, &intent);
        if code == UMBRA_OK {
            Ok(())
        } else {
            self.fault.record(code, "intent push");
            Err(Error::with_code(
                code,
                format!("the host refused an intent (code {code})"),
            ))
        }
    }
}

/// Time and randomness of one tick call.
#[derive(Debug)]
pub struct TickContext<'a> {
    tick: u64,
    rate_hz: u32,
    host: Option<HostCopy>,
    rng: UmbraToken,
    fault: &'a Fault,
}

impl<'a> TickContext<'a> {
    /// The context of one batch item.
    pub(crate) fn new(
        tick: u64,
        rate_hz: u32,
        host: Option<HostCopy>,
        rng: UmbraToken,
        fault: &'a Fault,
    ) -> Self {
        Self {
            tick,
            rate_hz,
            host,
            rng,
            fault,
        }
    }
}

impl TickContext<'_> {
    /// Simulation tick number.
    #[must_use]
    pub fn tick(&self) -> u64 {
        self.tick
    }

    /// Tick rate in Hz.
    #[must_use]
    pub fn rate_hz(&self) -> u32 {
        self.rate_hz
    }

    /// The entity's own RNG stream, through the host API.
    pub fn rng(&mut self) -> Rng<'_> {
        Rng::new(self.host, self.rng, self.fault)
    }
}

/// Draws through the host API with this item's token; a refused draw makes the call report a fault.
#[derive(Debug)]
pub struct Rng<'a> {
    host: Option<HostCopy>,
    token: UmbraToken,
    fault: &'a Fault,
}

impl<'a> Rng<'a> {
    /// The stream of `token` through `host`, reporting refusals to `fault`.
    pub(crate) fn new(host: Option<HostCopy>, token: UmbraToken, fault: &'a Fault) -> Self {
        Self { host, token, fault }
    }
}

impl Rng<'_> {
    /// Next `u64`. A refused draw returns 0 and makes this call (`tick_batch`, `sense`, `check`,
    /// `tick`) return the host's code.
    pub fn next_u64(&mut self) -> u64 {
        let drawn = self
            .host
            .ok_or(UMBRA_ERR_STATE)
            .and_then(|h| h.next_u64(self.token));
        drawn.unwrap_or_else(|code| {
            self.fault.record(code, "rng_next_u64");
            0
        })
    }

    /// Next `f64` in `[0, 1)`. A refused draw returns 0.0 and makes this call return the host's code.
    pub fn next_f64(&mut self) -> f64 {
        let drawn = self
            .host
            .ok_or(UMBRA_ERR_STATE)
            .and_then(|h| h.next_f64(self.token));
        drawn.unwrap_or_else(|code| {
            self.fault.record(code, "rng_next_f64");
            0.0
        })
    }
}

/// Logs through the host API stored at entry; a no-op before entry and after `shutdown`.
pub fn log(level: Level, message: &str) {
    crate::host::log(level, message);
}

/// Builds behaviors of one capability from JSON params.
pub trait BehaviorFactory: Send + Sync + 'static {
    /// Component id (grammar of ADR 0002; `builtin.` is refused by the host).
    const ID: &'static str;
    /// JSON Schema of the params (ADR 0009 subset); the top-level `description` is the catalog text.
    const PARAM_SCHEMA: &'static str;
    /// Capability version; `None` means the plugin version.
    const VERSION: Option<&'static str> = None;
    /// The configured behavior.
    type Behavior: Behavior;
    /// Params, outputs and result text for the metadata JSON.
    #[must_use]
    fn interface() -> Interface {
        Interface::default()
    }
    /// Builds a behavior from the params.
    ///
    /// # Errors
    /// Bad params.
    fn configure(params: &Params<'_>) -> Result<Self::Behavior, Error>;
}

/// A configured behavior; instantiated once per entity.
pub trait Behavior: Send + Sync + 'static {
    /// The per-entity instance type.
    type Instance: BehaviorInstance;
    /// Creates an instance at `rate_hz`.
    ///
    /// # Errors
    /// The behavior cannot run at this rate.
    fn instantiate(&self, rate_hz: u32) -> Result<Self::Instance, Error>;
}

/// One entity's behavior state.
pub trait BehaviorInstance: Send + 'static {
    /// One tick.
    fn tick(
        &mut self,
        ctx: &mut TickContext<'_>,
        entity: &EntityView<'_>,
        out: &mut Intents<'_>,
    ) -> Status;
    /// Instance halt: idempotent, may be called when not Running (ASM-096 c).
    fn halt(&mut self);
    /// Parent halt (ADR 0013 amendment); default: the full reset.
    fn halt_running(&mut self) {
        self.halt();
    }
    /// Binds a param; `None` unsets it.
    ///
    /// # Errors
    /// The param is not declared (the default).
    fn set_input(&mut self, _param: &str, _value: Option<Value>) -> Result<(), Error> {
        Err(Error::new("undeclared input"))
    }
    /// Reads an output port.
    #[must_use]
    fn output(&self, _port: &str) -> Option<Value> {
        None
    }
}
