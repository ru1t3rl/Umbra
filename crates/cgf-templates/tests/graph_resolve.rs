//! AT-4 (FSM rules), AT-5 (subgraph references and versions), AT-6 (call sites and bindings), AT-8
//! (cycles, depth, tree shape, many errors at once), AT-11 (leaf parameters) and AT-20 (input
//! bindings, load-time rows): resolving a set of behavior graph files.
//! Requirements: REQ-B-9, REQ-B-10, REQ-B-11, REQ-B-12, REQ-B-14, REQ-B-15, REQ-T-2, REQ-T-3,
//! ADR 0012, ADR 0014, ASM-056, ASM-072, ASM-073, ASM-075, ASM-076, ASM-085, ASM-088, ASM-089.
//!
//! Step 4 of M3: written before WI-13 and enabled with it. Two tests changed in WI-13 (ADR 0014 has
//! no object params, ASM-102): `key_form_in_leaf_params_is_invalid_binding` and
//! `param_and_key_forms_are_distinct`.
//!
//! Registry and helpers: `graph_support` (stub conditions, actions and one stub behavior,
//! `stub.behavior.ext`, with a declared binding-only param `target`). Positions follow the M2
//! conventions: a missing param at the `params` table (an inline table at its `{`), an unknown
//! param at its key, a wrong or out-of-range value at the value; graph-level errors at the
//! offending name or reference value, as stated per test. Choices the plan leaves open are listed
//! in the plan's running log.

#![forbid(unsafe_code)]

mod common;
mod graph_support;

use cgf_templates::{Diagnostic, SourceFile};
use common::{Row, errors, lines, ok, pos_after, pos_of, pos_of_nth, row, rows, src};
use graph_support::{caller, graphs, graphs_with_depth, minimal, with_interface};

fn path(name: &str) -> String {
    format!("behaviors/{name}.behavior.toml")
}

fn files(items: &[(&str, &str)]) -> Vec<SourceFile> {
    items.iter().map(|(n, t)| src(&path(n), t)).collect()
}

fn found(items: &[(&str, &str)]) -> Vec<Diagnostic> {
    errors(graphs(&files(items)))
}

fn rows_of(items: &[(&str, &str)]) -> Vec<Row> {
    rows(&found(items))
}

/// The child used by the call-site tests: `count` (required integer >= 1) and `label` (optional
/// string, default `"x"`).
const CHILD_PARAMS: &str = "params = { count = { type = \"integer\", minimum = 1 }, label = { type = \"string\", default = \"x\" } }";

fn child(id: &str) -> String {
    with_interface(id, CHILD_PARAMS)
}

// --- AT-4: FSM rules ---------------------------------------------------------------------------

/// An FSM node `m` with the given FSM lines, two action nodes `x` and `y` (the state nodes) and
/// the given extra node lines.
fn fsm_graph(fsm_lines: &[&str], extra_nodes: &[&str]) -> String {
    let mut all = vec![
        "schema_version = 1",
        "id = \"test.graph.fsm\"",
        "version = \"1.0.0\"",
        "root = \"m\"",
        "[nodes.m]",
        "type = \"fsm\"",
    ];
    all.extend_from_slice(fsm_lines);
    all.extend_from_slice(&[
        "[nodes.x]",
        "type = \"action\"",
        "ref = \"stub.action.noop\"",
        "[nodes.y]",
        "type = \"action\"",
        "ref = \"stub.action.noop\"",
    ]);
    all.extend_from_slice(extra_nodes);
    lines(&all)
}

/// REQ-B-12: an FSM with no `initial` is `no_initial_state` (absent key: code, file and message).
#[test]
fn fsm_without_initial_is_no_initial_state() {
    let text = fsm_graph(&["states = { a = \"x\", b = \"y\" }"], &[]);

    let d = found(&[("g", &text)]);

    assert_eq!(d.len(), 1, "{d:?}");
    assert_eq!(d[0].code.as_str(), "no_initial_state");
    assert!(d[0].message.contains("initial"), "{}", d[0].message);
}

/// REQ-B-12: an `initial` that names no state is `no_initial_state` at the value.
#[test]
fn unknown_initial_is_no_initial_state() {
    let text = fsm_graph(
        &["initial = \"nowhere\"", "states = { a = \"x\", b = \"y\" }"],
        &[],
    );

    assert_eq!(
        rows_of(&[("g", &text)]),
        [row(
            "no_initial_state",
            path("g"),
            pos_of(&text, "\"nowhere\"")
        )]
    );
}

/// REQ-B-12: a state that cannot be reached from `initial` is `unreachable_state`, at the state's
/// key in `states`.
#[test]
fn unreachable_state_reported_at_state_key() {
    let text = fsm_graph(
        &["initial = \"a\"", "states = { a = \"x\", lonely = \"y\" }"],
        &[],
    );

    let d = found(&[("g", &text)]);

    assert_eq!(
        rows(&d),
        [row("unreachable_state", path("g"), pos_of(&text, "lonely"))]
    );
    assert!(
        d[0].message.contains("lonely") && d[0].message.contains("`a`"),
        "{}",
        d[0].message
    );
}

/// ASM-072: exit names and state names share one namespace per FSM.
#[test]
fn exit_and_state_same_name_is_duplicate_name() {
    let text = fsm_graph(
        &[
            "initial = \"a\"",
            "states = { a = \"x\", b = \"y\" }",
            "exits = { b = \"success\" }",
            "[[nodes.m.transitions]]",
            "from = \"a\"",
            "to = \"b\"",
            "on = \"success\"",
        ],
        &[],
    );

    let d = rows_of(&[("g", &text)]);

    assert!(
        d.contains(&row(
            "duplicate_name",
            path("g"),
            pos_of(&text, "b = \"success\"")
        )),
        "{d:?}"
    );
}

/// ADR 0012: a guard subtree may contain only `condition`, `inverter`, `sequence` and `selector`;
/// an action is `invalid_guard`, at the guard reference.
#[test]
fn guard_with_action_is_invalid_guard() {
    let text = fsm_graph(
        &[
            "initial = \"a\"",
            "states = { a = \"x\", b = \"y\" }",
            "[[nodes.m.transitions]]",
            "from = \"a\"",
            "to = \"b\"",
            "when = \"guard\"",
        ],
        &[
            "[nodes.guard]",
            "type = \"action\"",
            "ref = \"stub.action.noop\"",
        ],
    );

    assert_eq!(
        rows_of(&[("g", &text)]),
        [row("invalid_guard", path("g"), pos_after(&text, "when = "))]
    );
}

// --- AT-5: subgraph references and versions ----------------------------------------------------

