//! AT-6 (CLI part): `cgf-cli validate <path>... [--json]`, run as a real process.
//! Requirements: REQ-T-6, REQ-T-2, REQ-X-2 (partial), ASM-062.
//!
//! Exit codes: 0 valid, 1 any diagnostic (including `io` for a missing path), 2 usage (clap).
//! Text diagnostics and the summary line go to stdout; `--json` prints one object. Fixtures in
//! `tests/fixtures/bad/` use the built-in component IDs; the process runs in the crate directory,
//! so the relative fixture paths below appear unchanged in the output.

#![forbid(unsafe_code)]

mod common;

use common::{assert_usage_error, cli, example, example_dir, json_of, stderr, stdout};

const BAD: &str = "tests/fixtures/bad";

/// `(path, line, column, code)` of every diagnostic of the bad fixture directory, in output order
/// (path, then position). The cycle is reported once, at `a_cycle.toml`.
fn expected_bad() -> Vec<(String, u32, u32, &'static str)> {
    let at = |file: &str| format!("{BAD}/{file}");
    vec![
        (at("a_cycle.toml"), 5, 11, "inheritance_cycle"),
        (at("c_unknown_parent.toml"), 5, 11, "unknown_template"),
        (at("d_params.toml"), 7, 10, "missing_param"),
        (at("d_params.toml"), 7, 28, "param_wrong_type"),
        (at("e_unknown_model.toml"), 6, 9, "unknown_id"),
        (at("f_newer.toml"), 3, 18, "unsupported_schema_version"),
        (at("g_syntax.toml"), 7, 16, "syntax"),
    ]
}

/// The `path:line:col: error[code]` heads of the diagnostic lines of a text report.
fn heads(text: &str) -> Vec<String> {
    text.lines()
        .filter(|l| !l.starts_with(' ') && l.contains(": error["))
        .filter_map(|l| l.split("]: ").next().map(|h| format!("{h}]")))
        .collect()
}

#[test]
fn given_example_scenario_then_exit_0_and_ok_line() {
    let out = cli(&["validate", &example()]);

    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    assert_eq!(stdout(&out).lines().last(), Some("ok: 4 files checked"));
}

#[test]
fn given_valid_example_then_stderr_is_empty() {
    let out = cli(&["validate", &example()]);

    assert_eq!(stderr(&out), "");
}

#[test]
fn given_template_file_then_exit_0_and_ok_line() {
    let file = example_dir().join("templates/ground_vehicle_base.toml");

    let out = cli(&["validate", &file.to_string_lossy()]);

    assert_eq!(out.status.code(), Some(0), "stdout: {}", stdout(&out));
    assert_eq!(stdout(&out).lines().last(), Some("ok: 1 files checked"));
}

#[test]
fn given_example_directory_then_every_toml_is_checked() {
    let out = cli(&["validate", &example_dir().to_string_lossy()]);

    assert_eq!(out.status.code(), Some(0), "stdout: {}", stdout(&out));
    assert_eq!(stdout(&out).lines().last(), Some("ok: 4 files checked"));
}

/// The exact expected list of diagnostics for a directory with seven independent problems, in
/// stable order (REQ-T-2: all errors at once; REQ-K-4: reproducible output).
#[test]
fn given_bad_fixture_dir_then_exit_1_and_every_diagnostic_printed_as_path_line_col() {
    let out = cli(&["validate", BAD]);

    let want: Vec<String> = expected_bad()
        .iter()
        .map(|(p, l, c, code)| format!("{p}:{l}:{c}: error[{code}"))
        .collect();
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        heads(&stdout(&out)),
        want.iter().map(|h| format!("{h}]")).collect::<Vec<_>>()
    );
    assert_eq!(stdout(&out).lines().last(), Some("7 error(s) in 6 file(s)"));
}

/// The plan's rendering of the cycle: one line, then a `note:` line for the other edge.
#[test]
fn given_bad_fixture_dir_then_the_cycle_is_printed_with_a_note_for_the_other_edge() {
    let out = cli(&["validate", BAD]);

    let text = stdout(&out);
    let lines: Vec<&str> = text.lines().collect();
    let at = lines
        .iter()
        .position(|l| l.contains("error[inheritance_cycle]"))
        .unwrap_or_else(|| panic!("no cycle line in:\n{text}"));
    assert_eq!(
        lines[at],
        format!(
            "{BAD}/a_cycle.toml:5:11: error[inheritance_cycle]: template inheritance cycle: `cli.cycle_a` -> `cli.cycle_b` -> `cli.cycle_a`"
        )
    );
    assert_eq!(
        lines[at + 1],
        format!("  note: `cli.cycle_b` extends `cli.cycle_a` ({BAD}/b_cycle.toml:5:11)")
    );
}

#[test]
fn given_bad_fixture_dir_twice_then_output_is_byte_identical() {
    let (a, b) = (cli(&["validate", BAD]), cli(&["validate", BAD]));

    assert_eq!(a.status.code(), Some(1));
    assert_ne!(stdout(&a), "");
    assert_eq!(a.stdout, b.stdout);
}

#[test]
fn given_missing_path_then_exit_1_with_io() {
    let out = cli(&["validate", "tests/fixtures/no_such_path"]);

    assert_eq!(out.status.code(), Some(1));
    let text = stdout(&out);
    assert!(text.contains("error[io]"), "{text}");
    assert!(text.contains("no_such_path"), "{text}");
}

#[test]
fn given_no_args_then_exit_2() {
    let out = cli(&["validate"]);

    assert_usage_error(&out, "required");
}

#[test]
fn given_unknown_flag_then_exit_2() {
    assert_usage_error(&cli(&["validate", "--no-such-flag", BAD]), "--no-such-flag");
}

