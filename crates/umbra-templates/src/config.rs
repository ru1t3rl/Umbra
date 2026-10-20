//! The `umbra.toml` configuration file reader (ADR 0029, REQ-X-2, REQ-X-8, REQ-T-2, REQ-P-10).
//!
//! Same located-diagnostics pipeline as the template and scenario files (ADR 0008, ADR 0010): no
//! serde, every error is reported with `path:line:col`, unknown keys are errors at every level. The
//! reader returns plain values; mapping them onto handles and server types is the caller's job, so
//! this crate stays free of server types. Relative paths in the file resolve against the file's
//! directory.

use std::net::{Ipv6Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::time::Duration;

use umbra_core::TickRate;

use crate::diagnostic::{DiagnosticCode, Diagnostics, sort_and_dedup};
use crate::reader::Reader;
use crate::scenario_read::read_tick_rate;
use crate::source::SourceFile;
use crate::template::Version;
use crate::value::{LKind, LTable, LValue, Origin, parse};

/// The highest stream rate a subscriber may be given (ADR 0027 ceiling).
pub const MAX_STREAM_RATE_HZ: f64 = 60.0;

const TOP_KEYS: [&str; 5] = ["schema_version", "simulation", "plugins", "log", "server"];
const SIMULATION_KEYS: [&str; 1] = ["tick_rate_hz"];
const PLUGINS_KEYS: [&str; 1] = ["paths"];
const LOG_KEYS: [&str; 1] = ["level"];
const SERVER_KEYS: [&str; 8] = [
    "bind",
    "scenario_root",
    "body_limit_bytes",
    "command_queue",
    "allowed_hosts",
    "allowed_origins",
    "shutdown_grace_s",
    "stream",
];
const STREAM_KEYS: [&str; 6] = [
    "default_rate_hz",
    "max_rate_hz",
    "queue_frames",
    "max_events_per_frame",
    "send_timeout_s",
    "max_connections",
];

/// A log level name of `[log] level`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    /// No logging.
    Off,
    /// Errors only.
    Error,
    /// Warnings and errors.
    Warn,
    /// Informational messages too.
    Info,
    /// Debug messages too.
    Debug,
    /// Everything.
    Trace,
}

impl LogLevel {
    /// The names accepted in the file.
    pub const NAMES: [&'static str; 6] = ["off", "error", "warn", "info", "debug", "trace"];

    /// The level named `name` (lower case), if it is one of [`Self::NAMES`].
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "off" => Self::Off,
            "error" => Self::Error,
            "warn" => Self::Warn,
            "info" => Self::Info,
            "debug" => Self::Debug,
            "trace" => Self::Trace,
            _ => return None,
        })
    }

    /// The name used in the file.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Error => "error",
            Self::Warn => "warn",
            Self::Info => "info",
            Self::Debug => "debug",
            Self::Trace => "trace",
        }
    }
}

/// `[server.stream]`, with the ADR 0029 defaults.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct StreamConfig {
    /// Rate used when a subscriber gives no `max_rate_hz`; default 30.
    pub default_rate_hz: f64,
    /// Upper bound a subscriber may ask for; default 60.
    pub max_rate_hz: f64,
    /// Per-connection subscription capacity in frames; default 4.
    pub queue_frames: usize,
    /// Events per frame before the rest is dropped; default 4096.
    pub max_events_per_frame: usize,
    /// A subscriber that cannot take a frame within this time is closed; default 10 s.
    pub send_timeout: Duration,
    /// The most simultaneous stream connections; default 64.
    pub max_connections: usize,
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            default_rate_hz: 30.0,
            max_rate_hz: MAX_STREAM_RATE_HZ,
            queue_frames: 4,
            max_events_per_frame: 4096,
            send_timeout: Duration::from_secs(10),
            max_connections: 64,
        }
    }
}

