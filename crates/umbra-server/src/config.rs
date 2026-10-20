//! What the server needs to start, assembled from the configuration file (ADR 0029).

use std::path::PathBuf;

use umbra_engine::{Config, ServerConfig, TickRate};

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
}
