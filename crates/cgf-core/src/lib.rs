//! `cgf-core`: shared domain types for the CGF Engine.
//!
//! Responsibility, one module each:
//! - [`units`]: SI `f64` newtypes (`Meters`, `Seconds`, `Radians`, ...) with dimension-checked
//!   arithmetic (REQ-C-4).
//! - [`time`]: [`SimTime`] (tick count at a [`TickRate`]) and [`TimeError`] (REQ-C-1).
//! - [`id`]: [`EntityId`] and the namespaced string IDs [`TemplateId`], [`BehaviorId`],
//!   [`MotionModelId`], [`PluginId`] (REQ-C-2, ADR 0002).
//!
//! Dependency rule (REQ-A-1): this crate depends on no other workspace crate.
//! It contains no `unsafe` code and never reads the wall clock (REQ-K-4).

#![forbid(unsafe_code)]

pub mod coords;
pub mod geo;
pub mod hash;
pub mod id;
pub mod intent;
pub mod math;
pub mod motion;
pub mod ownership;
pub mod rng;
pub mod terrain;
pub mod time;
pub mod units;

pub use coords::{CoordError, Ecef, Enu, Geodetic, LocalFrame, Ned, Position};
pub use geo::{Euler, Quaternion, Vec3};
pub use hash::{STATE_HASH_VERSION, StateHash, StateHasher, fnv1a64};
pub use id::{BehaviorId, EntityId, IdError, MAX_ID_LEN, MotionModelId, PluginId, TemplateId};
pub use intent::Intent;
pub use motion::{KinematicState, MotionModel};
pub use ownership::{Ownership, RemoteSource};
pub use rng::{RngStream, SimRng, SplitMix64, StreamKey};
pub use terrain::{FlatTerrain, TerrainProvider};
pub use time::{SimTime, TickRate, TimeError};
pub use units::{
    Kilograms, Meters, MetersPerSecond, MetersPerSecondSquared, Radians, RadiansPerSecond, Seconds,
};
