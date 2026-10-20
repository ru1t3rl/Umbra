//! AT-18 (reader rows, WI-3): the `umbra.toml` reader. Requirements: REQ-X-2, REQ-X-8, REQ-T-2,
//! REQ-P-10, ADR 0029.

#![forbid(unsafe_code)]

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use umbra_core::{ParamSchema, TickRate};
use umbra_templates::config::{Config, LogLevel, read_config};
use umbra_templates::value::parse;
use umbra_templates::{CONFIG_FILE_SCHEMA, CompiledSchema, Diagnostic, DiagnosticCode, SourceFile};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/config")
        .join(name)
}

fn source(name: &str) -> SourceFile {
    let path = fixture(name);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
    SourceFile::new(path, text)
}

fn read_ok(name: &str) -> Config {
    read_config(&source(name)).unwrap_or_else(|d| panic!("{name}: {:?}", d.into_vec()))
}

fn errors(text: &str) -> Vec<Diagnostic> {
    match read_config(&SourceFile::new("umbra.toml", text)) {
        Ok(c) => panic!("expected errors, got {c:?}"),
        Err(d) => d.into_vec(),
    }
}

fn codes(list: &[Diagnostic]) -> Vec<(DiagnosticCode, u32)> {
    list.iter()
        .map(|d| (d.code, d.location.as_ref().map_or(0, |l| l.line)))
        .collect()
}

#[test]
fn empty_file_gives_all_defaults() {
    let config = read_ok("empty.toml");
    assert_eq!(config, Config::default());
    assert_eq!(config.tick_rate, None);
    assert_eq!(config.plugin_paths, None);
    assert_eq!(config.log_level, None);
    let s = &config.server;
    assert_eq!(s.bind, SocketAddr::from(([127, 0, 0, 1], 7878)));
    assert_eq!(s.scenario_root, Path::new("."));
    assert_eq!((s.body_limit_bytes, s.command_queue), (1_048_576, 64));
    assert!(s.allowed_hosts.is_empty() && s.allowed_origins.is_empty());
    assert_eq!(s.shutdown_grace, Duration::from_secs(5));
    let st = &s.stream;
    assert_eq!((st.default_rate_hz, st.max_rate_hz), (30.0, 60.0));
    assert_eq!((st.queue_frames, st.max_events_per_frame), (4, 4096));
    assert_eq!(
        (st.send_timeout, st.max_connections),
        (Duration::from_secs(10), 64)
    );
}

#[test]
fn full_example_file_reads_every_key() {
    let config = read_ok("full.toml");
    let base = fixture("full.toml");
    let dir = base.parent().unwrap_or(Path::new(""));
    assert_eq!(config.tick_rate, TickRate::new(30));
    assert_eq!(
        config.plugin_paths,
        Some(vec![dir.join("plugins"), dir.join("../shared/plugins")])
    );
    assert_eq!(config.log_level, Some(LogLevel::Info));
    let s = &config.server;
    assert_eq!(s.bind, SocketAddr::from(([0, 0, 0, 0], 9000)));
    assert_eq!(s.scenario_root, dir.join("scenarios"));
    assert_eq!((s.body_limit_bytes, s.command_queue), (2_097_152, 128));
    assert_eq!(s.allowed_hosts, ["umbra.example", "10.0.0.5:9000"]);
    assert_eq!(
        s.allowed_origins,
        ["http://localhost:5173", "https://ui.example"]
    );
    assert_eq!(s.shutdown_grace, Duration::from_secs_f64(2.5));
    let st = &s.stream;
    assert_eq!((st.default_rate_hz, st.max_rate_hz), (20.0, 45.5));
    assert_eq!((st.queue_frames, st.max_events_per_frame), (8, 1000));
    assert_eq!(
        (st.send_timeout, st.max_connections),
        (Duration::from_secs(3), 16)
    );
}

#[test]
fn unknown_key_at_every_level_is_located() {
    let text = "colour = 1\n[simulation]\nx = 1\n[plugins]\nx = 1\n[log]\nx = 1\n[server]\nx = 1\n[server.stream]\nx = 1\n";
    let found = errors(text);
    assert_eq!(
        codes(&found),
        [1, 3, 5, 7, 9, 11].map(|l| (DiagnosticCode::UnknownKey, l))
    );
    let first = found[0].to_string();
    assert!(
        first.starts_with("umbra.toml:1:1: error[unknown_key]"),
        "{first}"
    );
}