/// REQ-B-9: a `subgraph` ref to a graph of the same load resolves.
#[test]
fn subgraph_ref_to_file_graph_resolves() {
    let c = child("test.graph.child");
    let p = caller(
        "test.graph.parent",
        "test.graph.child",
        None,
        "{ count = 2 }",
        &[],
    );

    let set = ok(graphs(&files(&[("child", &c), ("parent", &p)])));

    assert_eq!(set.len(), 2);
    let parent = set.iter().find(|g| g.id.as_str() == "test.graph.parent");
    assert_eq!(parent.map(|g| g.subgraph_refs().len()), Some(1));
}

/// REQ-B-9: a `subgraph` ref to a registered behavior (a plugin stub) resolves the same way.
#[test]
fn subgraph_ref_to_registered_behavior_resolves() {
    let p = caller(
        "test.graph.parent",
        "stub.behavior.ext",
        None,
        "{ target = { key = \"t\" }, speed = 2.0 }",
        &["[blackboard]", "t = \"entity\""],
    );

    assert_eq!(ok(graphs(&files(&[("parent", &p)]))).len(), 1);
}

/// REQ-T-1: an unknown subgraph target is `unknown_id` at the ref, with a note that lists the
/// registered behaviors.
#[test]
fn unknown_subgraph_is_unknown_id_listing_behaviors() {
    let p = caller("test.graph.parent", "test.graph.nowhere", None, "", &[]);

    let d = found(&[("parent", &p)]);

    assert_eq!(
        rows(&d),
        [row(
            "unknown_id",
            path("parent"),
            pos_of(&p, "\"test.graph.nowhere\"")
        )]
    );
    assert!(
        d[0].message.contains("test.graph.nowhere"),
        "{}",
        d[0].message
    );
    assert!(
        d[0].notes
            .iter()
            .any(|n| n.message.contains("stub.behavior.ext")),
        "{:?}",
        d[0].notes
    );
}

/// ASM-063: an ID of another kind is named as such: "`x` is an action, not a behavior".
#[test]
fn subgraph_ref_to_action_is_kind_mismatch() {
    let p = caller("test.graph.parent", "stub.action.noop", None, "", &[]);

    let d = found(&[("parent", &p)]);

    assert_eq!(
        rows(&d),
        [row(
            "unknown_id",
            path("parent"),
            pos_of(&p, "\"stub.action.noop\"")
        )]
    );
    assert_eq!(
        d[0].message,
        "`stub.action.noop` is an action, not a behavior"
    );
}

/// REQ-B-12, ADR 0014: bindings are checked against the declared `param_types` of a registered
/// behavior: an `entity` key binds `target`; a `number` key is `invalid_binding`; an unbound
/// required `target` is `missing_param`.
#[test]
fn binding_checked_against_registered_behavior_param_types() {
    let with = |key_type: &str, params: &str| {
        caller(
            "test.graph.parent",
            "stub.behavior.ext",
            None,
            params,
            &["[blackboard]", &format!("t = \"{key_type}\"")],
        )
    };
    let good = with("entity", "{ target = { key = \"t\" } }");
    let wrong = with("number", "{ target = { key = \"t\" } }");
    let missing = with("entity", "{ }");

    assert_eq!(ok(graphs(&files(&[("parent", &good)]))).len(), 1);
    assert_eq!(
        rows_of(&[("parent", &wrong)]),
        [row(
            "invalid_binding",
            path("parent"),
            pos_of(&wrong, "{ key = \"t\" }")
        )]
    );
    let d = found(&[("parent", &missing)]);
    assert_eq!(
        rows(&d),
        [row(
            "missing_param",
            path("parent"),
            pos_after(&missing, "params = ")
        )]
    );
    assert!(d[0].message.contains("target"), "{}", d[0].message);
}

/// ASM-089 (b): the `params` table of a leaf is never classified itself: `params = { param = "x" }`
/// is a literal param named `param` (unknown to the leaf: `unknown_param` at the key), not a
/// reference.
#[test]
fn leaf_params_table_with_single_param_key_is_a_literal() {
    let text = lines(&[
        "schema_version = 1",
        "id = \"test.graph.literal\"",
        "version = \"1.0.0\"",
        "root = \"r\"",
        "[nodes.r]",
        "type = \"condition\"",
        "ref = \"stub.cond.flag\"",
        "params = { param = \"x\" }",
    ]);

    assert_eq!(
        rows_of(&[("g", &text)]),
        [row(
            "unknown_param",
            path("g"),
            pos_of(&text, "param = \"x\"")
        )]
    );
}

/// ASM-085: Cargo range semantics; the registered behavior has version 1.0.0.
#[test]
fn version_range_satisfied() {
    for range in ["^1", "^1.0", ">=1.0, <2", "=1.0.0", "~1.0", "*"] {
        let p = caller(
            "test.graph.parent",
            "stub.behavior.ext",
            Some(range),
            "{ target = { key = \"t\" } }",
            &["[blackboard]", "t = \"entity\""],
        );

        assert_eq!(ok(graphs(&files(&[("parent", &p)]))).len(), 1, "{range}");
    }
}

/// REQ-B-12: a version outside the range is `version_mismatch` at the `version` value, with a note
/// that gives the target's version and where it is defined.
#[test]
fn version_mismatch_reported_with_target_version_note() {
    let c = child("test.graph.attack");
    let p = caller(
        "test.graph.parent",
        "test.graph.attack",
        Some("^2.0"),
        "{ count = 1 }",
        &[],
    );

    let d = found(&[("attack", &c), ("parent", &p)]);

    assert_eq!(
        rows(&d),
        [row(
            "version_mismatch",
            path("parent"),
            pos_of(&p, "\"^2.0\"")
        )]
    );
    assert!(
        d[0].message.contains("1.0.0") && d[0].message.contains("^2.0"),
        "{}",
        d[0].message
    );
    let note = &d[0].notes[0];
    assert!(note.message.contains("defined"), "{}", note.message);
    let at = note
        .location
        .as_ref()
        .unwrap_or_else(|| panic!("note location"));
    assert_eq!(
        (at.path.clone(), (at.line, at.column)),
        (path("attack").into(), pos_of(&c, "\"1.0.0\""))
    );
}

/// ASM-085: a missing `version` on the call accepts any version.
#[test]
fn missing_version_accepts_any() {
    let c = child("test.graph.attack").replace("version = \"1.0.0\"", "version = \"7.3.1\"");
    let p = caller(
        "test.graph.parent",
        "test.graph.attack",
        None,
        "{ count = 1 }",
        &[],
    );

    assert_eq!(
        ok(graphs(&files(&[("attack", &c), ("parent", &p)]))).len(),
        2
    );
}

