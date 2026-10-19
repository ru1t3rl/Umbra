//! WI-12: the shape of `umbra list-plugins` (ASM-144) and the stderr plugin line (ASM-113), with the
//! fixture plugin. REQ-X-2.

#![forbid(unsafe_code)]

#[path = "../../umbra-engine/tests/plugin_support/mod.rs"]
mod plugin_support;

mod common;

use common::{cli, json_of, stderr, stdout};

fn fixture_dir() -> String {
    plugin_support::fixture(&[]).to_string_lossy().into_owned()
}

#[test]
fn list_plugins_json_has_the_documented_shape() {
    let dir = fixture_dir();

    let out = cli(&["list-plugins", "--plugin-path", dir.as_str(), "--json"]);

    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    let doc = json_of(&out);
    let plugins = doc["plugins"].as_array().expect("plugins array");
    assert_eq!(plugins.len(), 1);
    let plugin = &plugins[0];
    assert_eq!(plugin["id"], "test.plugin.fixture");
    assert_eq!(plugin["version"], "0.1.0");
    assert_eq!(plugin["abi"], "0.0");
    assert!(plugin["name"].is_string() && plugin["path"].is_string());
    let capabilities = plugin["capabilities"].as_array().expect("capabilities");
    let ok = capabilities
        .iter()
        .find(|c| c["id"] == "test.behavior.ok")
        .expect("test.behavior.ok is listed");
    assert_eq!(ok["kind"], "behavior");
    assert_eq!(ok["version"], "0.1.0");
}

#[test]
fn list_plugins_text_ends_with_a_count_and_no_plugins_is_zero() {
    let dir = fixture_dir();

    let some = cli(&["list-plugins", "--plugin-path", dir.as_str()]);
    let none = cli(&["list-plugins"]);

    let text = stdout(&some);
    assert!(
        text.starts_with("test.plugin.fixture 0.1.0 abi=0.0 path="),
        "{text}"
    );
    assert!(
        text.contains("\n  behavior test.behavior.ok 0.1.0\n"),
        "{text}"
    );
    assert_eq!(text.lines().last(), Some("1 plugin(s)"));
    assert_eq!(stdout(&none), "0 plugin(s)\n");
}

#[test]
fn list_plugins_reads_the_plugin_paths_of_a_scenario() {
    let dir = plugin_support::fixture(&[]);
    let scenario = plugin_support::write_scenario(
        "list-scenario",
        1.0,
        &format!("plugin_paths = [{:?}]", dir.display().to_string()),
        &[],
        &[],
        &[],
    );
    let scenario = scenario.to_string_lossy().into_owned();

    let out = cli(&["list-plugins", scenario.as_str()]);

    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    assert!(
        stdout(&out).contains("test.plugin.fixture"),
        "{}",
        stdout(&out)
    );
}

#[test]
fn the_plugin_line_is_on_stderr_at_info_and_absent_at_the_default_level() {
    let dir = fixture_dir();
    let scenario = plugin_support::fault_scenario("line", "abort", 1.0, &["healthy"])
        .to_string_lossy()
        .into_owned();
    let run = |level: &str| {
        cli(&[
            "run",
            scenario.as_str(),
            "--plugin-path",
            dir.as_str(),
            "--ticks",
            "3",
            "--log-level",
            level,
        ])
    };

    let info = run("info");
    let warn = run("warn");

    assert!(stderr(&info).contains("loaded plugin test.plugin.fixture 0.1.0 (abi 0.0)"));
    assert!(!stderr(&warn).contains("loaded plugin"));
    assert_eq!(
        info.stdout, warn.stdout,
        "stdout does not depend on the log level"
    );
    assert!(!stdout(&info).contains("plugin"));
}
