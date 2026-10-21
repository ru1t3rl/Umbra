//! `umbra-core`: shared domain types for Umbra.
//!
//! Responsibility, one module each:
//! - [`units`]: SI `f64` newtypes (`Meters`, `Seconds`, `Radians`, ...) with dimension-checked
//!   arithmetic (REQ-C-4).
//! - [`time`]: [`SimTime`] (tick count at a [`TickRate`]) and [`TimeError`] (REQ-C-1).
//! - [`id`]: [`EntityId`] and the namespaced string IDs [`TemplateId`], [`BehaviorId`],
//!   [`MotionModelId`], [`PluginId`] (REQ-C-2, ADR 0002).
//! - [`math`]: the single site for transcendental float functions (ADR 0003).
//! - [`geo`]: [`Vec3`], [`Quaternion`], [`Euler`] (REQ-C-3).
//! - [`coords`]: WGS-84, [`Ecef`], [`Geodetic`], [`Enu`]/[`Ned`], [`LocalFrame`], [`Position`]
//!   (REQ-C-3, D-15).
//! - [`dead_reckoning`]: [`dead_reckoning::DeadReckoning`], [`dead_reckoning::RemoteState`] and
//!   [`dead_reckoning::extrapolate`], shared by publisher and receiver (REQ-N-6, ADR 0032).
//! - [`hash`]: the canonical [`StateHasher`] (ADR 0007).
//! - [`rng`]: [`SimRng`] and derived streams (ADR 0006).
//! - [`params`], [`registry`]: [`ParamValue`], [`ParamSchema`] and the single component
//!   [`Registry`] of [`Factory`] objects (REQ-T-1, REQ-T-3, D-8, ADR 0008).
//! - [`behavior`], [`perception`]: the [`Behavior`], [`Condition`], [`Action`] and [`Sensor`]
//!   traits, the typed [`Blackboard`], [`Force`] and [`Contact`] (REQ-B-1, REQ-B-5, REQ-B-11,
//!   ADR 0013-0015).
//! - [`graph`], [`catalog`]: the format-free [`GraphDef`] behavior graph, the [`NODE_TYPES`] table,
//!   [`Provenance`], [`ComponentInfo`] and the [`Catalog`] types (REQ-B-6, REQ-B-7, REQ-B-13,
//!   REQ-B-17, ADR 0011, ADR 0012).
//! - [`intent`], [`ownership`], [`terrain`], [`motion`]: [`Intent`], [`Ownership`],
//!   [`TerrainProvider`], [`KinematicState`] and the [`MotionModel`] trait (REQ-B-2, REQ-K-5,
//!   REQ-C-6, REQ-M-1).
//! - [`net`]: the protocol-neutral network model and the adapter traits (REQ-N-1, D-9, ADR 0030).
//! - [`plugin`]: [`PluginFaultPolicy`] (REQ-P-7, ADR 0021).
//! - [`event`]: the canonical [`SimEvent`] and [`ControlAction`] (REQ-C-5, ADR 0028).
//!
//! Dependency rule (REQ-A-1): this crate depends on no other workspace crate.
//! It contains no `unsafe` code and never reads the wall clock (REQ-K-4).

#![forbid(unsafe_code)]

pub mod behavior;
pub mod catalog;
pub mod coords;
pub mod dead_reckoning;
pub mod event;
pub mod geo;
pub mod graph;
pub mod hash;
pub mod id;
pub mod intent;
pub mod math;
pub mod motion;
pub mod net;
pub mod ownership;
pub mod params;
pub mod perception;
pub mod plugin;
pub mod registry;
pub mod rng;
pub mod terrain;
pub mod time;
pub mod units;

pub use behavior::{
    Action, ActionInstance, BatchItem, BbType, BbValue, Behavior, BehaviorBatch, BehaviorContext,
    BehaviorInstance, Blackboard, BlackboardError, Condition, EntityView, InstantiateError,
    IntentBuffer, RouteView, Status, tick_each,
};
pub use catalog::{
    Access, BlackboardParam, Catalog, CatalogEntry, CatalogError, CatalogKind, ComponentInfo,
    Provenance,
};
pub use coords::{CoordError, Ecef, Enu, Geodetic, LocalFrame, Ned, Position};
pub use event::{ControlAction, SimEvent};
pub use geo::{Euler, Quaternion, Vec3, WorldEuler};
pub use graph::{
    Composite, DEFAULT_MAX_SUBGRAPH_DEPTH, Decorator, ExitDef, FsmDef, GraphDef, Interface,
    LeafRef, MAX_NODE_DEPTH, NODE_TYPES, NodeDef, NodeId, NodeKind, NodeKindTag, NodeTypeInfo,
    OutputPort, ParallelDef, ParallelPolicy, ParamTemplate, RepeatDef, ReservedForm, StateDef,
    SubgraphRef, SubstituteError, TimedDef, TransitionDef, TransitionTarget, Trigger, Version,
    VersionReq, reserved_form,
};
pub use hash::{STATE_HASH_VERSION, StateHash, StateHasher};
pub use id::{
    BehaviorId, ComponentId, EntityId, IdError, MAX_ID_LEN, MotionModelId, NameError, PluginId,
    TemplateId, WeaponId, validate_name,
};
pub use intent::{INTENT_SET_VERSION, Intent};
pub use motion::{KinematicState, MotionModel};
pub use ownership::{Ownership, RemoteSource};
pub use params::{ParamSchema, ParamSchemaError, ParamValue};
pub use perception::{
    Contact, ContactBuffer, Force, ForceError, Observer, SenseContext, Sensor, Target, hostile,
};
pub use plugin::PluginFaultPolicy;
pub use registry::{BuildError, ComponentKind, Factory, Registry, RegistryError};
pub use rng::{RngStream, SimRng, SplitMix64, StreamKey};
pub use terrain::{FlatTerrain, TerrainProvider};
pub use time::{DEFAULT_TICK_RATE_HZ, SimTime, TickRate, TimeError};
pub use units::{
    Kilograms, Meters, MetersPerSecond, MetersPerSecondSquared, Radians, RadiansPerSecond, Seconds,
};