/// `[server]`, with the ADR 0029 defaults.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct ServerConfig {
    /// The listen address; default `127.0.0.1:7878`.
    pub bind: SocketAddr,
    /// Loads are confined to this directory (ADR 0026); default `.`.
    pub scenario_root: PathBuf,
    /// The largest request body; default 1 MiB.
    pub body_limit_bytes: u64,
    /// Capacity of the engine command channel (ADR 0024); default 64.
    pub command_queue: usize,
    /// Extra accepted `Host` values (names or addresses, port optional); default none.
    pub allowed_hosts: Vec<String>,
    /// Extra accepted WebSocket origins, `scheme://host[:port]`; default none.
    pub allowed_origins: Vec<String>,
    /// Graceful-shutdown deadline; default 5 s.
    pub shutdown_grace: Duration,
    /// `[server.stream]`.
    pub stream: StreamConfig,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind: SocketAddr::from(([127, 0, 0, 1], 7878)),
            scenario_root: PathBuf::from("."),
            body_limit_bytes: 1_048_576,
            command_queue: 64,
            allowed_hosts: Vec::new(),
            allowed_origins: Vec::new(),
            shutdown_grace: Duration::from_secs(5),
            stream: StreamConfig::default(),
        }
    }
}

/// The settings of one `umbra.toml`. Keys the file leaves out keep their defaults; the keys with
/// no built-in value of their own (`None`) leave the caller's default in force.
#[derive(Debug, Clone, Default, PartialEq)]
#[non_exhaustive]
pub struct Config {
    /// `[simulation] tick_rate_hz`: the default for scenarios that omit it.
    pub tick_rate: Option<TickRate>,
    /// `[plugins] paths`, resolved against the file's directory.
    pub plugin_paths: Option<Vec<PathBuf>>,
    /// `[log] level`.
    pub log_level: Option<LogLevel>,
    /// `[server]`.
    pub server: ServerConfig,
}

/// Why a configuration file could not be used.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// The file could not be read.
    #[error("cannot read {path}: {source}")]
    Io {
        /// The file.
        path: PathBuf,
        /// Why.
        #[source]
        source: std::io::Error,
    },
    /// The file has errors; every one is listed.
    #[error(transparent)]
    Invalid(#[from] Diagnostics),
}

