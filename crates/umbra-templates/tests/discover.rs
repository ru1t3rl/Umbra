//! AT-6 (library part) and AT-9 (`discovery_returns_sorted_paths`): `Loader::validate_path` and
//! file discovery. Requirements: REQ-T-6 (groundwork), REQ-T-2, REQ-K-4, ASM-053, ASM-058.
//!
//! Fixtures under `tests/fixtures/dirs/` and `tests/fixtures/scenarios/`; the standard stub
//! registry is used.

#![forbid(unsafe_code)]

mod common;

use std::path::PathBuf;

use common::{Counters, errors, fixture_path, loader, ok, registry, row, rows};

fn files(report: &umbra_templates::ValidationReport) -> Vec<PathBuf> {
    report.files_checked.clone()
}

fn validate(rel: &str) -> (umbra_templates::ValidationReport, Counters) {
    let (registry, counters) = registry();
    let report = loader(&registry).validate_path(&fixture_path(rel));
    (report, counters)
}

#[test]
fn validate_path_on_scenario_checks_scenario_and_its_templates() {
    let (report, _) = validate("scenarios/relative/scenario.toml");

    assert_eq!(rows(&report.diagnostics), []);
    assert_eq!(
        files(&report),
        [
            fixture_path("scenarios/relative/scenario.toml"),
            fixture_path("scenarios/relative/templates/sub/deeper.toml"),
            fixture_path("scenarios/relative/templates/veh.toml"),
        ]
    );
}

#[test]
fn validate_path_on_a_template_file_checks_that_file() {
    let (report, _) = validate("scenarios/relative/templates/veh.toml");

    assert_eq!(rows(&report.diagnostics), []);
    assert_eq!(
        files(&report),
        [fixture_path("scenarios/relative/templates/veh.toml")]
    );
}

#[test]
fn validate_path_on_directory_checks_every_toml_recursively_in_sorted_order() {
    let (report, _) = validate("dirs/mixed");

    assert_eq!(
        files(&report),
        [
            fixture_path("dirs/mixed/m_mid.toml"),
            fixture_path("dirs/mixed/sub/a_good.toml"),
            fixture_path("dirs/mixed/sub/b_bad.toml"),
            fixture_path("dirs/mixed/sub/deeper/c_good.toml"),
            fixture_path("dirs/mixed/z_last.toml"),
        ]
    );
}

#[test]
fn validate_path_on_directory_reports_the_errors_of_the_bad_file_only() {
    let (report, _) = validate("dirs/mixed");

    assert_eq!(
        rows(&report.diagnostics),
        [row(
            "unknown_key",
            fixture_path("dirs/mixed/sub/b_bad.toml"),
            (5, 1)
        )]
    );
}

/// ASM-053: `.toml` files only; the neighbouring `notes.txt` is not read.
#[test]
fn discovery_ignores_files_without_the_toml_extension() {
    let (report, _) = validate("dirs/mixed");

    assert!(
        report
            .files_checked
            .iter()
            .all(|p| p.extension().is_some_and(|e| e == "toml"))
    );
}

/// REQ-K-4: the order is by path, independent of the file system's directory order.
#[test]
fn discovery_returns_sorted_paths() {
    let (report, _) = validate("dirs/mixed");

    let mut sorted = report.files_checked.clone();
    sorted.sort();
    assert_eq!(report.files_checked, sorted);
}

#[test]
fn validating_twice_gives_identical_reports() {
    let (a, _) = validate("dirs/mixed");
    let (b, _) = validate("dirs/mixed");

    assert_eq!(a, b);
}

#[test]
fn missing_path_is_io_diagnostic() {
    let (report, _) = validate("dirs/no_such_path");

    assert_eq!(report.diagnostics.len(), 1);
    assert_eq!(report.diagnostics[0].code.as_str(), "io");
    assert!(
        report.diagnostics[0].message.contains("no_such_path"),
        "{}",
        report.diagnostics[0].message
    );
}

#[test]
fn syntax_errors_in_a_validated_file_are_reported_with_positions() {
    let (report, _) = validate("syntax/inline_two_bad_values.toml");

    assert_eq!(
        rows(&report.diagnostics),
        [
            row(
                "syntax",
                fixture_path("syntax/inline_two_bad_values.toml"),
                (9, 16)
            ),
            row(
                "syntax",
                fixture_path("syntax/inline_two_bad_values.toml"),
                (9, 23)
            ),
        ]
    );
}

/// Validation and loading share one pipeline (ASM-058): `validate_path` dry-runs `Factory::build`
/// exactly as often as `load_scenario` does, and builds nothing more (no engine, no spawning).
#[test]
fn validate_path_builds_exactly_as_often_as_load_scenario() {
    let path = fixture_path("scenarios/relative/scenario.toml");
    let (registry_a, counters_a) = registry();
    let (registry_b, counters_b) = registry();

    let loaded = loader(&registry_a).load_scenario(&path);
    let report = loader(&registry_b).validate_path(&path);

    assert!(loaded.is_ok());
    assert_eq!(rows(&report.diagnostics), []);
    assert!(
        counters_b.motion() >= 1,
        "validation must dry-run the factories"
    );
    assert_eq!(counters_b.motion(), counters_a.motion());
    assert_eq!(counters_b.terrain(), counters_a.terrain());
}

#[test]
fn load_scenario_on_a_template_file_is_not_a_scenario() {
    let (registry, _) = registry();

    let found = errors(
        loader(&registry).load_scenario(&fixture_path("scenarios/relative/templates/veh.toml")),
    );

    assert_eq!(found.len(), 1);
    assert_eq!(found[0].code.as_str(), "not_a_scenario");
}

#[test]
fn load_scenario_on_a_missing_file_is_an_io_diagnostic() {
    let (registry, _) = registry();

    let found = errors(loader(&registry).load_scenario(&fixture_path("scenarios/none.toml")));

    assert_eq!(found.len(), 1);
    assert_eq!(found[0].code.as_str(), "io");
}

#[test]
fn load_scenario_twice_gives_equal_scenarios() {
    let (registry, _) = registry();
    let path = fixture_path("scenarios/relative/scenario.toml");

    let first = ok(loader(&registry).load_scenario(&path));
    let second = ok(loader(&registry).load_scenario(&path));

    assert_eq!(first, second);
}
