//! Behavior contracts: [`Behavior`], [`Condition`], [`Action`], the typed [`Blackboard`] and the
//! read-only views they get (REQ-B-1, REQ-B-5, REQ-B-11, ADR 0013, ADR 0014).
//!
//! Behaviors read state through [`EntityView`] and write only [`Intent`]s through the append-only
//! [`IntentBuffer`]; they never mutate entity state (REQ-B-1). Randomness and time come only from
//! [`BehaviorContext`] (REQ-K-4).

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use crate::coords::Ecef;
use crate::id::{BehaviorId, ComponentId, EntityId};
use crate::intent::Intent;
use crate::motion::KinematicState;
use crate::perception::{Contact, Force};
use crate::rng::RngStream;
use crate::time::{SimTime, TickRate};

/// Result of ticking a node (REQ-B-8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Finished successfully.
    Success,
    /// Finished unsuccessfully.
    Failure,
    /// Not finished; ticked again next tick.
    Running,
}

/// Why a behavior or action could not create an instance (for example a duration that does not
/// fit in ticks at this rate). Not expected after validation; callers propagate it.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum InstantiateError {
    /// A duration does not convert to whole ticks at the tick rate.
    #[error("{what}: duration does not convert to ticks at {hz} Hz")]
    Duration {
        /// What carries the duration (a node or parameter name).
        what: String,
        /// The tick rate in hertz.
        hz: u32,
    },
    /// A nested subgraph could not be instantiated.
    #[error("subgraph `{id}`: {source}")]
    Subgraph {
        /// The subgraph's ID.
        id: BehaviorId,
        /// Why it failed.
        #[source]
        source: Box<InstantiateError>,
    },
    /// Any other reason.
    #[error("{message}")]
    Other {
        /// Human-readable reason.
        message: String,
    },
}

/// Time and randomness for one behavior tick (REQ-K-4).
#[derive(Debug)]
#[non_exhaustive]
pub struct BehaviorContext<'a> {
    /// Simulation time of the current tick.
    pub now: SimTime,
    /// The entity's own hashed RNG stream (ADR 0006).
    pub rng: &'a mut RngStream,
}

impl<'a> BehaviorContext<'a> {
    /// Creates a context.
    pub fn new(now: SimTime, rng: &'a mut RngStream) -> Self {
        Self { now, rng }
    }
}

/// Read-only view of the entity being ticked.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct EntityView<'a> {
    /// The entity's ID.
    pub id: EntityId,
    /// Its pre-tick kinematic state.
    pub state: &'a KinematicState,
    /// Its force, if any.
    pub force: Option<&'a Force>,
    /// Its route, if it has one.
    pub route: Option<RouteView<'a>>,
    /// Contacts from this tick's Perception phase, sorted by contact entity ID.
    pub contacts: &'a [Contact],
}

impl<'a> EntityView<'a> {
    /// A view with no force, no route and no contacts.
    pub fn new(id: EntityId, state: &'a KinematicState) -> Self {
        Self {
            id,
            state,
            force: None,
            route: None,
            contacts: &[],
        }
    }

    /// Sets the force.
    #[must_use]
    pub fn with_force(mut self, force: Option<&'a Force>) -> Self {
        self.force = force;
        self
    }

    /// Sets the route.
    #[must_use]
    pub fn with_route(mut self, route: RouteView<'a>) -> Self {
        self.route = Some(route);
        self
    }

    /// Sets the contacts (sorted by entity ID).
    #[must_use]
    pub fn with_contacts(mut self, contacts: &'a [Contact]) -> Self {
        self.contacts = contacts;
        self
    }
}

/// Read-only view of an entity's route.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct RouteView<'a> {
    /// Waypoints in order, ECEF.
    pub waypoints: &'a [Ecef],
    /// Index of the next waypoint.
    pub next: usize,
    /// Whether the last waypoint has been reached.
    pub complete: bool,
}

