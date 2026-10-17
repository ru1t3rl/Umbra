//! Review fixes for WI-8 (REQ-T-2: no error may be lost; ASM-053, ASM-066, ASM-067): override
//! locations, unreadable template paths, duration guard, discovery details, directory validation.

#![forbid(unsafe_code)]

mod common;

use cgf_templates::SourceFile;
use common::{
    errors, fixture_path, lines, loader, ok, pos_of, pos_of_nth, registry, row, rows, scenario, src,
};

const SCN: &str = "scn/scenario.toml";

fn gamma_template() -> SourceFile {
    src(
        "scn/templates/veh.toml",
        &lines(&[
            "schema_version = 1",
            "id = \"veh.g\"",
            "[motion]",
            "model = \"stub.motion.gamma\"",
            "params = { x = 1.0 }",
        ]),
    )
}

fn head(extra: &[&str]) -> String {
    let mut l = vec![
        "schema_version = 1",
        "[scenario]",
        "name = \"t\"",
        "seed = 1",
        "tick_rate_hz = 30",
        "duration_s = 10.0",
    ];
    l.extend_from_slice(extra);
    lines(&l)
}

fn entity(extra: &[&str]) -> String {
    let mut l = vec![
        "[[entities]]",
        "template = \"veh.g\"",
        "position = { lat_deg = 52.0, lon_deg = 4.0 }",
    ];
    l.extend_from_slice(extra);
    lines(&l)
}

/// Item 7: an override that switches model reports its errors in the scenario, once per entity.
#[test]
fn override_switching_model_locates_missing_params_in_the_scenario() {
    let switch = [
        "[entities.overrides.motion]",
        "model = \"stub.motion.alpha\"",
    ];
    let text = head(&[]) + &entity(&switch) + &entity(&switch);

    let found = errors(scenario(&src(SCN, &text), &[gamma_template()]));

    assert_eq!(
        rows(&found),
        [
            row(
                "missing_param",
                SCN,
                pos_of_nth(&text, "[entities.overrides.motion]", 0)
            ),
            row(
                "missing_param",
                SCN,
                pos_of_nth(&text, "[entities.overrides.motion]", 1)
            ),
        ]
    );
}

/// Item 16: an empty `[entities.overrides]` changes nothing and builds nothing more.
#[test]
fn empty_overrides_do_not_trigger_a_second_build() {
    let (registry_a, counters_a) = registry();
    let (registry_b, counters_b) = registry();
    let plain = head(&[]) + &entity(&[]);
    let empty = head(&[]) + &entity(&["[entities.overrides]"]);

    let a = loader(&registry_a).scenario_from_sources(&src(SCN, &plain), &[gamma_template()]);
    let b = loader(&registry_b).scenario_from_sources(&src(SCN, &empty), &[gamma_template()]);

    assert!(a.is_ok() && b.is_ok());
    assert_eq!(counters_a.motion(), counters_b.motion());
}

/// Item 8 (P6): an unreadable template path does not hide `unknown_template`; it explains it.
#[test]
fn missing_template_directory_still_reports_unknown_template_with_a_note() {
    let (registry, _) = registry();
    let path = fixture_path("scenarios/missing_template_dir/scenario.toml");
    let text = common::read_fixture("scenarios/missing_template_dir/scenario.toml");

    let found = errors(loader(&registry).load_scenario(&path));

    assert_eq!(found.len(), 2, "{found:?}");
    assert_eq!(found[0].code.as_str(), "io");
    assert_eq!(found[1].code.as_str(), "unknown_template");
    assert_eq!(
        rows(&found[1..]),
        [row("unknown_template", &path, pos_of(&text, "\"rel.veh\""))]
    );
    assert!(
        found[1].notes[0].message.contains("could not be read"),
        "{:?}",
        found[1].notes
    );
    assert!(
        found[1].notes[0].message.contains("no_such_dir"),
        "{:?}",
        found[1].notes
    );
}

