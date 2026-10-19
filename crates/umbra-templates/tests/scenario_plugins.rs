//! AT-10: the scenario keys `plugin_paths` and `on_plugin_fault`. Requirements: REQ-P-7, REQ-P-10,
//! ADR 0021 (fault policy), ADR 0022 (scenario keys; `plugins` stays an unknown key), ADR 0010 rules
//! (located `wrong_type` / `invalid_value`).
//!
//! Step 4 of M4: the reader does not know the keys before WI-11 (`Scenario::plugin_paths` and
//! `on_plugin_fault` exist as stubs with empty values and `scenario_plugin_paths` is a `todo!()`), so
//! those tests are ignored with that work item. `plugins_key_is_still_unknown` already holds.

#![forbid(unsafe_code)]

mod common;

use std::path::{Path, PathBuf};

use common::{alpha_template, errors, fixture_path, lines, ok, pos_of, row, rows, scenario, src};
use umbra_core::PluginFaultPolicy;
use umbra_templates::{Scenario, SourceFile, scenario_plugin_paths};

const SCN: &str = "scn/scenario.toml";

fn template() -> SourceFile {
    src(
        "scn/templates/veh.toml",
        &alpha_template("veh.base", None, "2.0"),
    )
}

fn base() -> String {
    lines(&[
        "schema_version = 1",
        "",
        "[scenario]",
        "name = \"test\"",
        "seed = 7",
        "tick_rate_hz = 30",
        "clock = \"afap\"",
        "duration_s = 10.0",
        "template_paths = [\"templates\"]",
        "",
        "[[entities]]",
        "template = \"veh.base\"",
        "position = { lat_deg = 52.0, lon_deg = 4.0 }",
        "heading_deg = 90.0",
    ])
}

/// `base()` with `extra` lines added to the `[scenario]` table.
fn with(extra: &str) -> String {
    base().replace("seed = 7", &format!("seed = 7\n{extra}"))
}

fn load(text: &str) -> Result<Scenario, umbra_templates::Diagnostics> {
    scenario(&src(SCN, text), &[template()])
}

fn failing(text: &str) -> Vec<common::Row> {
    rows(&errors(load(text)))
}

#[test]
#[ignore = "WI-11"]
fn plugin_paths_resolve_relative_to_scenario() {
    let s = ok(load(&with("plugin_paths = [\"plugins\", \"../shared\"]")));

    assert_eq!(
        s.plugin_paths,
        [
            Path::new("scn").join("plugins"),
            Path::new("scn").join("../shared")
        ]
    );
}

#[test]
#[ignore = "WI-11"]
fn plugin_paths_default_to_none() {
    let s = ok(load(&base()));

    assert_eq!(s.plugin_paths, Vec::<PathBuf>::new());
}

#[test]
#[ignore = "WI-11"]
fn plugin_paths_wrong_type_located() {
    let text = with("plugin_paths = \"plugins\"");

    assert_eq!(
        failing(&text),
        vec![row("wrong_type", SCN, pos_of(&text, "\"plugins\""))]
    );
}

#[test]
#[ignore = "WI-11"]
fn plugin_paths_entry_of_the_wrong_type_is_located_at_the_entry() {
    let text = with("plugin_paths = [\"plugins\", 3]");

    assert_eq!(
        failing(&text),
        vec![row("wrong_type", SCN, pos_of(&text, "3]"))]
    );
}

#[test]
#[ignore = "WI-11"]
fn on_plugin_fault_values() {
    let abort = ok(load(&with("on_plugin_fault = \"abort\"")));
    let disable = ok(load(&with("on_plugin_fault = \"disable_capability\"")));

    assert_eq!(abort.on_plugin_fault, PluginFaultPolicy::Abort);
    assert_eq!(
        disable.on_plugin_fault,
        PluginFaultPolicy::DisableCapability
    );
}

#[test]
#[ignore = "WI-11"]
fn on_plugin_fault_invalid_value_located_lists_both() {
    let text = with("on_plugin_fault = \"ignore\"");

    let found = errors(load(&text));

    assert_eq!(
        rows(&found),
        vec![row("invalid_value", SCN, pos_of(&text, "\"ignore\""))]
    );
    assert!(
        found[0].message.contains("abort") && found[0].message.contains("disable_capability"),
        "the message lists both values: {}",
        found[0].message
    );
}

#[test]
#[ignore = "WI-11"]
fn on_plugin_fault_wrong_type_is_located() {
    let text = with("on_plugin_fault = 42");

    assert_eq!(
        failing(&text),
        vec![row("wrong_type", SCN, pos_of(&text, "42"))]
    );
}

#[test]
#[ignore = "WI-11"]
fn on_plugin_fault_defaults_to_abort() {
    let s = ok(load(&base()));

    assert_eq!(s.on_plugin_fault, PluginFaultPolicy::Abort);
}

#[test]
#[ignore = "WI-11"]
fn scenario_plugin_paths_is_lenient() {
    let dir = fixture_path("scenarios/plugins/lenient");

    let got = scenario_plugin_paths(&dir.join("scenario.toml"));

    assert_eq!(got, [dir.join("plugins"), dir.join("more/plugins")]);
}

#[test]
#[ignore = "WI-11"]
fn scenario_plugin_paths_of_a_directory_is_the_union_over_its_scenario_files_in_walk_order() {
    let dir = fixture_path("scenarios/plugins/walk");

    let got = scenario_plugin_paths(&dir);

    assert_eq!(
        got,
        [
            dir.join("a").join("plugins_a"),
            dir.join("b").join("plugins_b")
        ]
    );
}

#[test]
#[ignore = "WI-11"]
fn scenario_plugin_paths_of_other_files_and_missing_paths_is_empty() {
    let dir = fixture_path("scenarios/plugins/walk");

    assert_eq!(
        scenario_plugin_paths(&dir.join("templates/t.toml")),
        Vec::<PathBuf>::new()
    );
    assert_eq!(
        scenario_plugin_paths(&dir.join("does-not-exist.toml")),
        Vec::<PathBuf>::new()
    );
}

/// The M2 rule is unchanged: `plugins` (ADR 0010's planned name) stays an unknown key. The M2 test
/// `unknown_key_in_the_scenario_table_is_unknown_key` is not edited; this repeats the rule next to the
/// new keys.
#[test]
fn plugins_key_is_still_unknown() {
    let text = with("plugins = []");

    assert_eq!(
        failing(&text),
        vec![row("unknown_key", SCN, pos_of(&text, "plugins"))]
    );
}
