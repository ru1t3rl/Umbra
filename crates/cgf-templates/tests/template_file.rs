//! AT-2 (file-level rows), AT-5 (template rows) and ASM-054 rule 3: reading one template file.
//! Requirements: REQ-T-2, REQ-T-5, ASM-054, ASM-056, ASM-057, ADR 0010.
//!
//! Every assertion checks code, path, line and column. Positions are found in the source text with
//! `pos_of`, which returns the line and the character column of the first occurrence of a needle.

#![forbid(unsafe_code)]

mod common;

use cgf_templates::SourceFile;
use common::{
    alpha_template, errors, fixture_path, fixture_source, lines, ok, pos_of, row, rows, src,
    templates,
};

const PATH: &str = "templates/t.toml";

/// The motion section every valid template needs, so a test isolates the error it is about.
const MOTION: [&str; 3] = [
    "[motion]",
    "model = \"stub.motion.alpha\"",
    "params = { a = 1.0 }",
];

fn with_motion(head: &[&str]) -> String {
    let all: Vec<&str> = head.iter().chain(MOTION.iter()).copied().collect();
    lines(&all)
}

fn load(text: &str) -> Vec<common::Row> {
    rows(&errors(templates(&[src(PATH, text)])))
}

fn one(code: &str, at: (u32, u32)) -> Vec<common::Row> {
    vec![row(code, PATH, at)]
}

// --- accepted ----------------------------------------------------------------------------------

/// REQ-T-5: version 1 is the supported version.
#[test]
fn schema_version_1_is_accepted() {
    let set = ok(templates(&[src(
        PATH,
        &alpha_template("tank.base", None, "1.0"),
    )]));

    assert_eq!(set.len(), 1);
}

#[test]
fn given_description_and_network_sections_then_template_is_accepted() {
    let text = lines(&[
        "schema_version = 1",
        "id = \"tank.base\"",
        "description = \"Generic tank\"",
        "[motion]",
        "model = \"stub.motion.alpha\"",
        "params = { a = 2.0 }",
        "[network.dis]",
        "entity_type = \"1.1.225.1.1.1.0\"",
    ]);

    let set = ok(templates(&[src(PATH, &text)]));

    assert_eq!(set.len(), 1);
}

#[test]
fn given_header_style_params_table_then_template_is_accepted() {
    let text = lines(&[
        "schema_version = 1",
        "id = \"tank.base\"",
        "[motion]",
        "model = \"stub.motion.alpha\"",
        "[motion.params]",
        "a = 2.0",
    ]);

    assert_eq!(ok(templates(&[src(PATH, &text)])).len(), 1);
}

// --- AT-5: schema_version ----------------------------------------------------------------------

/// The plan's example, exactly: `new.toml:1:18: error[unsupported_schema_version]`. The file stops
/// at the version check, so the unknown key after it is not reported.
#[test]
fn newer_schema_version_is_rejected_at_its_value_and_file_stops() {
    let text = lines(&["schema_version = 2", "colour = \"red\"", "id = 5"]);

    let found = errors(templates(&[src("new.toml", &text)]));

    assert_eq!(
        rows(&found),
        [row("unsupported_schema_version", "new.toml", (1, 18))]
    );
    assert_eq!(
        found[0].message,
        "schema_version 2 is newer than the newest supported version 1"
    );
}

#[test]
fn missing_schema_version_is_missing_key() {
    let text = lines(&[
        "id = \"tank.base\"",
        "[motion]",
        "model = \"stub.motion.alpha\"",
    ]);

    let found = errors(templates(&[src(PATH, &text)]));

    // The location of a key that is absent from the file is not specified by the plan; only the
    // code and the file are pinned (see the plan ambiguities in the M2 running log).
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].code.as_str(), "missing_key");
    assert!(
        found[0].message.contains("schema_version"),
        "{}",
        found[0].message
    );
    assert_eq!(
        found[0].location.as_ref().map(|l| l.path.clone()),
        Some(PATH.into())
    );
}

#[test]
fn schema_version_zero_is_invalid_value() {
    let text = with_motion(&["schema_version = 0", "id = \"tank.base\""]);

    assert_eq!(load(&text), one("invalid_value", (1, 18)));
}

#[test]
fn schema_version_negative_is_invalid_value() {
    let text = with_motion(&["schema_version = -1", "id = \"tank.base\""]);

    assert_eq!(load(&text), one("invalid_value", (1, 18)));
}

#[test]
fn schema_version_as_string_is_wrong_type() {
    let text = with_motion(&["schema_version = \"1\"", "id = \"tank.base\""]);

    assert_eq!(load(&text), one("wrong_type", (1, 18)));
}

