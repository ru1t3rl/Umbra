//! Review fixes for WI-7 (REQ-T-2: no error may be lost): dependents of unusable templates keep
//! their own errors, duplicate ids are found across broken files, `unknown_id` names the kind
//! an ID is registered under. Probes P1..P3b from the review.

#![forbid(unsafe_code)]

mod common;

use common::{errors, lines, pos_of, row, rows, src, templates};

fn child(model: &str) -> String {
    lines(&[
        "schema_version = 1",
        "id = \"c.child\"",
        "extends = \"p.parent\"",
        "[motion]",
        &format!("model = \"{model}\""),
        "[[sensors]]",
        "kind = \"x.y\"",
    ])
}

fn parent(extra: &[&str]) -> String {
    let mut l = vec!["schema_version = 1", "id = \"p.parent\""];
    l.extend_from_slice(extra);
    l.extend_from_slice(&[
        "[motion]",
        "model = \"stub.motion.alpha\"",
        "params = { a = 1.0 }",
    ]);
    lines(&l)
}

/// P1: the parent has an error of its own; the child's independent errors are still reported.
#[test]
fn child_of_a_broken_parent_keeps_its_own_unknown_ids() {
    let found = errors(templates(&[
        src("a.toml", &parent(&["colour = \"red\""])),
        src("b.toml", &child("stub.motion.nope")),
    ]));

    assert_eq!(
        rows(&found),
        [
            row("unknown_key", "a.toml", (3, 1)),
            row("unknown_id", "b.toml", (5, 9)),
            row("unknown_id", "b.toml", (7, 8)),
        ]
    );
}

/// P2: the parent is defined twice.
#[test]
fn child_of_a_duplicate_parent_keeps_its_own_unknown_ids() {
    let found = errors(templates(&[
        src("a.toml", &parent(&[])),
        src("b.toml", &parent(&[])),
        src("c.toml", &child("stub.motion.nope")),
    ]));

    assert_eq!(
        rows(&found),
        [
            row("duplicate_id", "b.toml", (2, 6)),
            row("unknown_id", "c.toml", (5, 9)),
            row("unknown_id", "c.toml", (7, 8)),
        ]
    );
}

/// P2b: the parent has an unknown parent of its own.
#[test]
fn child_of_a_template_with_an_unknown_parent_keeps_its_own_unknown_ids() {
    let found = errors(templates(&[
        src("a.toml", &parent(&["extends = \"nowhere.x\""])),
        src("b.toml", &child("stub.motion.nope")),
    ]));

    assert_eq!(
        rows(&found),
        [
            row("unknown_template", "a.toml", (3, 11)),
            row("unknown_id", "b.toml", (5, 9)),
            row("unknown_id", "b.toml", (7, 8)),
        ]
    );
}

/// A child naming a different model than its known ancestor drops the ancestor's params, so its
/// own params are checked even though the ancestor chain is unusable.
#[test]
fn child_switching_model_under_an_unusable_ancestor_has_its_params_checked() {
    let text = lines(&[
        "schema_version = 1",
        "id = \"c.child\"",
        "extends = \"p.parent\"",
        "[motion]",
        "model = \"stub.motion.alpha\"",
        "params = { a = \"bad\" }",
    ]);
    let top = lines(&[
        "schema_version = 1",
        "id = \"p.parent\"",
        "extends = \"nowhere.x\"",
        "[motion]",
        "model = \"stub.motion.gamma\"",
        "params = { x = 1.0 }",
    ]);

    let found = errors(templates(&[src("a.toml", &top), src("b.toml", &text)]));

    assert_eq!(
        rows(&found),
        [
            row("unknown_template", "a.toml", (3, 11)),
            row("param_wrong_type", "b.toml", pos_of(&text, "\"bad\"")),
        ]
    );
}

/// P3: one definition of an ID is in a file with an error.
#[test]
fn duplicate_id_is_found_when_one_definition_is_in_a_file_with_an_unknown_key() {
    let broken = lines(&[
        "schema_version = 1",
        "id = \"x.x\"",
        "colour = \"red\"",
        "[motion]",
        "model = \"stub.motion.alpha\"",
        "params = { a = 1.0 }",
    ]);
    let good = lines(&[
        "schema_version = 1",
        "id = \"x.x\"",
        "[motion]",
        "model = \"stub.motion.nope\"",
    ]);

    let found = errors(templates(&[src("a.toml", &broken), src("b.toml", &good)]));

    assert_eq!(
        rows(&found),
        [
            row("unknown_key", "a.toml", (3, 1)),
            row("duplicate_id", "b.toml", (2, 6)),
            row("unknown_id", "b.toml", (4, 9)),
        ]
    );
    let note = found[1].notes[0]
        .location
        .as_ref()
        .map(|l| (l.path.clone(), l.line, l.column));
    assert_eq!(note, Some(("a.toml".into(), 2, 6)));
}