#[test]
fn wrong_type_and_invalid_value_rows() {
    let rows: [(&str, DiagnosticCode); 29] = [
        (
            "[simulation]\ntick_rate_hz = \"60\"",
            DiagnosticCode::WrongType,
        ),
        (
            "[simulation]\ntick_rate_hz = 30.5",
            DiagnosticCode::WrongType,
        ),
        (
            "[simulation]\ntick_rate_hz = 0",
            DiagnosticCode::InvalidValue,
        ),
        (
            "[simulation]\ntick_rate_hz = -5",
            DiagnosticCode::InvalidValue,
        ),
        ("[plugins]\npaths = \"plugins\"", DiagnosticCode::WrongType),
        ("[plugins]\npaths = [1]", DiagnosticCode::WrongType),
        ("[log]\nlevel = \"loud\"", DiagnosticCode::InvalidValue),
        ("[log]\nlevel = 3", DiagnosticCode::WrongType),
        (
            "[server]\nbind = \"localhost\"",
            DiagnosticCode::InvalidValue,
        ),
        ("[server]\nbind = 7878", DiagnosticCode::WrongType),
        (
            "[server]\nbody_limit_bytes = 0",
            DiagnosticCode::InvalidValue,
        ),
        (
            "[server]\ncommand_queue = \"many\"",
            DiagnosticCode::WrongType,
        ),
        (
            "[server]\nallowed_origins = [\"localhost:5173\"]",
            DiagnosticCode::InvalidValue,
        ),
        (
            "[server]\nallowed_origins = [\"http://host/path\"]",
            DiagnosticCode::InvalidValue,
        ),
        (
            "[server]\nallowed_origins = [\"http://host:99999\"]",
            DiagnosticCode::InvalidValue,
        ),
        (
            "[server]\nallowed_hosts = [\"\"]",
            DiagnosticCode::InvalidValue,
        ),
        (
            "[server]\nshutdown_grace_s = 0",
            DiagnosticCode::InvalidValue,
        ),
        (
            "[server.stream]\nmax_rate_hz = 61",
            DiagnosticCode::InvalidValue,
        ),
        (
            "[server.stream]\ndefault_rate_hz = 0",
            DiagnosticCode::InvalidValue,
        ),
        (
            "[server.stream]\nsend_timeout_s = -1.0",
            DiagnosticCode::InvalidValue,
        ),
        (
            "[server.stream]\nmax_connections = 0",
            DiagnosticCode::InvalidValue,
        ),
        ("server = 3", DiagnosticCode::WrongType),
        (
            "[server]\nallowed_origins = [\"http://host:+80\"]",
            DiagnosticCode::InvalidValue,
        ),
        (
            "[server]\nallowed_origins = [\"http://[nope]:80\"]",
            DiagnosticCode::InvalidValue,
        ),
        (
            "[server]\nallowed_origins = [\"http://Example.COM\"]",
            DiagnosticCode::InvalidValue,
        ),
        ("[plugins]\npaths = [\"\"]", DiagnosticCode::InvalidValue),
        ("schema_version = 0", DiagnosticCode::InvalidValue),
        (
            "[server]\nshutdown_grace_s = 1e300",
            DiagnosticCode::InvalidValue,
        ),
        (
            "[server]\nallowed_origins = [\"http://[::1\"]",
            DiagnosticCode::InvalidValue,
        ),
    ];
    for (text, code) in rows {
        let found = errors(text);
        assert_eq!(found.len(), 1, "{text:?}: {found:?}");
        assert_eq!(found[0].code, code, "{text:?}: {found:?}");
        assert!(found[0].location.is_some(), "{text:?} must be located");
    }
}

#[test]
fn stream_rate_ordering_is_checked_on_the_effective_values() {
    assert_eq!(
        errors("[server.stream]\ndefault_rate_hz = 50\nmax_rate_hz = 40").len(),
        1
    );
    // The default of 30 applies when only the maximum is given.
    assert_eq!(errors("[server.stream]\nmax_rate_hz = 20").len(), 1);
    assert!(
        read_config(&SourceFile::new(
            "c.toml",
            "[server.stream]\ndefault_rate_hz = 60"
        ))
        .is_ok()
    );
}

#[test]
fn all_errors_reported_not_just_the_first() {
    let text = "colour = 1\n[simulation]\ntick_rate_hz = 0\n[log]\nlevel = \"x\"\n[server]\nbind = \"nope\"\ncommand_queue = 0\n[server.stream]\nmax_rate_hz = 99\nrate = 1\n";
    let found = errors(text);
    assert_eq!(
        codes(&found),
        [
            (DiagnosticCode::UnknownKey, 1),
            (DiagnosticCode::InvalidValue, 3),
            (DiagnosticCode::InvalidValue, 5),
            (DiagnosticCode::InvalidValue, 7),
            (DiagnosticCode::InvalidValue, 8),
            (DiagnosticCode::InvalidValue, 10),
            (DiagnosticCode::UnknownKey, 11),
        ]
    );
}