#[test]
fn a_template_file_that_is_not_utf8_is_an_io_diagnostic_and_explains_the_unknown_template() {
    let (registry, _) = registry();

    let found =
        errors(loader(&registry).load_scenario(&fixture_path("scenarios/bad_utf8/scenario.toml")));

    let codes: Vec<&str> = found.iter().map(|d| d.code.as_str()).collect();
    assert_eq!(codes, ["io", "unknown_template"]);
    assert!(
        found[1].notes[0].message.contains("bad.toml"),
        "{:?}",
        found[1].notes
    );
}

/// Item 9: template files with syntax errors and no recoverable id explain an unknown template.
#[test]
fn scenario_unknown_template_notes_files_with_syntax_errors() {
    let text = head(&[]) + &entity(&[]);
    let broken = src("scn/templates/broken.toml", "schema_version = 1\nid = $\n");

    let found = errors(scenario(&src(SCN, &text), &[broken]));

    let unknown = found.iter().find(|d| d.code.as_str() == "unknown_template");
    let notes = unknown.map(|d| {
        d.notes
            .iter()
            .map(|n| n.message.clone())
            .collect::<Vec<_>>()
    });
    assert!(
        notes
            .as_ref()
            .is_some_and(|n| n.len() == 1 && n[0].contains("syntax errors")),
        "{notes:?}"
    );
}

/// Item 10 (P21): a missing `[scenario]` table does not hide the other errors.
#[test]
fn missing_scenario_table_does_not_hide_entity_and_terrain_errors() {
    let text = lines(&[
        "schema_version = 1",
        "[terrain]",
        "provider = \"builtin.terrain.nope\"",
        "[[entities]]",
        "template = \"veh.nope\"",
        "position = { lat_deg = 52.0, lon_deg = 4.0 }",
    ]);

    let found = errors(scenario(&src(SCN, &text), &[gamma_template()]));

    let codes: Vec<&str> = found.iter().map(|d| d.code.as_str()).collect();
    assert_eq!(codes, ["missing_key", "unknown_id", "unknown_template"]);
}

/// Item 11: the duration guard.
fn ticks(duration: &str, hz: u32) -> Result<u64, Vec<common::Row>> {
    let text = head(&[])
        .replace("duration_s = 10.0", &format!("duration_s = {duration}"))
        .replace("tick_rate_hz = 30", &format!("tick_rate_hz = {hz}"))
        + &entity(&[]);
    scenario(&src(SCN, &text), &[gamma_template()])
        .map(|s| s.duration_ticks)
        .map_err(|d| rows(&d.iter().cloned().collect::<Vec<_>>()))
}

#[test]
fn a_float_artefact_of_a_whole_tick_count_is_not_rounded_up() {
    assert_eq!(ticks("0.1", 30), Ok(3));
    assert_eq!(ticks("1.1", 10), Ok(11));
}

#[test]
fn a_tiny_positive_duration_is_one_tick_not_zero() {
    assert_eq!(ticks("1e-12", 30), Ok(1));
    assert_eq!(ticks("1e-300", 1), Ok(1));
}

#[test]
fn a_huge_product_is_ceiled_not_rounded() {
    // The product is exact here (3 ULPs above 1e15): a partial tick is a whole extra tick.
    assert_eq!(ticks("1000000000000000.375", 1), Ok(1_000_000_000_000_001));
}

#[test]
fn a_product_at_or_above_two_to_the_64_does_not_fit() {
    let found = ticks("18446744073709551616.0", 1);

    assert!(found.is_err_and(|r| r[0].0 == "invalid_value"));
    assert!(ticks("18446744073709549568.0", 1).is_ok());
}

// --- discovery -----------------------------------------------------------------------------------

/// Item 13: one directory reached through two spellings is read once, not a duplicate of itself.
#[test]
fn the_same_template_directory_through_two_spellings_is_loaded_once() {
    let (registry, _) = registry();

    let loaded =
        ok(loader(&registry).load_scenario(&fixture_path("scenarios/spelling/scenario.toml")));

    assert_eq!(loaded.templates.len(), 2);
}

