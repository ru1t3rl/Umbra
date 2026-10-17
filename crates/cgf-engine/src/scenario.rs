//! STUB (WI-9): loading and instantiating scenarios (REQ-S-2, REQ-T-1, ASM-059, ASM-062).
//!
//! Signatures only. Note for WI-9: the plan changes `Engine::new`/`with_wall_clock` to take a terrain
//! provider and the run callback to return `ControlFlow`; the step-4 tests reach the engine only
//! through `tests/common/mod.rs` so that change touches one helper.
#![expect(clippy::todo, reason = "stub until WI-9: signatures only")]

use std::path::Path;

use cgf_core::{EntityId, Registry, TemplateId};
use cgf_kernel::{ClockMode, WallClock};
use cgf_templates::LoaderError;
pub use cgf_templates::{
    Diagnostic, DiagnosticCode, Diagnostics, Location, Note, Scenario, ValidationReport,
};

use crate::engine::Engine;
use crate::error::EngineError;

/// Overrides and the wall clock for one run.
pub struct RunOptions {
    /// The wall clock pacing the engine.
    pub wall: Box<dyn WallClock>,
    /// Replaces the scenario's seed.
    pub seed: Option<u64>,
    /// Replaces the scenario's clock.
    pub clock: Option<ClockMode>,
}

/// Engine-level metadata of a spawned entity (not hashed, ASM-059).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityInfo {
    /// The entity.
    pub id: EntityId,
    /// Its template.
    pub template: TemplateId,
    /// Its name.
    pub name: Option<String>,
    /// Its group.
    pub group: Option<String>,
}

/// An engine with a scenario spawned in it.
pub struct ScenarioRun {
    /// The engine.
    pub engine: Engine,
    /// Spawned entities in id order.
    pub entities: Vec<EntityInfo>,
    /// The scenario's run length in ticks.
    pub duration_ticks: u64,
}

/// Why a scenario could not be loaded.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ScenarioError {
    /// The files have errors.
    #[error(transparent)]
    Invalid(#[from] Diagnostics),
    /// A registered component schema is broken.
    #[error(transparent)]
    Loader(#[from] LoaderError),
}

/// Loads a scenario file and its templates.
///
/// # Errors
/// [`ScenarioError`].
pub fn load_scenario(_registry: &Registry, _path: &Path) -> Result<Scenario, ScenarioError> {
    todo!("WI-9")
}

/// Validates a scenario file, a template file or a directory without running anything.
///
/// # Errors
/// [`ScenarioError::Loader`] if a registered schema is broken.
pub fn validate_path(
    _registry: &Registry,
    _path: &Path,
) -> Result<ValidationReport, ScenarioError> {
    todo!("WI-9")
}

/// Builds terrain and motion models through the registry and spawns in file order.
///
/// # Errors
/// [`EngineError`].
pub fn instantiate(
    _registry: &Registry,
    _scenario: &Scenario,
    _options: RunOptions,
) -> Result<ScenarioRun, EngineError> {
    todo!("WI-9")
}