impl<'a> RouteView<'a> {
    /// Creates a route view.
    pub fn new(waypoints: &'a [Ecef], next: usize, complete: bool) -> Self {
        Self {
            waypoints,
            next,
            complete,
        }
    }
}

/// Append-only view of this tick's intents (REQ-B-1): behaviors can add intents, never remove or
/// inspect earlier ones.
pub struct IntentBuffer<'a>(&'a mut Vec<Intent>);

impl fmt::Debug for IntentBuffer<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IntentBuffer")
            .field("len", &self.0.len())
            .finish()
    }
}

impl<'a> IntentBuffer<'a> {
    /// Wraps the tick's intent list.
    pub fn new(v: &'a mut Vec<Intent>) -> Self {
        Self(v)
    }

    /// Appends an intent.
    pub fn push(&mut self, i: Intent) {
        self.0.push(i);
    }

    /// Number of intents in the list, including earlier ones.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the list is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// A behavior program; parameters are bound when it is built. Shared by entities.
///
/// ```
/// use std::sync::Arc;
/// use umbra_core::{Behavior, BehaviorInstance};
/// fn _accepts(_: Arc<dyn Behavior>, _: Box<dyn BehaviorInstance>) {}
/// ```
pub trait Behavior: Send + Sync {
    /// The behavior's ID.
    fn id(&self) -> &BehaviorId;

    /// Per-entity (or per-subgraph-node) state, durations converted to ticks at `rate`.
    ///
    /// # Errors
    /// [`InstantiateError`] if the instance cannot be created at `rate` (not expected after
    /// validation).
    fn instantiate(&self, rate: TickRate) -> Result<Box<dyn BehaviorInstance>, InstantiateError>;
}

/// Per-entity state of a [`Behavior`].
pub trait BehaviorInstance: Send {
    /// Ticks the behavior once, appending intents to `out`.
    fn tick(
        &mut self,
        ctx: &mut BehaviorContext<'_>,
        entity: &EntityView<'_>,
        out: &mut IntentBuffer<'_>,
    ) -> Status;

    /// Abandons a Running activation; the next tick starts fresh (blackboard kept, ADR 0014).
    ///
    /// This is the instance halt: it must be idempotent and may be called when the instance is
    /// not Running (a `subgraph` node calls it on every instance halt of its caller, so timers
    /// inside a graph child reset).
    fn halt(&mut self);

    /// A parent halt (ADR 0013): halt what is Running and keep the state of finished nodes; the
    /// default is the full reset of [`BehaviorInstance::halt`]. A `subgraph` node calls this when
    /// a pre-empting ancestor halts it, so a cooling `cooldown` inside a graph keeps its timer.
    fn halt_running(&mut self) {
        self.halt();
    }

    /// Value of a declared output port after the last tick (REQ-B-10), owned so an FFI bridge can
    /// return it; `None` = unset or no such port.
    fn output(&self, port: &str) -> Option<BbValue>;

    /// Sets an interface param's run-time value before the next tick (ADR 0014); `None` = unset.
    /// Required (no default body): graph instances write the read-only blackboard key of the same
    /// name via [`Blackboard::input`].
    ///
    /// # Errors
    /// The blackboard's error: undeclared param, not an input key, wrong type or non-finite number.
    fn set_input(&mut self, param: &str, value: Option<BbValue>) -> Result<(), BlackboardError>;
}

/// A boolean check. Never emits intents.
///
/// ```
/// use std::sync::Arc;
/// use umbra_core::Condition;
/// fn _accepts(_: Arc<dyn Condition>) {}
/// ```
pub trait Condition: Send + Sync {
    /// The condition's ID.
    fn id(&self) -> &ComponentId;

