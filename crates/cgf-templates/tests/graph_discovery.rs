//! WI-12 discovery: `*.behavior.toml` files are classified as graphs next to templates and a
//! scenario (ASM-074), through the path-based entry points. Requirements: REQ-B-6, REQ-T-2,
//! ASM-074.
//!
//! Fixtures under `tests/fixtures/graphs/discovery_mixed/`.

#![forbid(unsafe_code)]

mod common;
mod graph_support;

use common::{fixture_path, row, rows};
use graph_support::{loader_for, registry};

const DIR: &str = "graphs/discovery_mixed";

fn bad() -> std::path::PathBuf {
    fixture_path(&format!("{DIR}/templates/bad.behavior.toml"))
}

/// ASM-074: a directory with a scenario, a template and two graphs: graphs are not read as
/// templates (their `root`, `nodes` keys are not `unknown_key` there), and only the bad graph has
/// a diagnostic. Files come out sorted.
#[test]
fn directory_with_scenario_templates_and_graphs_is_classified() {
    let registry = registry();

    let report = loader_for(&registry).validate_path(&fixture_path(DIR));

    assert_eq!(
        rows(&report.diagnostics),
        [row("unknown_key", bad(), (7, 1))]
    );
    assert_eq!(
        report.files_checked,
        [
            fixture_path(&format!("{DIR}/scenario.toml")),
            fixture_path(&format!("{DIR}/templates/bad.behavior.toml")),
            fixture_path(&format!("{DIR}/templates/good.behavior.toml")),
            fixture_path(&format!("{DIR}/templates/veh.toml")),
        ]
    );
}

/// ASM-074: the scenario load reads the graph files under `template_paths` and reports their
/// errors with the rest.
#[test]
fn scenario_load_reports_graph_file_errors() {
    let registry = registry();

    let found = loader_for(&registry).load_scenario(&fixture_path(&format!("{DIR}/scenario.toml")));

    let diagnostics = common::errors(found);
    assert_eq!(rows(&diagnostics), [row("unknown_key", bad(), (7, 1))]);
}

/// ASM-074: a single graph file is validated as a graph.
#[test]
fn validate_path_on_a_graph_file_checks_that_file() {
    let registry = registry();
    let loader = loader_for(&registry);
    let good = fixture_path(&format!("{DIR}/templates/good.behavior.toml"));

    let ok = loader.validate_path(&good);
    let broken = loader.validate_path(&bad());

    assert_eq!(rows(&ok.diagnostics), []);
    assert_eq!(ok.files_checked, [good]);
    assert_eq!(
        rows(&broken.diagnostics),
        [row("unknown_key", bad(), (7, 1))]
    );
}

/// REQ-K-4: validating twice gives the same report.
#[test]
fn validating_graphs_twice_gives_identical_reports() {
    let registry = registry();
    let loader = loader_for(&registry);
    let dir = fixture_path(DIR);

    assert_eq!(loader.validate_path(&dir), loader.validate_path(&dir));
}
