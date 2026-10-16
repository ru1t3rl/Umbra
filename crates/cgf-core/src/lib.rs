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

pub mod id;
pub mod time;
pub mod units;

pub use id::{BehaviorId, EntityId, IdError, MAX_ID_LEN, MotionModelId, PluginId, TemplateId};
pub use time::{SimTime, TickRate, TimeError};
pub use units::{
    Kilograms, Meters, MetersPerSecond, MetersPerSecondSquared, Radians, RadiansPerSecond, Seconds,
};