/// Reads the configuration file at `path`.
///
/// # Errors
/// [`ConfigError::Io`] if it cannot be read; [`ConfigError::Invalid`] with every diagnostic if it
/// is not a valid configuration.
pub fn load_config(path: &Path) -> Result<Config, ConfigError> {
    let text = std::fs::read_to_string(path).map_err(|source| ConfigError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(read_config(&SourceFile::new(path, text))?)
}

/// Reads configuration text. An empty file gives the defaults.
///
/// # Errors
/// Every diagnostic of the file, sorted, when there is at least one.
pub fn read_config(file: &SourceFile) -> Result<Config, Diagnostics> {
    let (table, mut diagnostics) = parse(file);
    let mut config = Config::default();
    if diagnostics.is_empty()
        && let Some(table) = &table
    {
        let mut reader = Reader::new(file);
        if !matches!(check_optional_version(&mut reader, table), Version::Newer) {
            read_top(&mut reader, table, file, &mut config);
        }
        diagnostics.extend(reader.into_diagnostics());
    }
    sort_and_dedup(&mut diagnostics);
    Diagnostics::new(diagnostics).map_or(Ok(config), Err)
}

/// The version of the configuration file format (ADR 0029). Independent of the template and
/// scenario `SCHEMA_VERSION` (ASM-169).
pub const CONFIG_SCHEMA_VERSION: u32 = 1;

/// `schema_version` is optional here (absent means 1), unlike in scenario files (ADR 0029). When
/// present the codes and messages are those of `template::check_version`, against
/// [`CONFIG_SCHEMA_VERSION`].
fn check_optional_version(reader: &mut Reader<'_>, table: &LTable) -> Version {
    let Some(value) = table.get("schema_version") else {
        return Version::Ok;
    };
    let LKind::Integer(version) = value.kind() else {
        if matches!(value.kind(), LKind::Unsupported) {
            return Version::Bad; // already reported as `unsupported_value`
        }
        let message = format!(
            "`schema_version` must be an integer, found {}",
            value.kind().type_name()
        );
        reader.error(DiagnosticCode::WrongType, message, value.origin());
        return Version::Bad;
    };
    if *version > i64::from(CONFIG_SCHEMA_VERSION) {
        let message = format!(
            "schema_version {version} is newer than the newest supported version {CONFIG_SCHEMA_VERSION}"
        );
        let code = DiagnosticCode::UnsupportedSchemaVersion;
        reader.error(code, message, value.origin());
        return Version::Newer;
    }
    if *version < 1 {
        let message = format!("schema_version must be at least 1, found {version}");
        reader.error(DiagnosticCode::InvalidValue, message, value.origin());
        return Version::Bad;
    }
    Version::Ok
}

fn read_top(reader: &mut Reader<'_>, table: &LTable, file: &SourceFile, config: &mut Config) {
    reader.unknown_keys(table, &TOP_KEYS);
    let base = file.path().parent().unwrap_or_else(|| Path::new(""));
    if let Some(t) = section(reader, table, "simulation") {
        reader.unknown_keys(t, &SIMULATION_KEYS);
        if let Some(v) = t.get("tick_rate_hz") {
            config.tick_rate = read_tick_rate(reader, v);
        }
    }
    if let Some(t) = section(reader, table, "plugins") {
        reader.unknown_keys(t, &PLUGINS_KEYS);
        if let Some(v) = t.get("paths") {
            config.plugin_paths =
                read_paths(reader, v).map(|list| list.iter().map(|p| base.join(p)).collect());
        }
    }
    if let Some(t) = section(reader, table, "log") {
        reader.unknown_keys(t, &LOG_KEYS);
        if let Some(v) = t.get("level") {
            config.log_level = read_level(reader, v);
        }
    }
    if let Some(t) = section(reader, table, "server") {
        read_server(reader, t, base, &mut config.server);
    }
}

fn section<'t>(reader: &mut Reader<'_>, table: &'t LTable, key: &str) -> Option<&'t LTable> {
    table.get(key).and_then(|v| reader.table(key, v))
}

fn read_server(reader: &mut Reader<'_>, table: &LTable, base: &Path, server: &mut ServerConfig) {
    reader.unknown_keys(table, &SERVER_KEYS);
    if let Some(v) = table.get("bind")
        && let Some(addr) = read_bind(reader, v)
    {
        server.bind = addr;
    }
    if let Some(v) = table.get("scenario_root")
        && let Some(root) = reader.string("scenario_root", v)
    {
        server.scenario_root = base.join(root);
    }
    set(
        &mut server.body_limit_bytes,
        count(reader, table, "body_limit_bytes"),
    );
    set(
        &mut server.command_queue,
        size(reader, table, "command_queue"),
    );
    if let Some(v) = table.get("allowed_hosts")
        && let Some(list) = read_hosts(reader, v)
    {
        server.allowed_hosts = list;
    }
    if let Some(v) = table.get("allowed_origins")
        && let Some(list) = read_origins(reader, v)
    {
        server.allowed_origins = list;
    }
    set(
        &mut server.shutdown_grace,
        seconds(reader, table, "shutdown_grace_s"),
    );
    if let Some(stream) = section(reader, table, "stream") {
        read_stream(reader, stream, &mut server.stream);
    }
}

fn read_stream(reader: &mut Reader<'_>, table: &LTable, stream: &mut StreamConfig) {
    reader.unknown_keys(table, &STREAM_KEYS);
    let default_rate = rate(reader, table, "default_rate_hz");
    let max_rate = rate(reader, table, "max_rate_hz");
    set(
        &mut stream.default_rate_hz,
        default_rate.as_ref().map(|(r, _)| *r),
    );
    set(&mut stream.max_rate_hz, max_rate.as_ref().map(|(r, _)| *r));
    let both_valid = [
        ("default_rate_hz", &default_rate),
        ("max_rate_hz", &max_rate),
    ]
    .iter()
    .all(|(key, r)| r.is_some() || table.get(key).is_none());
    if both_valid && stream.default_rate_hz > stream.max_rate_hz {
        let implied = default_rate.is_none();
        let at = default_rate.or(max_rate).map(|(_, origin)| origin);
        if let Some(at) = at {
            let hint = if implied {
                " (the default; set `default_rate_hz` too)"
            } else {
                ""
            };
            let message = format!(
                "`default_rate_hz` ({}{hint}) must not exceed `max_rate_hz` ({})",
                stream.default_rate_hz, stream.max_rate_hz
            );
            reader.error(DiagnosticCode::InvalidValue, message, &at);
        }
    }
    set(
        &mut stream.queue_frames,
        size(reader, table, "queue_frames"),
    );
    set(
        &mut stream.max_events_per_frame,
        size(reader, table, "max_events_per_frame"),
    );
    set(
        &mut stream.send_timeout,
        seconds(reader, table, "send_timeout_s"),
    );
    set(
        &mut stream.max_connections,
        size(reader, table, "max_connections"),
    );
}

fn set<T>(slot: &mut T, value: Option<T>) {
    if let Some(v) = value {
        *slot = v;
    }
}

fn read_level(reader: &mut Reader<'_>, value: &LValue) -> Option<LogLevel> {
    let name = reader.string("level", value)?;
    let level = LogLevel::from_name(name);
    if level.is_none() {
        let message = format!(
            "`level` must be one of {}, found {name:?}",
            LogLevel::NAMES.join(", ")
        );
        reader.error(DiagnosticCode::InvalidValue, message, value.origin());
    }
    level
}

fn read_bind(reader: &mut Reader<'_>, value: &LValue) -> Option<SocketAddr> {
    let text = reader.string("bind", value)?;
    match text.parse::<SocketAddr>() {
        Ok(addr) => Some(addr),
        Err(error) => {
            let message = format!(
                "`bind` must be a socket address such as \"127.0.0.1:7878\" or \"[::1]:7878\": {error}"
            );
            reader.error(DiagnosticCode::InvalidValue, message, value.origin());
            None
        }
    }
}

/// An array of strings, each passed through `check` (an `Err` is the reason it is invalid). Every
/// wrong element is reported; `None` if anything was wrong.
fn string_entries(
    reader: &mut Reader<'_>,
    name: &str,
    value: &LValue,
    expected: &str,
    check: impl Fn(&str) -> Result<(), String>,
) -> Option<Vec<String>> {
    let items = reader.array(name, value)?;
    let mut ok = true;
    let mut out = Vec::new();
    for item in items {
        let Some(text) = reader.string(name, item) else {
            ok = false;
            continue;
        };
        if let Err(why) = check(text) {
            let message = format!("`{name}` entry {text:?} is not {expected}: {why}");
            reader.error(DiagnosticCode::InvalidValue, message, item.origin());
            ok = false;
        } else {
            out.push(text.to_string());
        }
    }
    ok.then_some(out)
}

fn read_paths(reader: &mut Reader<'_>, value: &LValue) -> Option<Vec<String>> {
    string_entries(reader, "paths", value, "a directory", |t| {
        if t.is_empty() {
            Err("empty".into())
        } else {
            Ok(())
        }
    })
}

fn read_hosts(reader: &mut Reader<'_>, value: &LValue) -> Option<Vec<String>> {
    let expected = "a host name or address with an optional port";
    string_entries(reader, "allowed_hosts", value, expected, |t| {
        if t.is_empty() || t.contains(|c: char| c.is_whitespace() || c == '/') {
            Err("empty, or has whitespace or `/`".into())
        } else {
            Ok(())
        }
    })
}

fn read_origins(reader: &mut Reader<'_>, value: &LValue) -> Option<Vec<String>> {
    string_entries(
        reader,
        "allowed_origins",
        value,
        "`scheme://host[:port]`",
        |t| check_origin(t).map_err(String::from),
    )
}

/// Checks `scheme://host[:port]` with no path, query, fragment or user info (ASM-169).
fn check_origin(text: &str) -> Result<(), &'static str> {
    let (scheme, rest) = text.split_once("://").ok_or("no `://`")?;
    let mut chars = scheme.chars();
    let scheme_ok = chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "+-.".contains(c));
    if !scheme_ok {
        return Err("the scheme must be lower case letters, digits, `+`, `-` or `.`");
    }
    let (host, port) = split_host_port(rest)?;
    if let Some(inner) = host.strip_prefix('[').and_then(|h| h.strip_suffix(']')) {
        if inner.parse::<Ipv6Addr>().is_err() {
            return Err("the text in brackets is not an IPv6 address");
        }
    } else if host.is_empty()
        || host.contains(|c: char| c.is_whitespace() || "/?#@\\[]".contains(c))
    {
        return Err("the host is empty or has a path, query, fragment or user info");
    }
    if host.bytes().any(|b| b.is_ascii_uppercase()) {
        return Err("origins are lower case; write the host in lower case");
    }
    if let Some(port) = port
        && !(port.bytes().all(|b| b.is_ascii_digit()) && port.parse::<u16>().is_ok())
    {
        return Err("the port is not a number from 0 to 65535");
    }
    Ok(())
}

