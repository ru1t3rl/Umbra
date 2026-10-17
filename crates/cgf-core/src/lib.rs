//! `cgf-core`: shared domain types for the CGF Engine.
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
//! - [`hash`]: the canonical [`StateHasher`] (ADR 0007).
//! - [`rng`]: [`SimRng`] and derived streams (ADR 0006).
//! - [`params`], [`registry`]: [`ParamValue`], [`ParamSchema`] and the single component
//!   [`Registry`] of [`Factory`] objects (REQ-T-1, REQ-T-3, D-8, ADR 0008).
//! - [`behavior`], [`perception`]: the [`Behavior`], [`Condition`], [`Action`] and [`Sensor`]
//!   traits, the typed [`Blackboard`], [`Force`] and [`Contact`] (REQ-B-1, REQ-B-5, REQ-B-11,
//!   ADR 0013-0015).
//! - [`intent`], [`ownership`], [`terrain`], [`motion`]: [`Intent`], [`Ownership`],
//!   [`TerrainProvider`], [`KinematicState`] and the [`MotionModel`] trait (REQ-B-2, REQ-K-5,
//!   REQ-C-6, REQ-M-1).
//!
//! Dependency rule (REQ-A-1): this crate depends on no other workspace crate.
//! It contains no `unsafe` code and never reads the wall clock (REQ-K-4).

#![forbid(unsafe_code)]

pub mod behavior;
pub mod coords;
pub mod geo;
pub mod hash;
pub mod id;
pub mod intent;
pub mod math;
pub mod motion;
pub mod ownership;
pub mod params;
pub mod perception;
pub mod registry;
pub mod rng;
pub mod terrain;
pub mod time;
pub mod units;

pub use behavior::{
    Action, ActionInstance, BbType, BbValue, Behavior, BehaviorContext, BehaviorInstance,
    Blackboard, BlackboardError, Condition, EntityView, InstantiateError, IntentBuffer, RouteView,
    Status,
};
pub use coords::{CoordError, Ecef, Enu, Geodetic, LocalFrame, Ned, Position};
pub use geo::{Euler, Quaternion, Vec3};
pub use hash::{STATE_HASH_VERSION, StateHash, StateHasher};
pub use id::{
    BehaviorId, ComponentId, EntityId, IdError, MAX_ID_LEN, MotionModelId, PluginId, TemplateId,
    WeaponId,
};
pub use intent::{INTENT_SET_VERSION, Intent};
pub use motion::{KinematicState, MotionModel};
pub use ownership::{Ownership, RemoteSource};
pub use params::{ParamSchema, ParamSchemaError, ParamValue};
pub use perception::{
    Contact, ContactBuffer, Force, ForceError, Observer, SenseContext, Sensor, Target, hostile,
};
pub use registry::{BuildError, ComponentKind, Factory, Registry, RegistryError};
pub use rng::{RngStream, SimRng, SplitMix64, StreamKey};
pub use terrain::{FlatTerrain, TerrainProvider};
pub use time::{SimTime, TickRate, TimeError};
pub use units::{
    Kilograms, Meters, MetersPerSecond, MetersPerSecondSquared, Radians, RadiansPerSecond, Seconds,
};