#[test]
fn schema_version_as_float_is_wrong_type() {
    let text = with_motion(&["schema_version = 1.0", "id = \"tank.base\""]);

    assert_eq!(load(&text), one("wrong_type", (1, 18)));
}

/// REQ-T-5 + REQ-T-2: a file that stops at its version check does not hide errors in other files.
#[test]
fn newer_version_in_one_file_does_not_hide_errors_in_others() {
    let newer = lines(&["schema_version = 3", "id = \"a.newer\""]);
    let broken = with_motion(&[
        "schema_version = 1",
        "id = \"b.broken\"",
        "colour = \"red\"",
    ]);

    let found = errors(templates(&[src("a.toml", &newer), src("b.toml", &broken)]));

    assert_eq!(
        rows(&found),
        [
            row("unsupported_schema_version", "a.toml", (1, 18)),
            row("unknown_key", "b.toml", (3, 1)),
        ]
    );
}

// --- AT-2: structure of one file ---------------------------------------------------------------

#[test]
fn given_unknown_top_level_key_then_unknown_key() {
    let text = lines(&[
        "schema_version = 1",
        "id = \"tank.base\"",
        "colour = \"red\"",
        "[motion]",
        "model = \"stub.motion.alpha\"",
        "params = { a = 1.0 }",
    ]);

    assert_eq!(load(&text), one("unknown_key", (3, 1)));
}

#[test]
fn given_unknown_key_in_motion_table_then_unknown_key_at_that_key() {
    let text = lines(&[
        "schema_version = 1",
        "id = \"tank.base\"",
        "[motion]",
        "model = \"stub.motion.alpha\"",
        "params = { a = 1.0 }",
        "speed = 3",
    ]);

    assert_eq!(load(&text), one("unknown_key", (6, 1)));
}

#[test]
fn given_wrong_type_for_extends_then_wrong_type_at_the_value() {
    let text = lines(&[
        "schema_version = 1",
        "id = \"tank.base\"",
        "extends = 5",
        "[motion]",
        "model = \"stub.motion.alpha\"",
        "params = { a = 1.0 }",
    ]);

    assert_eq!(load(&text), one("wrong_type", (3, 11)));
}

#[test]
fn given_wrong_type_for_id_then_wrong_type_at_the_value() {
    let text = with_motion(&["schema_version = 1", "id = 5"]);

    assert_eq!(load(&text), one("wrong_type", (2, 6)));
}

#[test]
fn given_wrong_type_for_description_then_wrong_type_at_the_value() {
    let text = lines(&[
        "schema_version = 1",
        "id = \"tank.base\"",
        "description = [1]",
        "[motion]",
        "model = \"stub.motion.alpha\"",
        "params = { a = 1.0 }",
    ]);

    assert_eq!(load(&text), one("wrong_type", (3, 15)));
}

#[test]
fn given_motion_as_a_string_then_wrong_type_at_the_value() {
    let text = lines(&[
        "schema_version = 1",
        "id = \"tank.base\"",
        "motion = \"fast\"",
    ]);

    assert_eq!(load(&text), one("wrong_type", (3, 10)));
}

#[test]
fn given_wrong_type_for_motion_model_then_wrong_type_at_the_value() {
    let text = lines(&[
        "schema_version = 1",
        "id = \"tank.base\"",
        "[motion]",
        "model = 5",
    ]);

    assert_eq!(load(&text), one("wrong_type", (4, 9)));
}

#[test]
fn given_params_that_are_not_a_table_then_wrong_type_at_the_value() {
    let text = lines(&[
        "schema_version = 1",
        "id = \"tank.base\"",
        "[motion]",
        "model = \"stub.motion.alpha\"",
        "params = 5",
    ]);

    assert_eq!(load(&text), one("wrong_type", (5, 10)));
}

#[test]
fn given_missing_id_then_missing_key() {
    let text = lines(&[
        "schema_version = 1",
        "[motion]",
        "model = \"stub.motion.alpha\"",
        "params = { a = 1.0 }",
    ]);

    let found = errors(templates(&[src(PATH, &text)]));

    // Location of an absent key: unspecified by the plan (ambiguity); code, file and message only.
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].code.as_str(), "missing_key");
    assert!(found[0].message.contains("`id`"), "{}", found[0].message);
}