/// Item 14: validating a directory builds each template once and reads each file once.
#[test]
fn validating_a_directory_with_a_scenario_builds_each_template_once() {
    let (registry, counters) = registry();

    let report = loader(&registry).validate_path(&fixture_path("dirs/with_scenario"));

    assert_eq!(rows(&report.diagnostics), []);
    assert_eq!(counters.motion(), 1);
    assert_eq!(counters.terrain(), 1);
    assert_eq!(report.files_checked.len(), 2);
}

/// Item 15 (P19): a child template validated on its own finds its parent next to it.
#[test]
fn validating_a_single_child_template_finds_its_parent_in_the_same_directory() {
    let (registry, _) = registry();

    let report = loader(&registry).validate_path(&fixture_path("dirs/child_in_dir/child.toml"));

    assert_eq!(rows(&report.diagnostics), []);
    assert_eq!(
        report.files_checked,
        [fixture_path("dirs/child_in_dir/child.toml")]
    );
}

#[test]
fn validating_a_single_template_does_not_report_errors_of_its_siblings() {
    let (registry, _) = registry();

    let report =
        loader(&registry).validate_path(&fixture_path("dirs/child_with_bad_sibling/child.toml"));

    assert_eq!(rows(&report.diagnostics), []);
}

#[test]
fn validating_the_bad_sibling_itself_reports_its_error() {
    let (registry, _) = registry();
    let path = fixture_path("dirs/child_with_bad_sibling/other_bad.toml");

    let report = loader(&registry).validate_path(&path);

    assert_eq!(
        rows(&report.diagnostics),
        [row("unknown_key", &path, (5, 1))]
    );
}

#[cfg(unix)]
mod unix {
    use std::fs;
    use std::os::unix::fs::symlink;
    use std::path::PathBuf;

    use super::{loader, registry, rows};

    fn scratch(name: &str) -> PathBuf {
        let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("{e}"));
        dir
    }

    const GOOD: &str = "schema_version = 1\nid = \"s.one\"\n[motion]\nmodel = \"stub.motion.alpha\"\nparams = { a = 1.0 }\n";

    /// Item 12: a symlink to a regular `.toml` file counts; a symlink to a directory is not followed.
    #[test]
    fn symlinked_toml_files_are_checked_but_directory_symlinks_are_not_followed() {
        let dir = scratch("symlinks");
        let real = dir.join("real");
        fs::create_dir_all(&real).unwrap_or_else(|e| panic!("{e}"));
        fs::write(real.join("a.toml"), GOOD).unwrap_or_else(|e| panic!("{e}"));
        let tree = dir.join("tree");
        fs::create_dir_all(&tree).unwrap_or_else(|e| panic!("{e}"));
        symlink(real.join("a.toml"), tree.join("link.toml")).unwrap_or_else(|e| panic!("{e}"));
        symlink(&real, tree.join("dirlink")).unwrap_or_else(|e| panic!("{e}"));
        let (registry, _) = registry();

        let report = loader(&registry).validate_path(&tree);

        assert_eq!(rows(&report.diagnostics), []);
        assert_eq!(report.files_checked, [tree.join("link.toml")]);
    }

    /// Item 12: reaching the depth limit is reported, not silent.
    #[test]
    fn a_directory_nested_too_deeply_is_reported() {
        let mut deep = scratch("deep");
        let top = deep.clone();
        for _ in 0..70 {
            deep.push("d");
        }
        fs::create_dir_all(&deep).unwrap_or_else(|e| panic!("{e}"));
        let (registry, _) = registry();

        let report = loader(&registry).validate_path(&top);

        assert_eq!(report.diagnostics.len(), 1);
        assert_eq!(report.diagnostics[0].code.as_str(), "io");
        assert!(
            report.diagnostics[0].message.contains("nested deeper"),
            "{}",
            report.diagnostics[0].message
        );
    }
}