/// P3b: the same with a syntax error in the first file (its `id` is recovered).
#[test]
fn duplicate_id_is_found_when_one_definition_is_in_a_file_with_a_syntax_error() {
    let broken = lines(&["schema_version = 1", "id = \"x.x\"", "a = $"]);
    let good = lines(&[
        "schema_version = 1",
        "id = \"x.x\"",
        "[motion]",
        "model = \"stub.motion.nope\"",
    ]);

    let found = errors(templates(&[src("a.toml", &broken), src("b.toml", &good)]));

    assert_eq!(
        rows(&found),
        [
            row("syntax", "a.toml", (3, 5)),
            row("duplicate_id", "b.toml", (2, 6)),
            row("unknown_id", "b.toml", (4, 9)),
        ]
    );
}

/// Item 4: an ID registered under another kind is reported as such.
#[test]
fn motion_model_naming_a_terrain_provider_says_which_kind_it_is() {
    let text = lines(&[
        "schema_version = 1",
        "id = \"t.t\"",
        "[motion]",
        "model = \"stub.terrain.flat\"",
    ]);

    let found = errors(templates(&[src("t.toml", &text)]));

    assert_eq!(rows(&found), [row("unknown_id", "t.toml", (4, 9))]);
    assert_eq!(
        found[0].message,
        "`stub.terrain.flat` is a terrain provider, not a motion model"
    );
}

// --- remaining lost-error cases (N1, N2) ---------------------------------------------------------

fn plain(id: &str, extends: Option<&str>) -> String {
    let mut l = vec!["schema_version = 1".to_string(), format!("id = \"{id}\"")];
    if let Some(e) = extends {
        l.push(format!("extends = \"{e}\""));
    }
    l.push("[motion]".into());
    l.push("model = \"stub.motion.alpha\"".into());
    l.push("params = { a = 1.0 }".into());
    l.join("\n") + "\n"
}

/// N1: an unknown parent on the second definition of a duplicated id.
#[test]
fn unknown_extends_on_the_second_definition_of_a_duplicated_id_is_reported() {
    let found = errors(templates(&[
        src("a.toml", &plain("x.x", None)),
        src("b.toml", &plain("x.x", Some("nope.nope"))),
    ]));

    assert_eq!(
        rows(&found),
        [
            row("duplicate_id", "b.toml", (2, 6)),
            row("unknown_template", "b.toml", (3, 11)),
        ]
    );
}

/// N1: and on the first definition.
#[test]
fn unknown_extends_on_the_first_definition_of_a_duplicated_id_is_reported() {
    let found = errors(templates(&[
        src("a.toml", &plain("x.x", Some("nope.nope"))),
        src("b.toml", &plain("x.x", None)),
    ]));

    assert_eq!(
        rows(&found),
        [
            row("unknown_template", "a.toml", (3, 11)),
            row("duplicate_id", "b.toml", (2, 6)),
        ]
    );
}

fn child_with_params(model: Option<&str>) -> String {
    let mut l = vec![
        "schema_version = 1",
        "id = \"c.child\"",
        "extends = \"p.parent\"",
        "[motion]",
    ];
    let model_line = model.map(|m| format!("model = \"{m}\""));
    if let Some(line) = &model_line {
        l.push(line);
    }
    l.push("params = { a = \"str\", zz = 1 }");
    lines(&l)
}

/// N2: the child's own wrong-typed and unknown params are reported under a broken parent.
#[test]
fn child_params_are_checked_key_by_key_under_a_broken_parent() {
    let text = child_with_params(Some("stub.motion.alpha"));

    let found = errors(templates(&[
        src("a.toml", &parent(&["colour = \"red\""])),
        src("b.toml", &text),
    ]));

    assert_eq!(
        rows(&found),
        [
            row("unknown_key", "a.toml", (3, 1)),
            row("param_wrong_type", "b.toml", pos_of(&text, "\"str\"")),
            row("unknown_param", "b.toml", pos_of(&text, "zz")),
        ]
    );
}

/// N2: the model may come from a readable but unusable ancestor.
#[test]
fn child_params_are_checked_against_the_model_of_an_unusable_ancestor() {
    let text = child_with_params(None);

    let found = errors(templates(&[
        src("a.toml", &parent(&["extends = \"nowhere.x\""])),
        src("b.toml", &text),
    ]));

    assert_eq!(
        rows(&found),
        [
            row("unknown_template", "a.toml", (3, 11)),
            row("param_wrong_type", "b.toml", pos_of(&text, "\"str\"")),
            row("unknown_param", "b.toml", pos_of(&text, "zz")),
        ]
    );
}

/// N2: a required parameter that the missing parent may supply is not reported.
#[test]
fn missing_params_are_not_reported_under_an_unusable_parent() {
    let text = lines(&[
        "schema_version = 1",
        "id = \"c.child\"",
        "extends = \"p.parent\"",
        "[motion]",
        "model = \"stub.motion.alpha\"",
        "params = { }",
    ]);

    let found = errors(templates(&[
        src("a.toml", &parent(&["colour = \"red\""])),
        src("b.toml", &text),
    ]));

    assert_eq!(rows(&found), [row("unknown_key", "a.toml", (3, 1))]);
}
