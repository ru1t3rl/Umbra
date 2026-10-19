//! AT-7 (process row), AT-8 (process rows) and the CLI rows of WI-12: `umbra run`, `umbra validate`
//! and `umbra list-plugins` with `--plugin-path`, as real processes.
//! Requirements: M4 acceptance 1, REQ-K-4 (cross-process determinism), REQ-P-7, REQ-P-10, REQ-X-2,
//! REQ-T-6, ASM-113, ASM-114.
//!
//! Plugin libraries are built by the nested-cargo helper shared with the engine tests
//! (`plugin_support`, included by path). Contract with the example scenario (WI-13): entities
//! `wander-direct`, `wander-graph` and `patrol-builtin`. Step 4 of M4: written before WI-12/WI-13;
//! the rows that need the example are ignored with WI-13, those that need only the fixture plugin
//! and the new flags with WI-12.
//!
//! Assumptions the plan leaves open (reported): `list-plugins --json` prints one JSON document that
//! contains, as string values, the plugin id, its version, each capability id and its kind; the text
//! form prints the same words. A load or fault error is exit 1 with the message on stderr (like every
//! other runtime error of the CLI).

#![forbid(unsafe_code)]

#[path = "../../umbra-engine/tests/plugin_support/mod.rs"]
mod plugin_support;

mod common;

use std::path::{Path, PathBuf};
use std::process::Output;
use std::sync::LazyLock;

use common::{cli, final_hash, is_16_lower_hex, json_of, stderr, stdout};

static WANDER: LazyLock<PathBuf> = LazyLock::new(plugin_support::wander);
static FIXTURE: LazyLock<PathBuf> = LazyLock::new(|| plugin_support::fixture(&[]));

