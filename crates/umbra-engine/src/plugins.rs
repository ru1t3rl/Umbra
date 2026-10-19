//! Loading plugins into a scenario run (M4: REQ-P-7, REQ-P-10, REQ-S-1, REQ-S-2, ADR 0021, ADR 0022).
//!
//! Everything here is additive; `load_scenario`, `validate_path`, `instantiate` and
//! `standard_registry` are unchanged. A scenario's plugin directories are read leniently before the
//! full load ([`umbra_templates::scenario_plugin_paths`]), because the full load needs the registry
//! the plugins extend (ADR 0022). Each call creates a fresh [`PluginHost`] (ASM-118).

use std::path::{Path, PathBuf};
use std::sync::Arc;

pub use umbra_core::PluginFaultPolicy;
use umbra_core::Registry;
use umbra_plugin_host::HostMode;
pub use umbra_plugin_host::{
    CapabilityInfo, HostOptions, PluginError, PluginFault, PluginHost, PluginInfo, PluginLog,
};
use umbra_templates::{Scenario, ValidationReport, scenario_plugin_paths};

use crate::engine::Engine;
use crate::error::EngineError;
use crate::scenario::{
    RunOptions, ScenarioError, ScenarioRun, instantiate_in, load_scenario, validate_path,
};

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
    base: &Registry,
    paths: &[PathBuf],
    options: HostOptions,
) -> Result<(Registry, PluginHost), PluginError> {
    let mut host = PluginHost::new(options);
    for package in PluginHost::discover(paths)? {
        host.load(&package)?;
    }
    let mut registry = base.clone();
    host.register(&mut registry)?;
    Ok((registry, host))
}

/// The plugin directories of a run or a validation: `extra` (CLI) first, then those the scenario
/// file (or every scenario file under a directory) names, with duplicates dropped by canonical path,
/// first occurrence kept (ADR 0022, ASM-144). A directory that cannot be resolved is kept as written,
/// so discovery reports it.
#[must_use]
pub fn resolve_plugin_paths(extra: &[PathBuf], scenario: Option<&Path>) -> Vec<PathBuf> {
    let named = scenario.map(scenario_plugin_paths).unwrap_or_default();
    let mut seen = Vec::new();
    let mut out = Vec::new();
    for path in extra.iter().cloned().chain(named) {
        let identity = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        if !seen.contains(&identity) {
            seen.push(identity);
            out.push(path);
        }
    }
    out
}

/// `extra` (CLI) paths first, then the scenario's `plugin_paths`; a fresh host per call; sets the
/// scenario's fault policy on it.
///
/// # Errors
/// [`ScenarioError`], including [`ScenarioError::Plugin`].
pub fn load_scenario_with_plugins(
    base: &Registry,
    path: &Path,
    extra: &[PathBuf],
) -> Result<LoadedScenario, ScenarioError> {
    let paths = resolve_plugin_paths(extra, Some(path));
    let (registry, host) = load_plugins(base, &paths, HostOptions::default())?;
    // The loader dry-runs `configure` of plugin behaviors, which a run-mode host allows only inside
    // its build scope (ASM-138).
    let scenario = {
        let _scope = host.build_scope();
        load_scenario(&registry, path)?
    };
    host.set_fault_policy(scenario.on_plugin_fault);
    Ok(LoadedScenario {
        registry,
        scenario,
        plugins: Arc::new(host),
    })
}

/// Like `validate_path`, with a `HostMode::Validation` host (dry runs allowed, nothing ticks).
///
/// # Errors
/// [`ScenarioError`], including [`ScenarioError::Plugin`].
pub fn validate_path_with_plugins(
    base: &Registry,
    path: &Path,
    extra: &[PathBuf],
) -> Result<ValidationReport, ScenarioError> {
    let paths = resolve_plugin_paths(extra, Some(path));
    let mut options = HostOptions::default();
    options.mode = HostMode::Validation;
    let (registry, _host) = load_plugins(base, &paths, options)?;
    validate_path(&registry, path)
}

/// Like `instantiate`, for a scenario with plugins: creates the engine, attaches `loaded.plugins`,
/// then builds and spawns inside that engine's build scope (RR-m2). Plain `instantiate` with a
/// registry holding run-mode plugin factories fails with `EngineError::BehaviorBuild`.
///
/// # Errors
/// [`EngineError`]; [`EngineError::PluginHostInUse`] if the host is attached to another engine.
pub fn instantiate_with_plugins(
    loaded: &LoadedScenario,
    options: RunOptions,
) -> Result<ScenarioRun, EngineError> {
    instantiate_in(
        &loaded.registry,
        &loaded.scenario,
        options,
        Some(&loaded.plugins),
    )
}

impl Engine {
    /// Binds the host's fault state to this engine; after every tick of `run`/`step`, `take_abort()`
    /// stops the run with [`EngineError::PluginFault`]. A host attached to another engine is refused.
    ///
    /// # Errors
    /// [`EngineError::PluginHostInUse`].
    pub fn attach_plugins(&mut self, host: Arc<PluginHost>) -> Result<(), EngineError> {
        if !host.try_attach() {
            return Err(EngineError::PluginHostInUse);
        }
        self.plugins = Some(host);
        Ok(())
    }
}