/// Cargo semantics: a pre-release is not matched by a plain range, only by one that names it.
#[test]
fn prerelease_needs_explicit_range() {
    let c = child("test.graph.attack").replace("version = \"1.0.0\"", "version = \"2.0.0-beta.1\"");
    let plain = caller(
        "test.graph.parent",
        "test.graph.attack",
        Some("^2.0"),
        "{ count = 1 }",
        &[],
    );
    let explicit = caller(
        "test.graph.parent",
        "test.graph.attack",
        Some(">=2.0.0-beta.1"),
        "{ count = 1 }",
        &[],
    );

    assert_eq!(
        rows_of(&[("attack", &c), ("parent", &plain)]),
        [row(
            "version_mismatch",
            path("parent"),
            pos_of(&plain, "\"^2.0\"")
        )]
    );
    assert_eq!(
        ok(graphs(&files(&[("attack", &c), ("parent", &explicit)]))).len(),
        2
    );
}

/// REQ-B-14: a version range that is not semver is `invalid_version` at the value.
#[test]
fn invalid_version_range_is_invalid_version() {
    let p = caller(
        "test.graph.parent",
        "stub.behavior.ext",
        Some("not a range"),
        "{ target = { key = \"t\" } }",
        &["[blackboard]", "t = \"entity\""],
    );

    assert_eq!(
        rows_of(&[("parent", &p)]),
        [row(
            "invalid_version",
            path("parent"),
            pos_of(&p, "\"not a range\"")
        )]
    );
}

// --- AT-6: call sites --------------------------------------------------------------------------

fn call_child(params: &str) -> (String, String) {
    (
        child("test.graph.child"),
        caller("test.graph.parent", "test.graph.child", None, params, &[]),
    )
}

/// REQ-B-10: a required param missing at the call site is `missing_param` at the `params` table.
#[test]
fn call_site_missing_param() {
    let (c, p) = call_child("{ }");

    let d = found(&[("child", &c), ("parent", &p)]);

    assert_eq!(
        rows(&d),
        [row(
            "missing_param",
            path("parent"),
            pos_after(&p, "params = ")
        )]
    );
    assert_eq!(
        d[0].message,
        "behavior `test.graph.child`: missing required parameter `count`"
    );
}

/// REQ-B-10: a param the interface does not declare is `unknown_param` at its key.
#[test]
fn call_site_unknown_param() {
    let (c, p) = call_child("{ count = 1, extra = 2 }");

    assert_eq!(
        rows_of(&[("child", &c), ("parent", &p)]),
        [row("unknown_param", path("parent"), pos_of(&p, "extra"))]
    );
}

/// REQ-B-10: a value of the wrong type is `param_wrong_type` at the value.
#[test]
fn call_site_wrong_type() {
    let (c, p) = call_child("{ count = \"many\" }");

    assert_eq!(
        rows_of(&[("child", &c), ("parent", &p)]),
        [row(
            "param_wrong_type",
            path("parent"),
            pos_of(&p, "\"many\"")
        )]
    );
}

/// REQ-B-10: a value outside the interface's range is `param_out_of_range` at the value.
#[test]
fn call_site_out_of_range() {
    let (c, p) = call_child("{ count = 0 }");

    assert_eq!(
        rows_of(&[("child", &c), ("parent", &p)]),
        [row(
            "param_out_of_range",
            path("parent"),
            pos_after(&p, "count = ")
        )]
    );
}

/// REQ-B-10: an omitted optional param is filled from its default (no error).
#[test]
fn defaults_fill_missing_optional_params() {
    let (c, p) = call_child("{ count = 1 }");

    assert_eq!(
        ok(graphs(&files(&[("child", &c), ("parent", &p)]))).len(),
        2
    );
}

/// ASM-076: a `{ param = .. }` reference to a param the enclosing graph does not declare is
/// `invalid_binding` at the reference.
#[test]
fn param_ref_to_undeclared_param_is_invalid_binding() {
    let c = child("test.graph.child");
    let p = caller(
        "test.graph.parent",
        "test.graph.child",
        None,
        "{ count = { param = \"b\" } }",
        &[
            "[interface]",
            "params = { a = { type = \"integer\", default = 1 } }",
        ],
    );

    let d = found(&[("child", &c), ("parent", &p)]);

    assert_eq!(
        rows(&d),
        [row(
            "invalid_binding",
            path("parent"),
            pos_of(&p, "{ param = \"b\" }")
        )]
    );
    assert!(d[0].message.contains('b'), "{}", d[0].message);
}

/// ASM-076: the referenced param's type must be compatible with the target property (an `integer`
/// satisfies `number`, a `string` does not satisfy `integer`).
#[test]
fn param_ref_type_mismatch_is_invalid_binding() {
    let c = child("test.graph.child");
    let p = caller(
        "test.graph.parent",
        "test.graph.child",
        None,
        "{ count = { param = \"s\" } }",
        &[
            "[interface]",
            "params = { s = { type = \"string\", default = \"a\" } }",
        ],
    );

    assert_eq!(
        rows_of(&[("child", &c), ("parent", &p)]),
        [row(
            "invalid_binding",
            path("parent"),
            pos_of(&p, "{ param = \"s\" }")
        )]
    );
}

/// ASM-076: a value substituted into a leaf that its schema rejects is reported at the call site
/// value that supplied it, with a note at the `{ param }` reference inside the called graph.
#[test]
fn substituted_value_out_of_leaf_range_reported_at_call_site_with_note_at_ref() {
    let mid = lines(&[
        "schema_version = 1",
        "id = \"test.graph.mid\"",
        "version = \"1.0.0\"",
        "root = \"r\"",
        "[interface]",
        "params = { range = { type = \"number\", default = 10.0 } }",
        "[nodes.r]",
        "type = \"condition\"",
        "ref = \"stub.cond.near\"",
        "params = { range_m = { param = \"range\" } }",
    ]);
    let outer = caller(
        "test.graph.outer",
        "test.graph.mid",
        None,
        "{ range = 500000.0 }",
        &[],
    );

    let d = found(&[("mid", &mid), ("outer", &outer)]);

    assert_eq!(
        rows(&d),
        [row(
            "param_out_of_range",
            path("outer"),
            pos_of(&outer, "500000.0")
        )]
    );
    let note = d[0]
        .notes
        .iter()
        .find_map(|n| n.location.as_ref())
        .unwrap_or_else(|| panic!("a located note"));
    assert_eq!(
        (note.path.clone(), (note.line, note.column)),
        (path("mid").into(), pos_of(&mid, "{ param = \"range\" }"))
    );
}

