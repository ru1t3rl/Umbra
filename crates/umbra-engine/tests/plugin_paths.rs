//! WI-12: how the plugin directories of a run or a validation are put together (ADR 0022, ASM-144):
//! CLI paths first, duplicates dropped by canonical path, the union over the scenario files of a
//! directory, and a conflict (the same plugin ID from two directories) as a load error.
//! Requirements: REQ-P-10, REQ-T-6.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "the helper builds known-valid fixtures")]

mod behavior_support;
mod common;
mod plugin_support;

use std::path::{Path, PathBuf};

use behavior_support::standard;
use plugin_support::{Tpl, write_scenario};
use umbra_engine::scenario::ScenarioError;
use umbra_engine::{PluginError, resolve_plugin_paths, validate_path_with_plugins};

/// An existing plugin directory without packages.
fn empty_dir() -> PathBuf {
    let dir = plugin_support::plugins_dir(&[]);
    std::fs::create_dir_all(&dir).expect("create the directory");
    dir
}

fn scenario_naming(name: &str, plugin_paths: &[&Path]) -> PathBuf {
    let list: Vec<String> = plugin_paths
        .iter()
        .map(|p| format!("{:?}", p.display().to_string()))
        .collect();
    write_scenario(
        name,
        1.0,
        &format!("plugin_paths = [{}]", list.join(", ")),
        &[Tpl {
            id: "test.entity.one",
            behavior: "test.behavior.ok",
            params: "",
        }],
        &[],
        &[("test.entity.one", "one")],
    )
}

#[test]
fn cli_paths_come_first_and_duplicates_are_dropped_by_canonical_path() {
    let a = plugin_support::fixture(&[]);
    let b = empty_dir();
    let scenario = scenario_naming("paths-dedupe", &[&b, &a.join("."), &a]);

    let resolved = resolve_plugin_paths(std::slice::from_ref(&a), Some(&scenario));

    assert_eq!(resolved, [a, b]);
}

#[test]
fn a_directory_validates_with_the_union_of_its_scenarios_plugin_paths() {
    let with_plugin = plugin_support::fixture(&[]);
    let empty = empty_dir();
    // `second.toml` sorts after `scenario.toml`: the plugin comes from the later file.
    let first = scenario_naming("union-a", &[&empty]);
    let second = scenario_naming("union-b", &[&with_plugin]);
    // Both scenarios in one directory: the second is copied next to the first.
    let dir = first.parent().expect("directory").to_path_buf();
    std::fs::copy(&second, dir.join("second.toml")).expect("copy the second scenario");

    let report = validate_path_with_plugins(&standard(), &dir, &[]).expect("validate runs");

    assert!(
        report.diagnostics.is_empty(),
        "{:?}",
        report.diagnostics.iter().collect::<Vec<_>>()
    );
}

#[test]
fn the_same_plugin_id_from_two_directories_is_a_load_error() {
    let a = plugin_support::fixture(&[]);
    let b = plugin_support::fixture(&[]);
    let scenario = scenario_naming("conflict", &[&b]);

    let result = validate_path_with_plugins(&standard(), &scenario, std::slice::from_ref(&a));

    assert!(
        matches!(
            result,
            Err(ScenarioError::Plugin(PluginError::Duplicate { .. }))
        ),
        "{:?}",
        result.map(|r| r.diagnostics.len())
    );
}