/// Splits `host`, `host:port`, `[v6]` or `[v6]:port`.
fn split_host_port(rest: &str) -> Result<(&str, Option<&str>), &'static str> {
    if rest.starts_with('[') {
        let end = rest.find(']').ok_or("unclosed `[`")?;
        let (host, tail) = rest.split_at(end + 1);
        return match tail {
            "" => Ok((host, None)),
            _ => tail
                .strip_prefix(':')
                .map(|p| (host, Some(p)))
                .ok_or("text after `]`"),
        };
    }
    Ok(rest
        .split_once(':')
        .map_or((rest, None), |(h, p)| (h, Some(p))))
}

/// A positive integer key as `u64`.
fn count(reader: &mut Reader<'_>, table: &LTable, key: &str) -> Option<u64> {
    let value = table.get(key)?;
    let n = reader.integer(key, value)?;
    u64::try_from(n).ok().filter(|&n| n > 0).or_else(|| {
        let message = format!("`{key}` must be a positive whole number, found {n}");
        reader.error(DiagnosticCode::InvalidValue, message, value.origin());
        None
    })
}

/// A positive integer key as `usize`.
fn size(reader: &mut Reader<'_>, table: &LTable, key: &str) -> Option<usize> {
    let n = count(reader, table, key)?;
    usize::try_from(n).ok().or_else(|| {
        let message = format!("`{key}` is too large, found {n}");
        if let Some(v) = table.get(key) {
            reader.error(DiagnosticCode::InvalidValue, message, v.origin());
        }
        None
    })
}