/// ADR 0014: output bindings are checked: an unknown port (at the port name), a parent key of the
/// wrong type or one that is not declared (at the key value) are `invalid_binding`.
#[test]
fn output_port_unknown_or_type_mismatch_is_invalid_binding() {
    let c = lines(&[
        "schema_version = 1",
        "id = \"test.graph.child\"",
        "version = \"1.0.0\"",
        "root = \"r\"",
        "[interface]",
        "outputs = { found = { type = \"integer\", from = \"n\" } }",
        "[blackboard]",
        "n = \"integer\"",
        "[nodes.r]",
        "type = \"action\"",
        "ref = \"stub.action.noop\"",
    ]);
    let with = |outputs: &str, key_type: &str| {
        lines(&[
            "schema_version = 1",
            "id = \"test.graph.parent\"",
            "version = \"1.0.0\"",
            "root = \"r\"",
            "[blackboard]",
            &format!("k = \"{key_type}\""),
            "[nodes.r]",
            "type = \"subgraph\"",
            "ref = \"test.graph.child\"",
            &format!("outputs = {outputs}"),
        ])
    };
    let unknown_port = with("{ nope = \"k\" }", "integer");
    let wrong_type = with("{ found = \"k\" }", "string");
    let undeclared = with("{ found = \"ghost\" }", "integer");
    let fine = with("{ found = \"k\" }", "integer");

    assert_eq!(
        rows_of(&[("child", &c), ("parent", &unknown_port)]),
        [row(
            "invalid_binding",
            path("parent"),
            pos_of(&unknown_port, "nope")
        )]
    );
    assert_eq!(
        rows_of(&[("child", &c), ("parent", &wrong_type)]),
        [row(
            "invalid_binding",
            path("parent"),
            pos_of(&wrong_type, "\"k\"")
        )]
    );
    assert_eq!(
        rows_of(&[("child", &c), ("parent", &undeclared)]),
        [row(
            "invalid_binding",
            path("parent"),
            pos_of(&undeclared, "\"ghost\"")
        )]
    );
    assert_eq!(
        ok(graphs(&files(&[("child", &c), ("parent", &fine)]))).len(),
        2
    );
}

/// ASM-088: a parent key can be the target of at most one output binding across a graph's subgraph
/// nodes; the second binding is `invalid_binding`, at its value.
#[test]
fn parent_key_bound_twice_is_invalid_binding() {
    let c = lines(&[
        "schema_version = 1",
        "id = \"test.graph.child\"",
        "version = \"1.0.0\"",
        "root = \"r\"",
        "[interface]",
        "outputs = { found = { type = \"integer\", from = \"n\" } }",
        "[blackboard]",
        "n = \"integer\"",
        "[nodes.r]",
        "type = \"action\"",
        "ref = \"stub.action.noop\"",
    ]);
    let p = lines(&[
        "schema_version = 1",
        "id = \"test.graph.parent\"",
        "version = \"1.0.0\"",
        "root = \"seq\"",
        "[blackboard]",
        "shared = \"integer\"",
        "[nodes.seq]",
        "type = \"sequence\"",
        "children = [\"one\", \"two\"]",
        "[nodes.one]",
        "type = \"subgraph\"",
        "ref = \"test.graph.child\"",
        "outputs = { found = \"shared\" }",
        "[nodes.two]",
        "type = \"subgraph\"",
        "ref = \"test.graph.child\"",
        "outputs = { found = \"shared\" }",
    ]);

    assert_eq!(
        rows_of(&[("child", &c), ("parent", &p)]),
        [row(
            "invalid_binding",
            path("parent"),
            pos_of_nth(&p, "\"shared\"", 1)
        )]
    );
}

/// ADR 0014: a leaf param that names a blackboard key (`target_key`, `store_as`) must name a
/// declared key (or an interface param) of the right type: otherwise `invalid_binding` at the
/// string.
#[test]
fn blackboard_param_naming_undeclared_key_is_invalid_binding() {
    let leaf = |kind: &str, id: &str, params: &str, head: &[&str]| {
        let mut all = vec![
            "schema_version = 1".to_string(),
            "id = \"test.graph.keys\"".to_string(),
            "version = \"1.0.0\"".to_string(),
            "root = \"r\"".to_string(),
        ];
        all.extend(head.iter().map(|h| (*h).to_string()));
        all.extend([
            "[nodes.r]".to_string(),
            format!("type = \"{kind}\""),
            format!("ref = \"{id}\""),
            format!("params = {params}"),
        ]);
        let refs: Vec<&str> = all.iter().map(String::as_str).collect();
        lines(&refs)
    };
    let undeclared_read = leaf(
        "condition",
        "stub.cond.has",
        "{ target_key = \"missing\" }",
        &[],
    );
    let undeclared_write = leaf(
        "condition",
        "stub.cond.near",
        "{ range_m = 5.0, store_as = \"missing\" }",
        &[],
    );
    let wrong_type = leaf(
        "condition",
        "stub.cond.has",
        "{ target_key = \"t\" }",
        &["[blackboard]", "t = \"number\""],
    );
    let declared = leaf(
        "condition",
        "stub.cond.has",
        "{ target_key = \"t\" }",
        &["[blackboard]", "t = \"entity\""],
    );
    let param_key = leaf(
        "condition",
        "stub.cond.has",
        "{ target_key = \"t\" }",
        &["[interface]", "params = { t = { type = \"entity\" } }"],
    );

    assert_eq!(
        rows_of(&[("g", &undeclared_read)]),
        [row(
            "invalid_binding",
            path("g"),
            pos_of(&undeclared_read, "\"missing\"")
        )]
    );
    assert_eq!(
        rows_of(&[("g", &undeclared_write)]),
        [row(
            "invalid_binding",
            path("g"),
            pos_of(&undeclared_write, "\"missing\"")
        )]
    );
    assert_eq!(
        rows_of(&[("g", &wrong_type)]),
        [row(
            "invalid_binding",
            path("g"),
            pos_of(&wrong_type, "\"t\"")
        )]
    );
    assert_eq!(ok(graphs(&files(&[("g", &declared)]))).len(), 1);
    // The root graph has a required binding-only param, which is fine for a graph used as a
    // subgraph; as a loaded graph on its own it is only an interface.
    assert_eq!(ok(graphs(&files(&[("g", &param_key)]))).len(), 1);
}

// --- AT-8: cycles, depth, tree shape, many errors ----------------------------------------------

fn calls(id: &str, target: &str) -> String {
    caller(id, target, None, "", &[])
}

/// REQ-B-12: a graph that calls itself is a `subgraph_cycle`, at its own ref.
#[test]
fn self_reference_is_subgraph_cycle() {
    let g = calls("test.graph.me", "test.graph.me");

    let d = found(&[("me", &g)]);

    assert_eq!(
        rows(&d),
        [row(
            "subgraph_cycle",
            path("me"),
            pos_of_nth(&g, "\"test.graph.me\"", 1)
        )]
    );
}