#[test]
fn given_template_without_a_motion_model_then_missing_key() {
    let text = lines(&["schema_version = 1", "id = \"tank.base\""]);

    let found = errors(templates(&[src(PATH, &text)]));

    // ADR 0010: every resolved template names a motion model. The location is not specified.
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].code.as_str(), "missing_key");
    assert!(found[0].message.contains("motion"), "{}", found[0].message);
}

/// The column of the offending character, not of the string.
#[test]
fn given_invalid_template_id_then_invalid_id_with_column_of_bad_char() {
    let text = lines(&[
        "schema_version = 1",
        "id = \"tank.mbT\"",
        "[motion]",
        "model = \"stub.motion.alpha\"",
        "params = { a = 1.0 }",
    ]);

    assert_eq!(load(&text), one("invalid_id", pos_of(&text, "T\"")));
}

#[test]
fn given_template_id_without_namespace_then_invalid_id_at_the_value() {
    let text = with_motion(&["schema_version = 1", "id = \"tank\""]);

    assert_eq!(load(&text), one("invalid_id", (2, 6)));
}

#[test]
fn given_empty_template_id_then_invalid_id_at_the_value() {
    let text = with_motion(&["schema_version = 1", "id = \"\""]);

    assert_eq!(load(&text), one("invalid_id", (2, 6)));
}

#[test]
fn given_invalid_extends_id_then_invalid_id_at_the_bad_char() {
    let text = lines(&[
        "schema_version = 1",
        "id = \"tank.base\"",
        "extends = \"bad id\"",
        "[motion]",
        "model = \"stub.motion.alpha\"",
        "params = { a = 1.0 }",
    ]);

    assert_eq!(load(&text), one("invalid_id", pos_of(&text, " id\"")));
}

#[test]
fn given_invalid_motion_model_id_then_invalid_id_at_the_bad_char() {
    let text = lines(&[
        "schema_version = 1",
        "id = \"tank.base\"",
        "[motion]",
        "model = \"Stub.motion\"",
        "params = { }",
    ]);

    assert_eq!(load(&text), one("invalid_id", pos_of(&text, "Stub")));
}

#[test]
fn given_datetime_value_in_a_template_then_unsupported_value_at_it() {
    let text = lines(&[
        "schema_version = 1",
        "id = \"tank.base\"",
        "built = 1979-05-27",
        "[motion]",
        "model = \"stub.motion.alpha\"",
        "params = { a = 1.0 }",
    ]);

    // The datetime is a value the loader rejects; the unknown key `built` is a separate problem and
    // the reader may report either or both, so only the datetime row is pinned.
    let found = load(&text);

    assert!(
        found.contains(&row("unsupported_value", PATH, (3, 9))),
        "{found:?}"
    );
}

/// REQ-T-2 "all errors": independent problems of one file are reported together, in file order.
#[test]
fn given_several_errors_in_one_file_then_all_are_reported_in_file_order() {
    let text = lines(&[
        "schema_version = 1",
        "id = \"tank.mbT\"",
        "colour = \"red\"",
        "description = 5",
        "[motion]",
        "model = 5",
        "speed = 3",
    ]);

    assert_eq!(
        load(&text),
        [
            row("invalid_id", PATH, pos_of(&text, "T\"")),
            row("unknown_key", PATH, (3, 1)),
            row("wrong_type", PATH, (4, 15)),
            row("wrong_type", PATH, (6, 9)),
            row("unknown_key", PATH, (7, 1)),
        ]
    );
}

// --- AT-2: syntax errors of one file -----------------------------------------------------------

/// REQ-T-2: each syntax error of a file has file, line and column (ASM-054 rule 1).
#[test]
fn given_syntax_errors_then_each_has_file_line_column() {
    let rel = "syntax/inline_two_bad_values.toml";

    let found = errors(templates(&[fixture_source(rel)]));

    assert_eq!(
        rows(&found),
        [
            row("syntax", fixture_path(rel), (9, 16)),
            row("syntax", fixture_path(rel), (9, 23)),
        ]
    );
}

/// ASM-054 rule 3: a file with syntax errors is not checked semantically.
#[test]
fn given_syntax_error_and_a_semantic_error_in_one_file_then_only_the_syntax_error_is_reported() {
    let text = lines(&["schema_version = 1", "colour = \"red\"", "a = $"]);

    assert_eq!(load(&text), one("syntax", (3, 5)));
}

#[test]
fn given_syntax_error_in_one_file_then_semantic_errors_in_other_files_are_still_reported() {
    let broken = lines(&["schema_version = 1", "a = $"]);
    let other = with_motion(&["schema_version = 1", "id = \"b.other\"", "colour = \"red\""]);

    let found = errors(templates(&[src("a.toml", &broken), src("b.toml", &other)]));

    assert_eq!(
        rows(&found),
        [
            row("syntax", "a.toml", (2, 5)),
            row("unknown_key", "b.toml", (3, 1))
        ]
    );
}

