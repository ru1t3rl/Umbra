//! AT-6 (M7, process rows): the combined example `examples/scenarios/plugin_kinds` with the five
//! example plugins (`hover`, `radar`, `bearing`, `dispatch`, `net-jsonl`) and the built-in air and sea
//! models, as real `umbra` processes. Requirements: Acceptance 1 (a third-party-style example of each
//! kind), REQ-K-4 (cross-process determinism), REQ-P-4, REQ-X-2, REQ-T-6, ADR 0050, ADR 0056, ASM-649.
//!
//! The plugin libraries are built by the nested-cargo helper shared with the engine tests
//! (`plugin_support::example_plugins`, `CARGO_TARGET_TMPDIR`), never read from `target/plugins`.
//! Contract with the example (WI-21): `scenario.toml` at 30 Hz, at least 900 ticks long, uses all five
//! plugins and `builtin.air.kinematic` and `builtin.sea.kinematic`, takes its plugins from
//! `--plugin-path`, and runs without a network peer (the `net-jsonl` adapter records to a file named by
//! the scenario relative to its own directory).
//!
//! The dependency rule of the example crates (`umbra-sdk` only, `#![forbid(unsafe_code)]`) is checked
//! in `xtask/tests/check_rules.rs`.
//!
//! Step 4 of M7 (WI-2): ignored until WI-21 (the example scenario and, behind it, WI-6 and
//! WI-17..19).

#![forbid(unsafe_code)]

#[path = "../../umbra-engine/tests/plugin_support/mod.rs"]
mod plugin_support;

mod common;

use std::path::{Path, PathBuf};
use std::process::Output;
use std::sync::LazyLock;

use common::{cli, final_hash, is_16_lower_hex, json_of, stderr, stdout};

static PLUGINS: LazyLock<PathBuf> = LazyLock::new(|| {
    plugin_support::example_plugins(&["hover", "radar", "bearing", "dispatch", "net-jsonl"])
});

fn s(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

fn example_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/scenarios/plugin_kinds")
}

fn run_example(extra: &[&str]) -> Output {
    let scenario = s(&example_dir().join("scenario.toml"));
    let plugins = s(&PLUGINS);
    let mut args = vec!["run", scenario.as_str(), "--plugin-path", plugins.as_str()];
    args.extend_from_slice(extra);
    cli(&args)
}

/// REQ-T-6: the example directory validates with the plugins, and `run` ends with a final hash and
/// the plugin line of every plugin on stderr.
#[test]
#[ignore = "M7 WI-21"]
fn plugin_kinds_validates_and_runs() {
    let (dir, plugins) = (s(&example_dir()), s(&PLUGINS));

    let validated = cli(&["validate", dir.as_str(), "--plugin-path", plugins.as_str()]);
    let ran = run_example(&["--ticks", "300", "--log-level", "info"]);

    assert_eq!(
        validated.status.code(),
        Some(0),
        "stdout: {}, stderr: {}",
        stdout(&validated),
        stderr(&validated)
    );
    let last = stdout(&validated);
    let last = last.lines().last().unwrap_or_default();
    assert!(
        last.starts_with("ok: ") && last.ends_with(" files checked"),
        "{last}"
    );
    assert_eq!(ran.status.code(), Some(0), "stderr: {}", stderr(&ran));
    assert!(
        final_hash(&stdout(&ran))
            .as_deref()
            .is_some_and(is_16_lower_hex),
        "{}",
        stdout(&ran)
    );
    let err = stderr(&ran);
    for plugin in [
        "example.plugin.hover",
        "example.plugin.radar",
        "example.plugin.bearing",
        "example.plugin.dispatch",
        "example.plugin.net_jsonl",
    ] {
        assert!(err.contains(plugin), "no plugin line for {plugin}: {err}");
    }
}

/// ASM-114 (REQ-T-3): without the plugins the example fails validation with unknown ids, exit 1.
#[test]
#[ignore = "M7 WI-21"]
fn plugin_kinds_without_the_plugins_reports_unknown_ids() {
    let dir = s(&example_dir());

    let out = cli(&["validate", dir.as_str()]);

    assert_eq!(out.status.code(), Some(1));
    let text = stdout(&out);
    assert!(
        text.contains("unknown_id") && text.contains("example.motion.hover"),
        "{text}"
    );
}

/// REQ-K-4, AT-22: four separate processes print byte-identical output, with every entity state line
/// and the state hash after every tick, 900 ticks. Separate processes have different hasher seeds and
/// address-space layouts and load the five libraries afresh each time.
#[test]
#[ignore = "M7 WI-21"]
fn four_processes_byte_identical() {
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
    let hash_lines = stdout(&runs[0])
        .lines()
        .filter(|l| l.contains("\"state_hash\""))
        .count();
    assert_eq!(hash_lines, 901, "one hash per tick plus the final line");
    for r in &runs[1..] {
        assert_eq!(r.stdout, runs[0].stdout);
    }
}

/// REQ-X-2, Acceptance 1: `list-plugins` shows a plugin of every new kind.
#[test]
#[ignore = "M7 WI-21"]
fn list_plugins_shows_every_kind() {
    let plugins = s(&PLUGINS);

    let json = cli(&["list-plugins", "--plugin-path", plugins.as_str(), "--json"]);

    assert_eq!(json.status.code(), Some(0), "stderr: {}", stderr(&json));
    let doc = json_of(&json);
    let capabilities: Vec<(String, String)> = doc["plugins"]
        .as_array()
        .expect("plugins array")
        .iter()
        .flat_map(|p| p["capabilities"].as_array().expect("capabilities").iter())
        .map(|c| {
            (
                c["kind"].as_str().expect("kind").to_owned(),
                c["id"].as_str().expect("id").to_owned(),
            )
        })
        .collect();
    let mut want = vec![
        ("motion model".to_owned(), "example.motion.hover".to_owned()),
        ("sensor".to_owned(), "example.sensor.radar".to_owned()),
        (
            "condition".to_owned(),
            "example.cond.bearing_within".to_owned(),
        ),
        ("action".to_owned(), "example.action.dispatch".to_owned()),
        ("network adapter".to_owned(), "example.net.jsonl".to_owned()),
    ];
    want.sort();
    let mut got = capabilities;
    got.sort();
    assert_eq!(got, want);
}
