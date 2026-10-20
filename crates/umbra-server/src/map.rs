//! The edge between domain types and wire types (D-12): only this crate knows both.
//!
//! The clock, run status, scenario, step and diagnostic mappings are real (WI-11). The entity
//! mappings are signatures until the work item that serves them (`todo!` names it).

#![expect(
    clippy::todo,
    unused_variables,
    reason = "WI-6 signature stubs; the bodies arrive in WI-13"
)]

mod run;
mod status;

use umbra_api_types as api;
use umbra_engine::handle::EntityState;
use umbra_engine::{ClockMode, TimeScale};

pub use run::step_result;
pub use status::{
    diagnostic, engine_status, plugin_fault, run_state_name, run_status, scenario_info,
};

/// A wire value the engine cannot take.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum MapError {
    /// `scale` is not finite or outside `[0.001, 1000000]`.
    #[error("clock scale must be finite and within [0.001, 1000000]")]
    InvalidScale,
}

/// The engine clock for a wire clock setting.
///
/// # Errors
/// [`MapError::InvalidScale`] for a scale the kernel refuses (REST answers `422 validation_failed`).
pub fn clock_mode(setting: &api::ClockSetting) -> Result<ClockMode, MapError> {
    Ok(match *setting {
        api::ClockSetting::Afap => ClockMode::AsFastAsPossible,
        api::ClockSetting::Realtime => ClockMode::RealTime,
        api::ClockSetting::Stepped => ClockMode::Stepped,
        api::ClockSetting::Scaled { scale } => {
            ClockMode::Scaled(TimeScale::new(scale).map_err(|_| MapError::InvalidScale)?)
        }
    })
}

/// The wire clock setting for an engine clock.
#[must_use]
pub fn clock_setting(mode: ClockMode) -> api::ClockSetting {
    match mode {
        ClockMode::AsFastAsPossible => api::ClockSetting::Afap,
        ClockMode::RealTime => api::ClockSetting::Realtime,
        ClockMode::Stepped => api::ClockSetting::Stepped,
        ClockMode::Scaled(scale) => api::ClockSetting::Scaled { scale: scale.get() },
    }
}

/// The wire summary of an entity (WI-13); the stream uses the same mapping (WI-15).
#[must_use]
pub fn entity_summary(entity: &EntityState) -> api::EntitySummary {
    todo!("WI-13")
}

/// The wire detail of an entity (WI-13).
#[must_use]
pub fn entity_detail(entity: &EntityState) -> api::EntityDetail {
    todo!("WI-13")
}
