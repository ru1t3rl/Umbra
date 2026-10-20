//! What the server needs to start, assembled from the configuration file (ADR 0029).

use std::path::PathBuf;

use umbra_engine::builtins::{StandardRegistryError, standard_registry};
use umbra_engine::handle::HandleConfig;
use umbra_engine::{Config, Confinement, ServerConfig, TickRate};

/// The settings of one server run: the `[server]` table plus the values of other tables it uses.
///
/// Precedence of flag over file over default is applied by the caller (the CLI) before the values
/// arrive here.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct ServerSettings {
    /// `[server]` and `[server.stream]`.
    pub server: ServerConfig,
    /// `[plugins] paths` (empty if the file names none).
    pub plugin_paths: Vec<PathBuf>,
    /// The tick rate for scenarios that omit it: `[simulation] tick_rate_hz`, else 60 Hz.
    pub default_tick_rate: TickRate,
}

impl ServerSettings {
    /// The settings a configuration file gives; keys it leaves out keep their defaults.
    #[must_use]
    pub fn from_config(config: &Config) -> Self {
        Self {
            server: config.server.clone(),
            plugin_paths: config.plugin_paths.clone().unwrap_or_default(),
            default_tick_rate: config.tick_rate.unwrap_or_default(),
        }
    }

    /// The confinement of served loads (D-b, ADR 0026): the scenario root plus the configured plugin
    /// directories, canonicalised.
    ///
    /// # Errors
    /// The `io` error of a root or plugin directory that cannot be canonicalised.
    pub fn confinement(&self) -> std::io::Result<Confinement> {
        Confinement::new(&self.server.scenario_root, &self.plugin_paths)
    }

    /// The engine handle configuration `umbra serve` runs with (ASM-180, ASM-197): the standard
    /// registry, loads confined to the scenario root, the configured plugin paths, default tick rate
    /// and command queue. The wall clock and the other knobs keep the handle defaults; a caller may
    /// change the public fields afterwards.
    ///
    /// # Errors
    /// [`SetupError`] if the confinement directories do not exist or the registry cannot be built.
    pub fn handle_config(&self) -> Result<HandleConfig, SetupError> {
        let mut config = HandleConfig::new(standard_registry()?);
        config.load.default_tick_rate = self.default_tick_rate;
        config
            .load
            .extra_plugin_paths
            .clone_from(&self.plugin_paths);
        config.load.confinement = Some(self.confinement()?);
        config.command_capacity = self.server.command_queue;
        Ok(config)
    }
}

/// Why the engine could not be configured from the settings.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SetupError {
    /// The scenario root or a plugin directory cannot be resolved.
    #[error("cannot resolve the scenario root or a plugin directory: {0}")]
    Directory(#[from] std::io::Error),
    /// The built-in registry could not be built (not expected).
    #[error(transparent)]
    Registry(#[from] StandardRegistryError),
}
