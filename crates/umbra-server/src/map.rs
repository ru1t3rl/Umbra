//! The edge between domain types and wire types (D-12): only this crate knows both.
//!
//! The clock, run status, scenario, step and diagnostic mappings (WI-11), and the entity, template
//! and component mappings with the entity query (WI-13).

mod components;
mod entities;
mod query;
mod run;
mod status;
mod templates;

use umbra_api_types as api;
use umbra_engine::{ClockMode, TimeScale};

pub use components::{component_entry, component_kind_filter, component_kind_query};
pub use entities::{entity_detail, entity_page, entity_summary, spawn_spec};
pub use query::entity_query;
pub use run::step_result;
pub use status::{
    diagnostic, engine_status, plugin_fault, run_state_name, run_status, scenario_info,
};
pub use templates::{template_detail, template_summary};

/// A wire value the engine cannot take.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum MapError {
    /// `scale` is not finite or outside `[0.001, 1000000]`.
    #[error("clock scale must be finite and within [0.001, 1000000]")]
    InvalidScale,
    /// A spawn request names an id that is not valid (`template`, `force`).
    #[error("{0}")]
    InvalidId(String),
    /// A spawn request position the engine cannot place (a coordinate that is not finite or a
    /// latitude outside [-90, 90]).
    #[error("{0}")]
    InvalidSpawn(String),
}

impl MapError {
    /// The problem code this failure answers with (`422` for all of them).
    #[must_use]
    pub fn code(&self) -> api::ProblemCode {
        match self {
            Self::InvalidScale => api::ProblemCode::ValidationFailed,
            Self::InvalidId(_) => api::ProblemCode::InvalidId,
            Self::InvalidSpawn(_) => api::ProblemCode::InvalidSpawn,
        }
    }
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