/// REQ-B-12: a two-graph cycle is reported once, at the ref of the smallest ID in it, with a note
/// per further edge (the plan's example: message "subgraph cycle: `a` -> `b` -> `a`").
#[test]
fn two_graph_cycle_reported_once_with_note_per_edge() {
    let a = calls("test.graph.a", "test.graph.b");
    let b = calls("test.graph.b", "test.graph.a");

    let d = found(&[("a", &a), ("b", &b)]);

    assert_eq!(
        rows(&d),
        [row(
            "subgraph_cycle",
            path("a"),
            pos_of(&a, "\"test.graph.b\"")
        )]
    );
    assert_eq!(
        d[0].message,
        "subgraph cycle: `test.graph.a` -> `test.graph.b` -> `test.graph.a`"
    );
    assert_eq!(d[0].notes.len(), 1);
    let at = d[0].notes[0]
        .location
        .as_ref()
        .unwrap_or_else(|| panic!("located note"));
    assert_eq!(
        (at.path.clone(), (at.line, at.column)),
        (path("b").into(), pos_of(&b, "\"test.graph.a\""))
    );
}

/// REQ-B-12: a three-graph cycle (`c -> a -> b -> c`) is reported once, at the smallest ID `a`,
/// whatever the file order.
#[test]
fn three_graph_cycle_reported_at_smallest_id() {
    let a = calls("test.graph.a", "test.graph.b");
    let b = calls("test.graph.b", "test.graph.c");
    let c = calls("test.graph.c", "test.graph.a");

    for order in [[0, 1, 2], [2, 1, 0], [1, 2, 0]] {
        let all = [("a", &a), ("b", &b), ("c", &c)];
        let given: Vec<(&str, &str)> = order
            .iter()
            .map(|&i| (all[i].0, all[i].1.as_str()))
            .collect();

        let d = found(&given);

        assert_eq!(
            rows(&d),
            [row(
                "subgraph_cycle",
                path("a"),
                pos_of(&a, "\"test.graph.b\"")
            )],
            "{order:?}"
        );
        assert_eq!(d[0].notes.len(), 2, "{order:?}");
    }
}

/// ASM-092: a cycle is reported once per strongly connected component, not once per cycle. Here
/// `a -> b -> a` and `a -> c -> a` share the component {a, b, c}: one diagnostic, at the first
/// ref of the smallest ID, printing the shortest cycle through it (ties broken by target ID).
#[test]
fn two_cycles_in_one_component_are_reported_once() {
    let a = lines(&[
        "schema_version = 1",
        "id = \"test.graph.a\"",
        "version = \"1.0.0\"",
        "root = \"s\"",
        "[nodes.s]",
        "type = \"sequence\"",
        "children = [\"to_b\", \"to_c\"]",
        "[nodes.to_b]",
        "type = \"subgraph\"",
        "ref = \"test.graph.b\"",
        "[nodes.to_c]",
        "type = \"subgraph\"",
        "ref = \"test.graph.c\"",
    ]);
    let b = calls("test.graph.b", "test.graph.a");
    let c = calls("test.graph.c", "test.graph.a");

    let d = found(&[("a", &a), ("b", &b), ("c", &c)]);

    assert_eq!(
        rows(&d),
        [row(
            "subgraph_cycle",
            path("a"),
            pos_of(&a, "\"test.graph.b\"")
        )]
    );
    assert_eq!(
        d[0].message,
        "subgraph cycle: `test.graph.a` -> `test.graph.b` -> `test.graph.a`"
    );
}

/// REQ-B-12: a graph that calls itself from under an FSM state or under a decorator is still a
/// `subgraph_cycle`, at that ref (a guard cannot hold a subgraph: ADR 0012 allows only
/// conditions, inverters, sequences and selectors there).
#[test]
fn self_call_under_fsm_state_or_inverter_is_subgraph_cycle() {
    let under_fsm = lines(&[
        "schema_version = 1",
        "id = \"test.graph.me\"",
        "version = \"1.0.0\"",
        "root = \"m\"",
        "[nodes.m]",
        "type = \"fsm\"",
        "initial = \"a\"",
        "states = { a = \"call\" }",
        "[nodes.call]",
        "type = \"subgraph\"",
        "ref = \"test.graph.me\"",
    ]);
    let under_inverter = lines(&[
        "schema_version = 1",
        "id = \"test.graph.me\"",
        "version = \"1.0.0\"",
        "root = \"i\"",
        "[nodes.i]",
        "type = \"inverter\"",
        "child = \"call\"",
        "[nodes.call]",
        "type = \"subgraph\"",
        "ref = \"test.graph.me\"",
    ]);

    for text in [&under_fsm, &under_inverter] {
        assert_eq!(
            rows_of(&[("me", text)]),
            [row(
                "subgraph_cycle",
                path("me"),
                pos_of_nth(text, "\"test.graph.me\"", 1)
            )]
        );
    }
}

/// ASM-056: a graph that only calls a graph on a cycle gets no error of its own.
#[test]
fn graph_depending_on_cycle_has_no_cascade() {
    let a = calls("test.graph.a", "test.graph.b");
    let b = calls("test.graph.b", "test.graph.a");
    let d_graph = calls("test.graph.d", "test.graph.a");

    let diagnostics = found(&[("a", &a), ("b", &b), ("d", &d_graph)]);

    assert_eq!(
        diagnostics
            .iter()
            .map(|d| d.code.as_str())
            .collect::<Vec<_>>(),
        ["subgraph_cycle"]
    );
}

/// One graph per chain link: `test.g01` calls `test.g02`, and so on; the last one is a leaf graph.
fn chain(len: usize) -> Vec<(String, String)> {
    (1..=len)
        .map(|i| {
            let id = format!("test.g{i:02}");
            let text = if i == len {
                minimal(&id)
            } else {
                calls(&id, &format!("test.g{:02}", i + 1))
            };
            (format!("g{i:02}"), text)
        })
        .collect()
}

fn chain_files(chain: &[(String, String)]) -> Vec<(&str, &str)> {
    chain
        .iter()
        .map(|(n, t)| (n.as_str(), t.as_str()))
        .collect()
}

/// ASM-073: a chain of 16 graphs (the root included) is accepted, 17 is `subgraph_too_deep`,
/// reported at the ref of the node that crosses the limit (the 16th graph's call).
#[test]
fn depth_16_accepted_17_rejected() {
    let ok16 = chain(16);
    let bad17 = chain(17);

    assert_eq!(ok(graphs(&files(&chain_files(&ok16)))).len(), 16);
    let d = found(&chain_files(&bad17));
    assert_eq!(
        rows(&d),
        [row(
            "subgraph_too_deep",
            path("g16"),
            pos_of(&bad17[15].1, "\"test.g17\"")
        )]
    );
}