    /// Success iff true. May write declared blackboard keys (for example `store_as`).
    fn check(
        &self,
        ctx: &mut BehaviorContext<'_>,
        entity: &EntityView<'_>,
        bb: &mut Blackboard,
    ) -> bool;
}

/// A leaf that may emit intents over several ticks.
///
/// ```
/// use std::sync::Arc;
/// use umbra_core::{Action, ActionInstance};
/// fn _accepts(_: Arc<dyn Action>, _: Box<dyn ActionInstance>) {}
/// ```
pub trait Action: Send + Sync {
    /// The action's ID.
    fn id(&self) -> &ComponentId;

    /// Per-activation state, durations converted to ticks at `rate`.
    ///
    /// # Errors
    /// [`InstantiateError`] if the instance cannot be created at `rate` (not expected after
    /// validation).
    fn instantiate(&self, rate: TickRate) -> Result<Box<dyn ActionInstance>, InstantiateError>;
}

/// Per-entity state of an [`Action`].
pub trait ActionInstance: Send {
    /// Ticks the action once, appending intents to `out`.
    fn tick(
        &mut self,
        ctx: &mut BehaviorContext<'_>,
        entity: &EntityView<'_>,
        bb: &mut Blackboard,
        out: &mut IntentBuffer<'_>,
    ) -> Status;

    /// Abandons a Running activation; the next tick starts fresh.
    fn halt(&mut self);
}

/// The type of a blackboard key (ASM-071).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub enum BbType {
    /// `bool`.
    Bool,
    /// Finite `f64`.
    Number,
    /// `i64`.
    Integer,
    /// A string.
    String,
    /// An entity ID.
    Entity,
    /// An ECEF position.
    Position,
}

impl BbType {
    /// The type named `name` (the text [`Display`](fmt::Display) prints: `bool`, `number`,
    /// `integer`, `string`, `entity`, `position`), or `None`.
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "bool" => Self::Bool,
            "number" => Self::Number,
            "integer" => Self::Integer,
            "string" => Self::String,
            "entity" => Self::Entity,
            "position" => Self::Position,
            _ => return None,
        })
    }
}

impl fmt::Display for BbType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Bool => "bool",
            Self::Number => "number",
            Self::Integer => "integer",
            Self::String => "string",
            Self::Entity => "entity",
            Self::Position => "position",
        })
    }
}

/// A blackboard value.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum BbValue {
    /// `bool`.
    Bool(bool),
    /// Finite `f64`.
    Number(f64),
    /// `i64`.
    Integer(i64),
    /// A string.
    String(Arc<str>),
    /// An entity ID.
    Entity(EntityId),
    /// An ECEF position.
    Position(Ecef),
}

impl BbValue {
    /// The value's type.
    pub fn ty(&self) -> BbType {
        match self {
            Self::Bool(_) => BbType::Bool,
            Self::Number(_) => BbType::Number,
            Self::Integer(_) => BbType::Integer,
            Self::String(_) => BbType::String,
            Self::Entity(_) => BbType::Entity,
            Self::Position(_) => BbType::Position,
        }
    }

    fn is_finite(&self) -> bool {
        match self {
            Self::Number(n) => n.is_finite(),
            Self::Position(p) => p.0.x.is_finite() && p.0.y.is_finite() && p.0.z.is_finite(),
            _ => true,
        }
    }
}

/// Why a blackboard write was refused.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum BlackboardError {
    /// The key is not declared.
    #[error("blackboard key `{key}` is not declared")]
    Undeclared {
        /// The key.
        key: String,
    },
    /// `set` or `unset` on a read-only interface param key.
    #[error("blackboard key `{key}` is read-only")]
    ReadOnly {
        /// The key.
        key: String,
    },
    /// `input` on a declared key that is not an input key.
    #[error("blackboard key `{key}` is not an input key")]
    NotInput {
        /// The key.
        key: String,
    },
    /// The value's type differs from the key's declared type.
    #[error("blackboard key `{key}` holds {expected}, not {found}")]
    WrongType {
        /// The key.
        key: String,
        /// The declared type.
        expected: BbType,
        /// The type of the refused value.
        found: BbType,
    },
    /// The value is NaN or infinite.
    #[error("blackboard key `{key}` needs a finite value")]
    NonFinite {
        /// The key.
        key: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
struct Slot {
    ty: BbType,
    read_only: bool,
    value: Option<BbValue>,
}

/// Typed, declared per-instance key-value store (ASM-071, ADR 0014).
///
/// Values start unset. Cloning gives an independent store (instances never share one, REQ-B-11).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Blackboard {
    slots: BTreeMap<Arc<str>, Slot>,
}

