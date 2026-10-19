//! Plugin log routing (REQ-P-5, ADR 0021): the `log` host callback hands each message to the
//! [`PluginLog`] of the host that made the current call, attributed to the plugin. The default sink
//! forwards to `tracing` (target `umbra_plugin`, field `plugin`). `PluginLog` is a leaf in the lock
//! order (ADR 0019): it must never call into the host or a plugin.

use umbra_core::PluginId;
use umbra_plugin_api::{
    UMBRA_LOG_DEBUG, UMBRA_LOG_ERROR, UMBRA_LOG_INFO, UMBRA_LOG_TRACE, UMBRA_LOG_WARN,
    UmbraLogLevel,
};

/// Log severity of a plugin message (`UMBRA_LOG_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum LogLevel {
    /// Error.
    Error,
    /// Warning.
    Warn,
    /// Info.
    Info,
    /// Debug.
    Debug,
    /// Trace.
    Trace,
}

impl LogLevel {
    /// The level of an ABI `UMBRA_LOG_*` value; `None` for an unknown value (never a cast, ADR
    /// 0019).
    pub(crate) fn from_abi(level: UmbraLogLevel) -> Option<Self> {
        Some(match level {
            UMBRA_LOG_ERROR => Self::Error,
            UMBRA_LOG_WARN => Self::Warn,
            UMBRA_LOG_INFO => Self::Info,
            UMBRA_LOG_DEBUG => Self::Debug,
            UMBRA_LOG_TRACE => Self::Trace,
            _ => return None,
        })
    }
}

/// Where plugin log messages go; the default forwards to `tracing`.
///
/// Implementations are leaves (ADR 0019 lock order, step 4): they may be called while a plugin call
/// is in progress on the calling thread, so they must never call into the host or a plugin. A panic
/// is caught at the boundary and reported to the plugin as an error code.
pub trait PluginLog: Send + Sync {
    /// Called with the plugin the message is attributed to.
    fn log(&self, plugin: &PluginId, level: LogLevel, message: &str);
}

/// The default [`PluginLog`]: `tracing` events with target `umbra_plugin` and a `plugin` field.
pub(crate) struct TracingLog;

impl PluginLog for TracingLog {
    fn log(&self, plugin: &PluginId, level: LogLevel, message: &str) {
        let plugin = plugin.as_str();
        match level {
            LogLevel::Error => tracing::error!(target: "umbra_plugin", plugin, "{message}"),
            LogLevel::Warn => tracing::warn!(target: "umbra_plugin", plugin, "{message}"),
            LogLevel::Info => tracing::info!(target: "umbra_plugin", plugin, "{message}"),
            LogLevel::Debug => tracing::debug!(target: "umbra_plugin", plugin, "{message}"),
            LogLevel::Trace => tracing::trace!(target: "umbra_plugin", plugin, "{message}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_abi_level_maps_and_unknown_levels_do_not() {
        assert_eq!(LogLevel::from_abi(UMBRA_LOG_ERROR), Some(LogLevel::Error));
        assert_eq!(LogLevel::from_abi(UMBRA_LOG_WARN), Some(LogLevel::Warn));
        assert_eq!(LogLevel::from_abi(UMBRA_LOG_INFO), Some(LogLevel::Info));
        assert_eq!(LogLevel::from_abi(UMBRA_LOG_DEBUG), Some(LogLevel::Debug));
        assert_eq!(LogLevel::from_abi(UMBRA_LOG_TRACE), Some(LogLevel::Trace));
        assert_eq!(LogLevel::from_abi(0), None);
        assert_eq!(LogLevel::from_abi(6), None);
    }

    #[test]
    fn tracing_log_accepts_every_level_without_a_subscriber() {
        let plugin = PluginId::new("test.plugin.log").unwrap_or_else(|e| panic!("{e}"));
        for level in [
            LogLevel::Error,
            LogLevel::Warn,
            LogLevel::Info,
            LogLevel::Debug,
            LogLevel::Trace,
        ] {
            TracingLog.log(&plugin, level, "message");
        }
    }
}