#[test]
fn given_two_paths_then_both_reported() {
    let d = format!("{BAD}/d_params.toml");
    let f = format!("{BAD}/f_newer.toml");

    let out = cli(&["validate", &d, &f]);

    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        heads(&stdout(&out)),
        [
            format!("{d}:7:10: error[missing_param]"),
            format!("{d}:7:28: error[param_wrong_type]"),
            format!("{f}:3:18: error[unsupported_schema_version]"),
        ]
    );
    assert_eq!(stdout(&out).lines().last(), Some("3 error(s) in 2 file(s)"));
}

#[test]
fn given_a_valid_and_an_invalid_path_then_exit_1() {
    let out = cli(&["validate", &example(), &format!("{BAD}/f_newer.toml")]);

    assert_eq!(out.status.code(), Some(1));
    assert_eq!(heads(&stdout(&out)).len(), 1);
}

#[test]
fn validate_prints_no_state_lines_or_hash() {
    let out = cli(&["validate", &example()]);

    let text = stdout(&out);
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    assert!(!text.contains("tick="), "{text}");
    assert!(!text.contains("state_hash"), "{text}");
}

// --- --json ---------------------------------------------------------------------------------

#[test]
fn given_json_flag_then_output_parses_with_codes_lines_columns() {
    let out = cli(&["validate", "--json", BAD]);

    let doc = json_of(&out);

    assert_eq!(out.status.code(), Some(1));
    assert_eq!(doc["valid"], false);
    let found: Vec<(String, u64, u64, String)> = doc["diagnostics"]
        .as_array()
        .unwrap_or_else(|| panic!("diagnostics is not an array: {doc}"))
        .iter()
        .map(|d| {
            (
                d["path"].as_str().unwrap_or_default().to_string(),
                d["line"].as_u64().unwrap_or_default(),
                d["column"].as_u64().unwrap_or_default(),
                d["code"].as_str().unwrap_or_default().to_string(),
            )
        })
        .collect();
    let want: Vec<(String, u64, u64, String)> = expected_bad()
        .into_iter()
        .map(|(p, l, c, code)| (p, u64::from(l), u64::from(c), code.to_string()))
        .collect();
    assert_eq!(found, want);
}

#[test]
fn given_json_flag_then_the_cycle_diagnostic_carries_its_note() {
    let doc = json_of(&cli(&["validate", "--json", BAD]));

    let cycle = &doc["diagnostics"][0];

    assert_eq!(cycle["code"], "inheritance_cycle");
    assert!(
        cycle["message"]
            .as_str()
            .is_some_and(|m| m.contains("`cli.cycle_a` -> `cli.cycle_b`"))
    );
    let notes = cycle["notes"]
        .as_array()
        .unwrap_or_else(|| panic!("notes: {cycle}"));
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0]["message"], "`cli.cycle_b` extends `cli.cycle_a`");
    assert_eq!(notes[0]["path"], format!("{BAD}/b_cycle.toml"));
    assert_eq!(notes[0]["line"], 5);
    assert_eq!(notes[0]["column"], 11);
}

#[test]
fn given_json_flag_and_valid_example_then_valid_true_with_the_checked_files() {
    let out = cli(&["validate", "--json", &example()]);

    let doc = json_of(&out);

    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    assert_eq!(doc["valid"], true);
    assert_eq!(doc["diagnostics"], serde_json::json!([]));
    assert_eq!(doc["files"].as_array().map(Vec::len), Some(4));
}

#[test]
fn given_json_flag_and_missing_path_then_an_io_diagnostic() {
    let out = cli(&["validate", "--json", "tests/fixtures/no_such_path"]);

    let doc = json_of(&out);

    assert_eq!(out.status.code(), Some(1));
    assert_eq!(doc["valid"], false);
    assert_eq!(doc["diagnostics"][0]["code"], "io");
}

#[test]
fn given_json_flag_then_stdout_is_a_single_document() {
    let out = cli(&["validate", "--json", BAD]);

    let text = stdout(&out);

    assert_eq!(text.trim_end().lines().count(), 1, "{text}");
}

/// Review item 15: a child template validated on its own finds its parent next to it.
#[test]
fn given_child_template_file_with_its_parent_in_the_same_directory_then_exit_0() {
    let file = example_dir().join("templates/tank_mbt_generic.toml");

    let out = cli(&["validate", &file.to_string_lossy()]);

    assert_eq!(out.status.code(), Some(0), "stdout: {}", stdout(&out));
    assert_eq!(stdout(&out).lines().last(), Some("ok: 1 files checked"));
}

/// Overlapping paths (a directory and a file in it) count each file and diagnostic once.
#[test]
fn given_overlapping_paths_then_files_and_diagnostics_are_counted_once() {
    let file = format!("{BAD}/d_params.toml");
    let alone = cli(&["validate", BAD]);

    let both = cli(&["validate", BAD, &file]);
    let json = json_of(&cli(&["validate", "--json", BAD, &file]));

    assert_eq!(stdout(&both), stdout(&alone));
    assert_eq!(json["diagnostics"].as_array().map(Vec::len), Some(7));
    assert_eq!(
        json["files"],
        json_of(&cli(&["validate", "--json", BAD]))["files"]
    );
}

#[test]
fn given_overlapping_valid_paths_then_each_file_is_counted_once() {
    let out = cli(&[
        "validate",
        &example(),
        &example_dir().to_string_lossy(),
        &example_dir().join("templates").to_string_lossy(),
    ]);

    assert_eq!(out.status.code(), Some(0), "stdout: {}", stdout(&out));
    assert_eq!(stdout(&out).lines().last(), Some("ok: 4 files checked"));
}