/// ASM-073: the limit is `LoaderOptions::max_subgraph_depth`.
#[test]
fn depth_limit_is_configurable() {
    let three = chain(3);
    let four = chain(4);

    assert_eq!(
        ok(graphs_with_depth(&files(&chain_files(&three)), 3)).len(),
        3
    );
    let d = errors(graphs_with_depth(&files(&chain_files(&four)), 3));
    assert_eq!(
        rows(&d),
        [row(
            "subgraph_too_deep",
            path("g03"),
            pos_of(&four[2].1, "\"test.g04\"")
        )]
    );
    assert_eq!(
        ok(graphs(&files(&chain_files(&four)))).len(),
        4,
        "the default limit is 16"
    );
}

/// REQ-T-1: an unknown leaf ref is `unknown_id` at the ref; a leaf of another kind says so.
#[test]
fn unknown_leaf_ref_is_unknown_id() {
    let missing = lines(&[
        "schema_version = 1",
        "id = \"test.graph.leaf\"",
        "version = \"1.0.0\"",
        "root = \"r\"",
        "[nodes.r]",
        "type = \"condition\"",
        "ref = \"stub.cond.missing\"",
    ]);
    let wrong_kind = missing.replace("stub.cond.missing", "stub.action.noop");

    let d = found(&[("g", &missing)]);
    let k = found(&[("g", &wrong_kind)]);

    assert_eq!(
        rows(&d),
        [row(
            "unknown_id",
            path("g"),
            pos_of(&missing, "\"stub.cond.missing\"")
        )]
    );
    assert!(
        d[0].notes
            .iter()
            .any(|n| n.message.contains("stub.cond.flag")),
        "{:?}",
        d[0].notes
    );
    assert_eq!(
        rows(&k),
        [row(
            "unknown_id",
            path("g"),
            pos_of(&wrong_kind, "\"stub.action.noop\"")
        )]
    );
    assert_eq!(
        k[0].message,
        "`stub.action.noop` is an action, not a condition"
    );
}

/// ASM-075: an unknown child, a node with two parents, a node cycle and an orphan node are four
/// distinct errors, all reported, each at its reference (the orphan at its table name).
#[test]
fn unknown_node_child_reused_orphan_and_node_cycle_reported() {
    let text = lines(&[
        "schema_version = 1",
        "id = \"test.graph.shape\"",
        "version = \"1.0.0\"",
        "root = \"root\"",
        "[nodes.root]",
        "type = \"sequence\"",
        "children = [\"a\", \"b\", \"ghost\", \"loop1\"]",
        "[nodes.a]",
        "type = \"sequence\"",
        "children = [\"shared\"]",
        "[nodes.b]",
        "type = \"sequence\"",
        "children = [\"shared\"]",
        "[nodes.shared]",
        "type = \"action\"",
        "ref = \"stub.action.noop\"",
        "[nodes.loop1]",
        "type = \"sequence\"",
        "children = [\"loop2\"]",
        "[nodes.loop2]",
        "type = \"sequence\"",
        "children = [\"loop1\"]",
        "[nodes.orphan]",
        "type = \"action\"",
        "ref = \"stub.action.noop\"",
    ]);

    assert_eq!(
        rows_of(&[("g", &text)]),
        [
            row("unknown_node", path("g"), pos_of(&text, "\"ghost\"")),
            row("node_reused", path("g"), pos_of_nth(&text, "\"shared\"", 1)),
            row("node_cycle", path("g"), pos_of_nth(&text, "\"loop1\"", 1)),
            row("unreachable_node", path("g"), pos_of(&text, "orphan")),
        ]
    );
}

/// Three graph files with five independent errors (two in `a`, one in `b`, two in `c`).
fn five_errors() -> [(String, String); 3] {
    let a = lines(&[
        "schema_version = 1",
        "id = \"test.a\"",
        "version = \"1.0.0\"",
        "root = \"s\"",
        "[nodes.s]",
        "type = \"sequence\"",
        "children = [\"near\", \"gone\"]",
        "[nodes.near]",
        "type = \"condition\"",
        "ref = \"stub.cond.near\"",
        "params = { range_m = 0.0 }",
        "[nodes.gone]",
        "type = \"action\"",
        "ref = \"stub.action.missing\"",
    ]);
    let b = caller(
        "test.b",
        "stub.behavior.ext",
        Some("^9"),
        "{ target = { key = \"t\" } }",
        &["[blackboard]", "t = \"entity\""],
    );
    let c = lines(&[
        "schema_version = 1",
        "id = \"test.c\"",
        "version = \"1.0.0\"",
        "root = \"s\"",
        "[blackboard]",
        "x = \"entity\"",
        "[nodes.s]",
        "type = \"sequence\"",
        "children = [\"has\", \"ghost\"]",
        "[nodes.has]",
        "type = \"condition\"",
        "ref = \"stub.cond.has\"",
        "params = { target_key = \"y\" }",
    ]);
    [("a".into(), a), ("b".into(), b), ("c".into(), c)]
}

/// REQ-T-2, REQ-B-12: five independent errors in three files are all reported, in the stable order
/// (path, then position).
#[test]
fn five_independent_errors_in_three_graph_files_all_reported_in_stable_order() {
    let [(_, a), (_, b), (_, c)] = five_errors();

    assert_eq!(
        rows_of(&[("a", &a), ("b", &b), ("c", &c)]),
        [
            row("param_out_of_range", path("a"), pos_after(&a, "range_m = ")),
            row(
                "unknown_id",
                path("a"),
                pos_of(&a, "\"stub.action.missing\"")
            ),
            row("version_mismatch", path("b"), pos_of(&b, "\"^9\"")),
            row("unknown_node", path("c"), pos_of(&c, "\"ghost\"")),
            row("invalid_binding", path("c"), pos_of(&c, "\"y\"")),
        ]
    );
}

/// REQ-K-4: the order of the files does not change the diagnostics: all six orders of the three
/// files give equal lists (notes included).
#[test]
fn shuffled_graph_files_give_identical_diagnostics() {
    let all = five_errors();
    let orders = [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ];
    let reference = {
        let given: Vec<(&str, &str)> = all.iter().map(|(n, t)| (n.as_str(), t.as_str())).collect();
        found(&given)
    };

    for order in orders {
        let given: Vec<(&str, &str)> = order
            .iter()
            .map(|&i| (all[i].0.as_str(), all[i].1.as_str()))
            .collect();

        assert_eq!(found(&given), reference, "{order:?}");
    }
}

// --- ids, cascades -----------------------------------------------------------------------------

