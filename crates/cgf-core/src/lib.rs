//! `cgf-core`: shared domain types for the CGF Engine.
//!
//! Responsibility: IDs (REQ-C-2), simulation time (REQ-C-1) and SI units (REQ-C-4).
//! These arrive in later M0 work items; this crate is currently an empty skeleton.
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

#[cfg(test)]
mod tests {
    #[test]
    fn smoke() {
        assert_eq!(2 + 2, 4);
    }
}