/// ASM-054 rule 3: the broken file's top-level `id` is recovered, so the valid child in another
/// file that extends it gets no `unknown_template` error; only the syntax error is reported.
#[test]
fn given_broken_file_with_recoverable_id_then_child_in_another_file_gets_no_unknown_template() {
    let broken = "syntax/broken_with_id/broken.toml";
    let child = "syntax/broken_with_id/child.toml";

    let found = errors(templates(&[fixture_source(broken), fixture_source(child)]));

    assert_eq!(rows(&found), [row("syntax", fixture_path(broken), (8, 16))]);
}

/// ASM-054 rule 3, the note variant: the broken file's id cannot be recovered (its value is junk),
/// so the child's `extends` is `unknown_template` with a note naming the broken file.
#[test]
fn given_broken_file_with_unrecoverable_id_then_child_gets_unknown_template_with_a_note() {
    let broken = "syntax/broken_without_id/broken.toml";
    let child = "syntax/broken_without_id/child.toml";
    let child_text = common::read_fixture(child);

    let found = errors(templates(&[fixture_source(child), fixture_source(broken)]));

    assert_eq!(
        rows(&found),
        [
            row("syntax", fixture_path(broken), (4, 6)),
            row(
                "unknown_template",
                fixture_path(child),
                pos_of(&child_text, "\"syntax.lost\"")
            ),
        ]
    );
    let notes: Vec<&str> = found[1].notes.iter().map(|n| n.message.as_str()).collect();
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert!(notes[0].contains("may be defined in"), "{}", notes[0]);
    assert!(notes[0].contains("broken.toml"), "{}", notes[0]);
    assert!(notes[0].contains("syntax errors"), "{}", notes[0]);
}

/// ASM-054 rule 3 does not mute genuinely unknown parents: with another healthy file around, a
/// child of a template that exists nowhere is still `unknown_template`, and carries the note only
/// because a file with syntax errors exists.
#[test]
fn given_no_broken_file_then_unknown_template_has_no_syntax_note() {
    let child = lines(&[
        "schema_version = 1",
        "id = \"c.child\"",
        "extends = \"syntax.lost\"",
    ]);

    let found = errors(templates(&[src("c.toml", &child)]));

    assert_eq!(found.len(), 1);
    assert!(found[0].notes.is_empty(), "{:?}", found[0].notes);
}

// --- Display -----------------------------------------------------------------------------------

/// ASM-056 and the plan's error output: `path:line:col: error[code]: message`.
#[test]
fn rendered_diagnostic_is_path_line_col_error_code_message() {
    let text = lines(&[
        "schema_version = 1",
        "id = \"tank.base\"",
        "colour = \"red\"",
        "[motion]",
        "model = \"stub.motion.alpha\"",
        "params = { a = 1.0 }",
    ]);

    let found = errors(templates(&[SourceFile::new("dir/t.toml", text.as_str())]));

    assert_eq!(found.len(), 1);
    assert_eq!(
        found[0].to_string(),
        format!("dir/t.toml:3:1: error[unknown_key]: {}", found[0].message)
    );
}

#[test]
fn diagnostic_codes_have_stable_snake_case_names() {
    use cgf_templates::DiagnosticCode as C;
    let names = [
        (C::Io, "io"),
        (C::Syntax, "syntax"),
        (C::UnsupportedValue, "unsupported_value"),
        (C::UnknownKey, "unknown_key"),
        (C::MissingKey, "missing_key"),
        (C::WrongType, "wrong_type"),
        (C::InvalidValue, "invalid_value"),
        (C::InvalidId, "invalid_id"),
        (C::UnsupportedSchemaVersion, "unsupported_schema_version"),
        (C::DuplicateId, "duplicate_id"),
        (C::UnknownTemplate, "unknown_template"),
        (C::UnknownId, "unknown_id"),
        (C::InheritanceCycle, "inheritance_cycle"),
        (C::MissingParam, "missing_param"),
        (C::ParamWrongType, "param_wrong_type"),
        (C::ParamOutOfRange, "param_out_of_range"),
        (C::UnknownParam, "unknown_param"),
        (C::ParamInvalid, "param_invalid"),
        (C::DuplicateName, "duplicate_name"),
        (C::NotAScenario, "not_a_scenario"),
    ];

    for (code, name) in names {
        assert_eq!(code.as_str(), name);
    }
}