/// ASM-056 / ADR 0012: two files with one graph ID: `duplicate_id` at the second `id` value, with a
/// note at the first definition.
#[test]
fn duplicate_graph_id_across_files_is_duplicate_id_with_note() {
    let one = minimal("test.graph.same");

    let d = found(&[("a", &one), ("b", &one)]);

    assert_eq!(
        rows(&d),
        [row(
            "duplicate_id",
            path("b"),
            pos_of(&one, "\"test.graph.same\"")
        )]
    );
    let at = d[0].notes[0]
        .location
        .as_ref()
        .unwrap_or_else(|| panic!("located note"));
    assert_eq!(
        (at.path.clone(), (at.line, at.column)),
        (path("a").into(), pos_of(&one, "\"test.graph.same\""))
    );
}

/// ASM-063: a graph ID is global: one that equals a registered component (here an action) is a
/// `duplicate_id`, with a note saying what is registered.
#[test]
fn graph_id_equal_to_a_registered_component_is_duplicate_id() {
    let g = minimal("stub.action.noop");

    let d = found(&[("g", &g)]);

    assert_eq!(
        rows(&d),
        [row(
            "duplicate_id",
            path("g"),
            pos_of(&g, "\"stub.action.noop\"")
        )]
    );
    assert!(
        d[0].notes.iter().any(|n| n.message.contains("action")),
        "{:?}",
        d[0].notes
    );
}

/// ASM-056: a graph whose file has a syntax error is "broken"; graphs that call it get no
/// cascade errors (no `unknown_id`).
#[test]
fn graph_in_a_file_with_a_syntax_error_gives_dependents_no_cascade() {
    let broken = format!("{}this is not toml\n", minimal("test.graph.broken"));
    let user = calls("test.graph.user", "test.graph.broken");

    let d = found(&[("broken", &broken), ("user", &user)]);

    assert_eq!(
        d.iter().map(|d| d.code.as_str()).collect::<Vec<_>>(),
        ["syntax"]
    );
}

/// ASM-056 (the other direction): a dependent of a broken graph still reports its own, independent
/// errors.
#[test]
fn dependents_of_a_broken_graph_still_report_their_own_errors() {
    let broken = format!("{}this is not toml\n", minimal("test.graph.broken"));
    let user = lines(&[
        "schema_version = 1",
        "id = \"test.graph.user\"",
        "version = \"1.0.0\"",
        "root = \"s\"",
        "[nodes.s]",
        "type = \"sequence\"",
        "children = [\"call\", \"gone\"]",
        "[nodes.call]",
        "type = \"subgraph\"",
        "ref = \"test.graph.broken\"",
        "[nodes.gone]",
        "type = \"action\"",
        "ref = \"stub.action.missing\"",
    ]);

    let d = found(&[("broken", &broken), ("user", &user)]);

    assert_eq!(
        d.iter().map(|d| d.code.as_str()).collect::<Vec<_>>(),
        ["syntax", "unknown_id"]
    );
}

// --- AT-11: leaf parameters --------------------------------------------------------------------

fn near(params: &str) -> String {
    lines(&[
        "schema_version = 1",
        "id = \"test.graph.near\"",
        "version = \"1.0.0\"",
        "root = \"r\"",
        "[nodes.r]",
        "type = \"condition\"",
        "ref = \"stub.cond.near\"",
        &format!("params = {params}"),
    ])
}

/// REQ-T-3, REQ-T-2: leaf params are validated against the leaf's schema, each error located in the
/// graph file (missing at the table, unknown at the key, wrong type and range at the value).
#[test]
fn bad_leaf_params_located_in_graph_file() {
    let missing = near("{ }");
    let unknown = near("{ range_m = 5.0, bogus = 1 }");
    let wrong = near("{ range_m = true }");
    let range = near("{ range_m = -5.0 }");

    assert_eq!(
        rows_of(&[("g", &missing)]),
        [row(
            "missing_param",
            path("g"),
            pos_after(&missing, "params = ")
        )]
    );
    assert_eq!(
        rows_of(&[("g", &unknown)]),
        [row("unknown_param", path("g"), pos_of(&unknown, "bogus"))]
    );
    assert_eq!(
        rows_of(&[("g", &wrong)]),
        [row("param_wrong_type", path("g"), pos_of(&wrong, "true"))]
    );
    assert_eq!(
        rows_of(&[("g", &range)]),
        [row(
            "param_out_of_range",
            path("g"),
            pos_after(&range, "range_m = ")
        )]
    );
}

/// REQ-B-15: there is no expression language: a string where a number is expected is a type
/// error, not something evaluated.
#[test]
fn expression_string_is_param_wrong_type() {
    let text = near("{ range_m = \"2 * 100\" }");

    assert_eq!(
        rows_of(&[("g", &text)]),
        [row(
            "param_wrong_type",
            path("g"),
            pos_of(&text, "\"2 * 100\"")
        )]
    );
}

// --- AT-20: input bindings, load-time rows -----------------------------------------------------

const TARGET_CHILD: &str = "params = { target = { type = \"entity\" } }";

fn bound(binding: &str, blackboard: &[&str]) -> (String, String) {
    (
        with_interface("test.graph.child", TARGET_CHILD),
        caller(
            "test.graph.parent",
            "test.graph.child",
            None,
            &format!("{{ target = {binding} }}"),
            blackboard,
        ),
    )
}

/// ADR 0014: a `{ key }` binding to a key the calling graph does not declare is `invalid_binding`
/// at the binding.
#[test]
fn binding_to_undeclared_parent_key_is_invalid_binding() {
    let (c, p) = bound("{ key = \"ghost\" }", &["[blackboard]", "t = \"entity\""]);

    assert_eq!(
        rows_of(&[("child", &c), ("parent", &p)]),
        [row(
            "invalid_binding",
            path("parent"),
            pos_of(&p, "{ key = \"ghost\" }")
        )]
    );
}

/// ADR 0014: the key's type must equal the param's type: a `number` key for an `entity` param is
/// `invalid_binding`.
#[test]
fn binding_type_mismatch_is_invalid_binding() {
    let (c, p) = bound("{ key = \"t\" }", &["[blackboard]", "t = \"number\""]);

    assert_eq!(
        rows_of(&[("child", &c), ("parent", &p)]),
        [row(
            "invalid_binding",
            path("parent"),
            pos_of(&p, "{ key = \"t\" }")
        )]
    );
}

/// ADR 0014: an `integer` key may feed a `number` param (and the same type binds, of course).
#[test]
fn integer_key_may_feed_number_param() {
    let c = with_interface(
        "test.graph.child",
        "params = { speed = { type = \"number\", default = 1.0 } }",
    );
    let p = caller(
        "test.graph.parent",
        "test.graph.child",
        None,
        "{ speed = { key = \"n\" } }",
        &["[blackboard]", "n = \"integer\""],
    );

    assert_eq!(
        ok(graphs(&files(&[("child", &c), ("parent", &p)]))).len(),
        2
    );
}