fn s(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

fn example_scenario() -> String {
    s(&plugin_support::wander_example_dir().join("scenario.toml"))
}

fn run_example(extra: &[&str]) -> Output {
    let (scenario, plugins) = (example_scenario(), s(&WANDER));
    let mut args = vec!["run", scenario.as_str(), "--plugin-path", plugins.as_str()];
    args.extend_from_slice(extra);
    cli(&args)
}

/// Every string value anywhere in a JSON document.
fn strings(value: &serde_json::Value, out: &mut Vec<String>) {
    match value {
        serde_json::Value::String(text) => out.push(text.clone()),
        serde_json::Value::Array(items) => items.iter().for_each(|v| strings(v, out)),
        serde_json::Value::Object(map) => map.values().for_each(|v| strings(v, out)),
        _ => {}
    }
}

/// REQ-K-4, AT-7: four separate processes print byte-identical output, with every entity state line
/// and the state hash after every tick, 900 ticks (three wander periods). Separate processes have
/// different hasher seeds and address-space layouts and load the library afresh each time.
#[test]
#[ignore = "WI-13"]
fn plugin_wander_in_four_processes_is_byte_identical() {
    let args = [
        "--json",
        "--print-every",
        "1",
        "--hash-every",
        "1",
        "--ticks",
        "900",
    ];

    let runs: Vec<Output> = (0..4).map(|_| run_example(&args)).collect();

    for r in &runs {
        assert_eq!(r.status.code(), Some(0), "stderr: {}", stderr(r));
    }
    let text = stdout(&runs[0]);
    let hash_lines = text
        .lines()
        .filter(|l| l.contains("\"state_hash\""))
        .count();
    assert_eq!(hash_lines, 901, "one hash per tick plus the final line");
    for r in &runs[1..] {
        assert_eq!(r.stdout, runs[0].stdout);
    }
}

/// M4 acceptance 1, REQ-S-1: `run` with `--plugin-path` prints the state lines of all three entities
/// and a 16-digit final state hash; the plugin line (id, version, ABI) goes to stderr at info level.
#[test]
#[ignore = "WI-13"]
fn run_example_with_plugin_path() {
    let out = run_example(&["--ticks", "300", "--log-level", "info"]);

    let text = stdout(&out);
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    for name in ["wander-direct", "wander-graph", "patrol-builtin"] {
        assert!(
            text.lines()
                .any(|l| l.starts_with("tick=30 ") && l.contains(&format!("name={name} "))),
            "no state line for {name} at tick 30 in:\n{text}"
        );
    }
    assert!(
        final_hash(&text).as_deref().is_some_and(is_16_lower_hex),
        "{text}"
    );
    let err = stderr(&out);
    assert!(
        err.contains("example.plugin.wander") && err.contains("0.1.0") && err.contains("0.0"),
        "the plugin line: {err}"
    );
    assert!(
        !text.contains("example.plugin.wander"),
        "stdout stays free of plugin lines"
    );
}

/// REQ-T-6: the example directory validates with `--plugin-path`.
#[test]
#[ignore = "WI-13"]
fn validate_example_with_plugin_path_ok() {
    let (dir, plugins) = (s(&plugin_support::wander_example_dir()), s(&WANDER));

    let out = cli(&["validate", dir.as_str(), "--plugin-path", plugins.as_str()]);

    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    let text = stdout(&out);
    let last = text.lines().last().unwrap_or_default();
    assert!(
        last.starts_with("ok: ") && last.ends_with(" files checked"),
        "{text}"
    );
}

/// REQ-X-2, decision 7: `list-plugins` prints the plugins with their capabilities, versions and kinds,
/// as text and as one JSON document.
#[test]
#[ignore = "WI-13"]
fn list_plugins_text_and_json() {
    let plugins = s(&WANDER);

    let text = cli(&["list-plugins", "--plugin-path", plugins.as_str()]);
    let json = cli(&["list-plugins", "--plugin-path", plugins.as_str(), "--json"]);

    assert_eq!(text.status.code(), Some(0), "stderr: {}", stderr(&text));
    let shown = stdout(&text);
    for word in [
        "example.plugin.wander",
        "0.1.0",
        "example.behavior.wander",
        "behavior",
    ] {
        assert!(shown.contains(word), "text lacks {word:?}:\n{shown}");
    }
    assert_eq!(json.status.code(), Some(0), "stderr: {}", stderr(&json));
    let mut found = Vec::new();
    strings(&json_of(&json), &mut found);
    for word in [
        "example.plugin.wander",
        "0.1.0",
        "example.behavior.wander",
        "behavior",
    ] {
        assert!(
            found.iter().any(|f| f == word),
            "json lacks {word:?}: {found:?}"
        );
    }
}

/// ASM-114 (REQ-T-3): without `--plugin-path` the example fails validation naming the plugin
/// behavior as an unknown id, exit 1.
#[test]
#[ignore = "WI-13"]
fn validate_example_without_plugin_path_reports_unknown_id() {
    let dir = s(&plugin_support::wander_example_dir());

    let out = cli(&["validate", dir.as_str()]);

    assert_eq!(out.status.code(), Some(1));
    let err = stderr(&out);
    assert!(
        err.contains("unknown_id") && err.contains("example.behavior.wander"),
        "{err}"
    );
}

/// ADR 0021, REQ-P-7: a plugin fault under `abort` is exit 1 with the fault message (plugin,
/// capability and call) on stderr.
#[test]
#[ignore = "WI-12"]
fn run_fault_under_abort_exits_1_with_the_fault_message() {
    let scenario = s(&plugin_support::fault_scenario(
        "cli-abort",
        "abort",
        2.0,
        &["healthy", "direct"],
    ));
    let plugins = s(&FIXTURE);

    let out = cli(&[
        "run",
        scenario.as_str(),
        "--plugin-path",
        plugins.as_str(),
        "--ticks",
        "60",
    ]);

    assert_eq!(out.status.code(), Some(1), "stdout: {}", stdout(&out));
    let err = stderr(&out);
    assert!(
        err.contains("test.plugin.fixture")
            && err.contains("test.behavior.panic_tick")
            && err.contains("tick_batch"),
        "{err}"
    );
}

/// ADR 0021: under `disable_capability` the same run ends normally (exit 0) with its final hash.
#[test]
#[ignore = "WI-12"]
fn run_fault_under_disable_capability_exits_0() {
    let scenario = s(&plugin_support::fault_scenario(
        "cli-disable",
        "disable_capability",
        2.0,
        &["healthy", "direct"],
    ));
    let plugins = s(&FIXTURE);

    let out = cli(&[
        "run",
        scenario.as_str(),
        "--plugin-path",
        plugins.as_str(),
        "--ticks",
        "60",
    ]);

    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    assert!(
        final_hash(&stdout(&out))
            .as_deref()
            .is_some_and(is_16_lower_hex)
    );
}

/// ADR 0022: a `--plugin-path` that does not exist is a load error, exit 1, naming the path.
#[test]
#[ignore = "WI-12"]
fn missing_plugin_dir_is_exit_1_naming_the_path() {
    let scenario = s(&plugin_support::fault_scenario(
        "cli-missing",
        "abort",
        1.0,
        &["healthy"],
    ));

    let out = cli(&[
        "run",
        scenario.as_str(),
        "--plugin-path",
        "/no/such/plugin/dir",
    ]);

    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).contains("/no/such/plugin/dir"),
        "{}",
        stderr(&out)
    );
}

/// ASM-113: runs without plugins print no plugin line and their stdout is unchanged: the M3 example
/// still prints a 16-digit hash and nothing on stderr at info level mentions a plugin.
#[test]
fn run_without_plugins_logs_no_plugin_line() {
    let scenario = s(&Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/scenarios/patrol_and_engage/scenario.toml"));

    let out = cli(&[
        "run",
        scenario.as_str(),
        "--ticks",
        "60",
        "--log-level",
        "info",
    ]);

    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    assert!(
        final_hash(&stdout(&out))
            .as_deref()
            .is_some_and(is_16_lower_hex)
    );
    assert!(
        !stderr(&out).to_lowercase().contains("plugin"),
        "{}",
        stderr(&out)
    );
}
