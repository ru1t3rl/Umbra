//! Loading plugins into a scenario run (M4: REQ-P-7, REQ-P-10, REQ-S-1, REQ-S-2, ADR 0021, ADR 0022).
//!
//! M4 step 4 stub: signatures exactly as in the M4 plan (`docs/plans/M4.md`, "umbra-engine"); the
//! bodies are `todo!()` until WI-12 replaces them. Everything here is additive; `load_scenario`,
//! `validate_path`, `instantiate` and `standard_registry` are unchanged.
#![expect(clippy::todo, reason = "M4 WI-12 stub")]

use std::path::{Path, PathBuf};
use std::sync::Arc;

pub use umbra_core::PluginFaultPolicy;
use umbra_core::Registry;
pub use umbra_plugin_host::{
    CapabilityInfo, HostOptions, PluginError, PluginFault, PluginHost, PluginInfo, PluginLog,
};
use umbra_templates::{Scenario, ValidationReport};

use crate::engine::Engine;
use crate::error::EngineError;
use crate::scenario::{RunOptions, ScenarioError, ScenarioRun};

/// A scenario with the plugins it needs loaded into one fresh host.
#[non_exhaustive]
pub struct LoadedScenario {
    /// The base registry plus the plugins' capabilities.
    pub registry: Registry,
    /// The scenario, loaded against that registry.
    pub scenario: Scenario,
    /// The host that owns this load's plugins and fault state.
    pub plugins: Arc<PluginHost>,
}

/// Loads the plugins under `paths` (ADR 0022 order) into a new host and returns `base` plus their
/// capabilities.
///
/// # Errors
/// [`PluginError`] from discovery, loading or registration.
pub fn load_plugins(
    _base: &Registry,
    _paths: &[PathBuf],
    _options: HostOptions,
) -> Result<(Registry, PluginHost), PluginError> {
    todo!("M4 WI-12")
}

/// `extra` (CLI) paths first, then the scenario's `plugin_paths`; a fresh host per call; sets the
/// scenario's fault policy on it.
///
/// # Errors
/// [`ScenarioError`], including [`ScenarioError::Plugin`].
pub fn load_scenario_with_plugins(
    _base: &Registry,
    _path: &Path,
    _extra: &[PathBuf],
) -> Result<LoadedScenario, ScenarioError> {
    todo!("M4 WI-12")
}

/// Like `validate_path`, with a `HostMode::Validation` host (dry runs allowed, nothing ticks).
///
/// # Errors
/// [`ScenarioError`], including [`ScenarioError::Plugin`].
pub fn validate_path_with_plugins(
    _base: &Registry,
    _path: &Path,
    _extra: &[PathBuf],
) -> Result<ValidationReport, ScenarioError> {
    todo!("M4 WI-12")
}

/// Like `instantiate`, for a scenario with plugins: creates the engine, attaches `loaded.plugins`,
/// then builds and spawns inside that engine's build scope (RR-m2). Plain `instantiate` with a
/// registry holding run-mode plugin factories fails with `EngineError::BehaviorBuild`.
///
/// # Errors
/// [`EngineError`]; [`EngineError::PluginHostInUse`] if the host is attached to another engine.
pub fn instantiate_with_plugins(
    _loaded: &LoadedScenario,
    _options: RunOptions,
) -> Result<ScenarioRun, EngineError> {
    todo!("M4 WI-12")
}

impl Engine {
    /// Binds the host's fault state to this engine; after every tick of `run`/`step`, `take_abort()`
    /// stops the run with [`EngineError::PluginFault`]. A host attached to another engine is refused.
    ///
    /// # Errors
    /// [`EngineError::PluginHostInUse`].
    pub fn attach_plugins(&mut self, _host: Arc<PluginHost>) -> Result<(), EngineError> {
        todo!("M4 WI-12")
    }
}