/// ADR 0014: a param that the called graph uses in a `{ param }` reference is needed at build time
/// and cannot be bound: `invalid_binding` at the binding, with a note at the reference.
#[test]
fn binding_a_param_used_as_param_reference_is_invalid_binding_with_note() {
    let c = lines(&[
        "schema_version = 1",
        "id = \"test.graph.child\"",
        "version = \"1.0.0\"",
        "root = \"r\"",
        "[interface]",
        "params = { range = { type = \"number\", default = 10.0 } }",
        "[nodes.r]",
        "type = \"condition\"",
        "ref = \"stub.cond.near\"",
        "params = { range_m = { param = \"range\" } }",
    ]);
    let p = caller(
        "test.graph.parent",
        "test.graph.child",
        None,
        "{ range = { key = \"n\" } }",
        &["[blackboard]", "n = \"number\""],
    );

    let d = found(&[("child", &c), ("parent", &p)]);

    assert_eq!(
        rows(&d),
        [row(
            "invalid_binding",
            path("parent"),
            pos_of(&p, "{ key = \"n\" }")
        )]
    );
    let note = d[0]
        .notes
        .iter()
        .find_map(|n| n.location.as_ref())
        .unwrap_or_else(|| panic!("located note"));
    assert_eq!(
        (note.path.clone(), (note.line, note.column)),
        (path("child").into(), pos_of(&c, "{ param = \"range\" }"))
    );
}

/// ADR 0014: `{ key }` is only allowed as a top-level value of `subgraph` params: in a leaf's
/// params, or nested inside a subgraph param value, it is `invalid_binding` at the form.
#[test]
fn key_form_in_leaf_params_is_invalid_binding() {
    let leaf = lines(&[
        "schema_version = 1",
        "id = \"test.graph.leaf\"",
        "version = \"1.0.0\"",
        "root = \"r\"",
        "[blackboard]",
        "t = \"string\"",
        "[nodes.r]",
        "type = \"condition\"",
        "ref = \"stub.cond.flag\"",
        "params = { name = { key = \"t\" } }",
    ]);
    let c = with_interface(
        "test.graph.child",
        "params = { cfg = { type = \"string\" } }",
    );
    let nested = caller(
        "test.graph.parent",
        "test.graph.child",
        None,
        "{ cfg = { inner = { key = \"t\" } } }",
        &["[blackboard]", "t = \"string\""],
    );

    assert_eq!(
        rows_of(&[("g", &leaf)]),
        [row(
            "invalid_binding",
            path("g"),
            pos_of(&leaf, "{ key = \"t\" }")
        )]
    );
    assert_eq!(
        rows_of(&[("child", &c), ("parent", &nested)]),
        [row(
            "invalid_binding",
            path("parent"),
            pos_of(&nested, "{ key = \"t\" }")
        )]
    );
}

/// ADR 0014: a binding-only param (`entity`, `position`) with no binding at the call site is
/// `missing_param`.
#[test]
fn binding_only_param_without_binding_is_missing_param() {
    let c = with_interface("test.graph.child", TARGET_CHILD);
    let p = caller("test.graph.parent", "test.graph.child", None, "{ }", &[]);

    let d = found(&[("child", &c), ("parent", &p)]);

    assert_eq!(
        rows(&d),
        [row(
            "missing_param",
            path("parent"),
            pos_after(&p, "params = ")
        )]
    );
    assert!(d[0].message.contains("target"), "{}", d[0].message);
}

/// ASM-076: `{ param = "x" }` and `{ key = "x" }` are different forms: in one `subgraph` node they
/// give one build-time reference and one run-time binding; a table with two keys (`{ key = "x",
/// other = 1 }`) is a literal (here a string param rejects it: `param_wrong_type` at the table).
/// ADR 0014 has no object params (ASM-102), so `cfg` is a string and the literal case is the
/// error; the `ok` case is the same call without `cfg`.
#[test]
fn param_and_key_forms_are_distinct() {
    let c = with_interface(
        "test.graph.child",
        "params = { speed = { type = \"number\", default = 1.0 }, cfg = { type = \"string\", default = \"a\" }, target = { type = \"entity\" } }",
    );
    let head = [
        "[interface]",
        "params = { fast = { type = \"number\", default = 9.0 } }",
        "[blackboard]",
        "t = \"entity\"",
    ];
    let both = "{ speed = { param = \"fast\" }, target = { key = \"t\" } }";
    let literal = caller(
        "test.graph.parent",
        "test.graph.child",
        None,
        "{ speed = { param = \"fast\" }, target = { key = \"t\" }, cfg = { key = \"x\", other = 1 } }",
        &head,
    );
    let p = caller("test.graph.parent", "test.graph.child", None, both, &head);

    assert_eq!(
        rows_of(&[("child", &c), ("parent", &literal)]),
        [row(
            "param_wrong_type",
            path("parent"),
            pos_of(&literal, "{ key = \"x\", other = 1 }")
        )],
        "a two-key table is a literal, and a string param rejects it"
    );
    let set = ok(graphs(&files(&[("child", &c), ("parent", &p)])));
    let parent = set
        .iter()
        .find(|g| g.id.as_str() == "test.graph.parent")
        .unwrap_or_else(|| panic!("parent graph"));
    let cgf_core::NodeKind::Subgraph(call) = &parent.nodes[0].kind else {
        panic!("subgraph node");
    };
    assert_eq!(call.inputs.get("target").map(String::as_str), Some("t"));
    assert_eq!(call.inputs.len(), 1);
    assert_eq!(
        call.params.references().len(),
        1,
        "one `{{ param }}` reference"
    );
}

/// ASM-071: a `[blackboard]` key that repeats an interface param name is `duplicate_name`, at the
/// blackboard key (every param is already a read-only key).
#[test]
fn blackboard_key_repeating_a_param_name_is_duplicate_name() {
    let text = lines(&[
        "schema_version = 1",
        "id = \"test.graph.dup\"",
        "version = \"1.0.0\"",
        "root = \"r\"",
        "[interface]",
        "params = { target = { type = \"entity\" } }",
        "[blackboard]",
        "target = \"entity\"",
        "[nodes.r]",
        "type = \"action\"",
        "ref = \"stub.action.noop\"",
    ]);

    assert_eq!(
        rows_of(&[("g", &text)]),
        [row(
            "duplicate_name",
            path("g"),
            pos_of_nth(&text, "target", 1)
        )]
    );
}
