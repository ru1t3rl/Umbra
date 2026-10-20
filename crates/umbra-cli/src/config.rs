//! Finding and reading `umbra.toml` (ADR 0029, REQ-X-2, ASM-159, ASM-160).
//!
//! `--config FILE` (a missing file is an error), else `./umbra.toml` if it exists, else the
//! built-in defaults. Precedence per setting is flag > file > default; the command functions apply
//! it with the helpers here.

use std::io::{self, Write};
use std::path::{Path, PathBuf};

use anyhow::Result;
use tracing_subscriber::filter::LevelFilter;
use umbra_engine::scenario::Diagnostic;
use umbra_engine::{Config, ConfigError, LoadSettings, LogLevel, load_config};

/// The file `umbra` looks for in the working directory when `--config` is not given.
const IMPLICIT_FILE: &str = "./umbra.toml";

/// The configuration a command runs with.
pub struct Loaded {
    /// The settings; all defaults when no file was found.
    pub config: Config,
    /// The file used, if any.
    pub path: Option<PathBuf>,
    /// Whether the file was discovered (`./umbra.toml`) rather than named with `--config`.
    pub implicit: bool,
}

/// What reading the configuration gave.
pub enum Outcome {
    /// Usable settings.
    Loaded(Box<Loaded>),
    /// The file has errors; the caller prints them and exits with 1.
    Invalid(Vec<Diagnostic>),
}

/// Finds and reads the configuration. Writes the `note:` line to stderr when an implicitly
/// discovered file is used (m13), whatever the log level.
///
/// # Errors
/// A named file that cannot be read, or an unreadable `./umbra.toml`.
pub fn load(explicit: Option<&Path>) -> Result<Outcome> {
    let (path, implicit) = match explicit {
        Some(path) => (path.to_path_buf(), false),
        None if Path::new(IMPLICIT_FILE).is_file() => (PathBuf::from(IMPLICIT_FILE), true),
        None => {
            return Ok(Outcome::Loaded(Box::new(Loaded {
                config: Config::default(),
                path: None,
                implicit: false,
            })));
        }
    };
    match load_config(&path) {
        Ok(config) => {
            if implicit {
                // Nothing sensible remains to do if stderr is gone.
                let _ = writeln!(io::stderr(), "note: using configuration {IMPLICIT_FILE}");
            }
            Ok(Outcome::Loaded(Box::new(Loaded {
                config,
                path: Some(path),
                implicit,
            })))
        }
        Err(ConfigError::Invalid(diagnostics)) => Ok(Outcome::Invalid(diagnostics.into_vec())),
        Err(other) => Err(other.into()),
    }
}

/// The plugin directories: any `--plugin-path` replaces `[plugins] paths` (lists are replaced, not
/// merged, ADR 0029).
pub fn plugin_paths(flag: &[PathBuf], config: &Config) -> Vec<PathBuf> {
    if flag.is_empty() {
        config.plugin_paths.clone().unwrap_or_default()
    } else {
        flag.to_vec()
    }
}

/// The load settings carrying `[simulation] tick_rate_hz` (ADR 0029).
pub fn load_settings(config: &Config) -> LoadSettings {
    let mut settings = LoadSettings::default();
    if let Some(rate) = config.tick_rate {
        settings.default_tick_rate = rate;
    }
    settings
}

/// The log level: `--log-level`, else `[log] level`, else `warn`.
pub fn log_level(flag: Option<LevelFilter>, config: &Config) -> LevelFilter {
    flag.unwrap_or_else(|| {
        config
            .log_level
            .map_or(LevelFilter::WARN, |level| match level {
                LogLevel::Off => LevelFilter::OFF,
                LogLevel::Error => LevelFilter::ERROR,
                LogLevel::Warn => LevelFilter::WARN,
                LogLevel::Info => LevelFilter::INFO,
                LogLevel::Debug => LevelFilter::DEBUG,
                LogLevel::Trace => LevelFilter::TRACE,
            })
    })
}