/// A finite, positive number of seconds as a [`Duration`].
fn seconds(reader: &mut Reader<'_>, table: &LTable, key: &str) -> Option<Duration> {
    let value = table.get(key)?;
    let s = reader.number(key, value)?;
    if !(s.is_finite() && s > 0.0) {
        let message =
            format!("`{key}` must be a finite number of seconds greater than 0, found {s}");
        reader.error(DiagnosticCode::InvalidValue, message, value.origin());
        return None;
    }
    let duration = Duration::try_from_secs_f64(s).ok();
    if duration.is_none() {
        let message = format!("`{key}` is too large, found {s}");
        reader.error(DiagnosticCode::InvalidValue, message, value.origin());
    }
    duration
}

/// A stream rate in (0, 60], with the origin of its value.
fn rate(reader: &mut Reader<'_>, table: &LTable, key: &str) -> Option<(f64, Origin)> {
    let value = table.get(key)?;
    let hz = reader.number(key, value)?;
    if hz.is_finite() && hz > 0.0 && hz <= MAX_STREAM_RATE_HZ {
        Some((hz, value.origin().clone()))
    } else {
        let message =
            format!("`{key}` must be greater than 0 and at most {MAX_STREAM_RATE_HZ}, found {hz}");
        reader.error(DiagnosticCode::InvalidValue, message, value.origin());
        None
    }
}