#[test]
fn syntax_errors_are_located() {
    let found = errors("[server\nbind = 1");
    assert!(
        found.iter().all(|d| d.code == DiagnosticCode::Syntax),
        "{found:?}"
    );
}

#[test]
fn newer_schema_version_is_unsupported() {
    let found = errors("schema_version = 2\n[simulation]\ntick_rate_hz = 0");
    assert_eq!(
        codes(&found),
        [(DiagnosticCode::UnsupportedSchemaVersion, 1)]
    );
    assert!(read_config(&SourceFile::new("c.toml", "schema_version = 1")).is_ok());
    assert_eq!(
        errors("schema_version = 0")[0].code,
        DiagnosticCode::InvalidValue
    );
    assert_eq!(
        errors("schema_version = \"1\"")[0].code,
        DiagnosticCode::WrongType
    );
}

#[test]
fn relative_paths_resolve_against_the_config_directory() {
    let text = "[plugins]\npaths = [\"p\", \"/abs/q\"]\n[server]\nscenario_root = \"root\"";
    let file = SourceFile::new("some/dir/umbra.toml", text);
    let config = read_config(&file).unwrap_or_else(|d| panic!("{:?}", d.into_vec()));
    assert_eq!(
        config.plugin_paths,
        Some(vec![PathBuf::from("some/dir/p"), PathBuf::from("/abs/q")])
    );
    assert_eq!(config.server.scenario_root, Path::new("some/dir/root"));
    // A file in the working directory keeps paths relative to it.
    let local = SourceFile::new("umbra.toml", "[plugins]\npaths = [\"p\"]");
    let config = read_config(&local).unwrap_or_else(|d| panic!("{:?}", d.into_vec()));
    assert_eq!(config.plugin_paths, Some(vec![PathBuf::from("p")]));
}

#[test]
fn ipv6_bind_parses() {
    let config = read_ok("ipv6.toml");
    assert_eq!(
        config.server.bind,
        SocketAddr::from((std::net::Ipv6Addr::LOCALHOST, 7878))
    );
    let origins = "[server]\nallowed_origins = [\"http://[::1]:5173\", \"tauri://localhost\"]";
    assert!(read_config(&SourceFile::new("c.toml", origins)).is_ok());
}

fn compiled() -> CompiledSchema {
    let parsed = ParamSchema::from_json_str(CONFIG_FILE_SCHEMA)
        .unwrap_or_else(|e| panic!("the config schema must be a JSON object: {e}"));
    CompiledSchema::compile(&parsed)
        .unwrap_or_else(|e| panic!("the config schema must compile: {e}"))
}

fn schema_violations(schema: &CompiledSchema, file: &SourceFile) -> usize {
    let (table, syntax) = parse(file);
    assert!(syntax.is_empty(), "{}: {syntax:?}", file.path().display());
    let root = table
        .unwrap_or_else(|| panic!("no root table"))
        .into_value();
    schema.validate(&root, "config").len()
}

#[test]
fn config_schema_accepts_the_example_and_rejects_invalid_fixtures() {
    let schema = compiled();
    assert!(CONFIG_FILE_SCHEMA.contains("urn:umbra:schema:config:v1"));
    for name in ["full.toml", "empty.toml", "ipv6.toml"] {
        assert_eq!(schema_violations(&schema, &source(name)), 0, "{name}");
    }
    let dir = fixture("invalid");
    let mut checked = 0;
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("reading {}: {e}", dir.display()))
        .filter_map(Result::ok)
        .map(|e| e.path())
        .collect();
    files.sort();
    for path in files {
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        let file = SourceFile::new(&path, text);
        // The reader rejects every invalid fixture. The schema rejects those it can express
        // (everything except the `bind` format).
        assert!(
            read_config(&file).is_err(),
            "{} must be rejected",
            path.display()
        );
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        if name != "bad_bind.toml" {
            assert!(
                schema_violations(&schema, &file) > 0,
                "schema accepted {name}"
            );
        }
        checked += 1;
    }
    assert_eq!(checked, 8);
}

#[test]
fn ordering_message_says_when_the_default_is_implied() {
    let found = errors("[server.stream]\nmax_rate_hz = 20");
    assert!(
        found[0]
            .message
            .contains("the default; set `default_rate_hz` too"),
        "{}",
        found[0].message
    );
    let found = errors("[server.stream]\ndefault_rate_hz = 50\nmax_rate_hz = 40");
    assert!(
        !found[0].message.contains("the default"),
        "{}",
        found[0].message
    );
}

#[test]
fn too_large_seconds_are_reported_as_too_large() {
    let found = errors("[server]\nshutdown_grace_s = 1e300");
    assert!(
        found[0].message.contains("too large"),
        "{}",
        found[0].message
    );
}
