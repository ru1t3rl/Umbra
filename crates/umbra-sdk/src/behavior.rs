//! The safe behavior traits and value types (REQ-SDK-1, ADR 0020).
//!
//! M4 step 4 stub: signatures exactly as in the M4 plan, bodies `todo!()` where they carry logic.
//! WI-7 replaces the bodies.

use core::marker::PhantomData;

use crate::error::Error;

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
    #[expect(clippy::todo, reason = "M4 WI-7 stub")]
    pub fn value(&self) -> Result<serde_json::Value, Error> {
        todo!("M4 WI-7")
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

/// The entity's route as the plugin sees it (fields are defined by WI-7; the plan names only the type).
#[derive(Debug, Clone, Copy)]
pub struct Route<'a> {
    marker: PhantomData<&'a ()>,
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
}

impl<'a> EntityView<'a> {
    /// The contacts of the last Perception phase, sorted by id.
    #[must_use]
    #[expect(clippy::todo, reason = "M4 WI-7 stub")]
    pub fn contacts(&self) -> &[Contact<'a>] {
        todo!("M4 WI-7")
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

/// Append-only intent output of one batch item.
#[derive(Debug)]
pub struct Intents<'a> {
    marker: PhantomData<&'a mut ()>,
}

impl Intents<'_> {
    /// Pushes an intent.
    ///
    /// # Errors
    /// The host refused it (the capability is then faulted).
    #[expect(clippy::todo, reason = "M4 WI-7 stub")]
    pub fn push(&mut self, _intent: Intent<'_>) -> Result<(), Error> {
        todo!("M4 WI-7")
    }
}

/// Time and randomness of one tick call.
#[derive(Debug)]
pub struct TickContext<'a> {
    marker: PhantomData<&'a mut ()>,
}

impl TickContext<'_> {
    /// Simulation tick number.
    #[must_use]
    #[expect(clippy::todo, reason = "M4 WI-7 stub")]
    pub fn tick(&self) -> u64 {
        todo!("M4 WI-7")
    }

    /// Tick rate in Hz.
    #[must_use]
    #[expect(clippy::todo, reason = "M4 WI-7 stub")]
    pub fn rate_hz(&self) -> u32 {
        todo!("M4 WI-7")
    }

    /// The entity's own RNG stream, through the host API.
    #[expect(clippy::todo, reason = "M4 WI-7 stub")]
    pub fn rng(&mut self) -> Rng<'_> {
        todo!("M4 WI-7")
    }
}

/// Draws through the host API with this item's token; a refused draw makes the call report a fault.
#[derive(Debug)]
pub struct Rng<'a> {
    marker: PhantomData<&'a mut ()>,
}

impl Rng<'_> {
    /// Next `u64`.
    #[expect(clippy::todo, reason = "M4 WI-7 stub")]
    pub fn next_u64(&mut self) -> u64 {
        todo!("M4 WI-7")
    }

    /// Next `f64` in `[0, 1)`.
    #[expect(clippy::todo, reason = "M4 WI-7 stub")]
    pub fn next_f64(&mut self) -> f64 {
        todo!("M4 WI-7")
    }
}

/// Logs through the host API stored at entry; a no-op before entry.
#[expect(clippy::todo, reason = "M4 WI-7 stub")]
pub fn log(_level: Level, _message: &str) {
    todo!("M4 WI-7")
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