impl Blackboard {
    /// Declares a writable key of type `ty`, unset. Redeclaring a key replaces it.
    pub fn declare(&mut self, key: &str, ty: BbType) {
        self.declare_slot(key, ty, false);
    }

    /// Declares a read-only key for an interface param (ADR 0014); only the runtime writes it, via
    /// [`Blackboard::input`]. Redeclaring a key replaces it.
    pub fn declare_input(&mut self, key: &str, ty: BbType) {
        self.declare_slot(key, ty, true);
    }

    fn declare_slot(&mut self, key: &str, ty: BbType, read_only: bool) {
        self.slots.insert(
            Arc::from(key),
            Slot {
                ty,
                read_only,
                value: None,
            },
        );
    }

    /// The value of `key`; `None` if unset or undeclared.
    pub fn get(&self, key: &str) -> Option<&BbValue> {
        self.slots.get(key).and_then(|s| s.value.as_ref())
    }

    /// Writes a declared, writable key.
    ///
    /// # Errors
    /// [`BlackboardError`] if the key is undeclared or read-only, the type differs, or the value
    /// is not finite. The old value is kept.
    pub fn set(&mut self, key: &str, v: BbValue) -> Result<(), BlackboardError> {
        let slot = self.slot_mut(key)?;
        if slot.read_only {
            return Err(BlackboardError::ReadOnly {
                key: key.to_owned(),
            });
        }
        Self::store(slot, key, Some(v))
    }

    /// Clears a declared, writable key.
    ///
    /// # Errors
    /// [`BlackboardError::Undeclared`] or [`BlackboardError::ReadOnly`].
    pub fn unset(&mut self, key: &str) -> Result<(), BlackboardError> {
        let slot = self.slot_mut(key)?;
        if slot.read_only {
            return Err(BlackboardError::ReadOnly {
                key: key.to_owned(),
            });
        }
        slot.value = None;
        Ok(())
    }

    /// Runtime-only write of an input key (copy-in before a subgraph tick); `None` = unset. The
    /// value must have exactly the key's type.
    ///
    /// # Errors
    /// [`BlackboardError::Undeclared`] if `key` is not declared, [`BlackboardError::NotInput`] if
    /// it was not declared with [`Blackboard::declare_input`]; wrong type or non-finite values
    /// are refused.
    pub fn input(&mut self, key: &str, v: Option<BbValue>) -> Result<(), BlackboardError> {
        let slot = self.slot_mut(key)?;
        if !slot.read_only {
            return Err(BlackboardError::NotInput {
                key: key.to_owned(),
            });
        }
        Self::store(slot, key, v)
    }

    fn slot_mut(&mut self, key: &str) -> Result<&mut Slot, BlackboardError> {
        self.slots
            .get_mut(key)
            .ok_or_else(|| BlackboardError::Undeclared {
                key: key.to_owned(),
            })
    }

    fn store(slot: &mut Slot, key: &str, v: Option<BbValue>) -> Result<(), BlackboardError> {
        if let Some(value) = &v {
            if value.ty() != slot.ty {
                return Err(BlackboardError::WrongType {
                    key: key.to_owned(),
                    expected: slot.ty,
                    found: value.ty(),
                });
            }
            if !value.is_finite() {
                return Err(BlackboardError::NonFinite {
                    key: key.to_owned(),
                });
            }
        }
        slot.value = v;
        Ok(())
    }
}
