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

/// ASM-074: a `*.behavior.toml` file with a `[scenario]` table is a graph: `load_scenario` and
/// `validate_path` agree that it is not a scenario, and in a directory it is read as a graph
/// (so its `[scenario]` table is an unknown key of the graph).
#[test]
fn graph_with_a_scenario_table_is_still_a_graph() {
    let dir = fixture_path("graphs/scenario_table_graph");
    let file = dir.join("x.behavior.toml");
    let registry = registry();
    let loader = loader_for(&registry);

    let as_scenario = common::errors(loader.load_scenario(&file));
    let single = loader.validate_path(&file);
    let directory = loader.validate_path(&dir);

    assert_eq!(as_scenario.len(), 1);
    assert_eq!(as_scenario[0].code.as_str(), "not_a_scenario");
    assert!(
        as_scenario[0].message.contains("behavior graph"),
        "{}",
        as_scenario[0].message
    );
    // validate_path reads the file as a graph: the `[scenario]` table is an unknown key.
    assert!(
        single
            .diagnostics
            .iter()
            .any(|d| d.code.as_str() == "unknown_key")
    );
    assert!(
        directory
            .diagnostics
            .iter()
            .any(|d| d.code.as_str() == "unknown_key")
    );
    assert!(
        directory
            .diagnostics
            .iter()
            .all(|d| d.code.as_str() != "not_a_scenario")
    );
}

const SIBLINGS: &str = "graphs/sibling_graphs";

fn sibling(name: &str) -> std::path::PathBuf {
    fixture_path(&format!("{SIBLINGS}/{name}.behavior.toml"))
}

/// ASM-074, WI-12 review m3: a single graph file is validated together with its direct siblings, so
/// a `duplicate_id` between two graph files is reported whichever one is given (located in the
/// later file, with a note at the first).
#[test]
fn validate_path_on_a_graph_file_reports_a_duplicate_id_with_a_sibling() {
    let registry = registry();
    let loader = loader_for(&registry);

    let from_a = loader.validate_path(&sibling("a"));
    let from_b = loader.validate_path(&sibling("b"));

    for report in [&from_a, &from_b] {
        assert_eq!(
            rows(&report.diagnostics),
            [row("duplicate_id", sibling("b"), (2, 6))]
        );
    }
    assert_eq!(from_a.files_checked, [sibling("a")]);
    assert_eq!(from_b.files_checked, [sibling("b")]);
}

/// ASM-074: a graph that calls a sibling is checked against it (no `unknown_id`, and a version
/// mismatch is found); a report holds the diagnostics located in the given file or with a note
/// pointing into it.
#[test]
fn validate_path_on_a_graph_file_resolves_calls_to_siblings() {
    let registry = registry();
    let loader = loader_for(&registry);

    let calling = loader.validate_path(&sibling("caller"));
    let called = loader.validate_path(&sibling("callee"));

    assert_eq!(
        rows(&calling.diagnostics),
        [row("version_mismatch", sibling("caller"), (9, 11))]
    );
    assert_eq!(calling.diagnostics[0].notes.len(), 1);
    // The note points into the callee, so its report includes the diagnostic too (ASM-067).
    assert_eq!(called.diagnostics, calling.diagnostics);
}
