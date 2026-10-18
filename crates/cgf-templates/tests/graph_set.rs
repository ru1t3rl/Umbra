//! WI-13 additions to the graph set tests: node depth, guards, FSM reachability, interface
//! outputs, cascade suppression in both directions (ASM-056), dry runs, depth on the reference
//! graph and the shared limits.
//! Requirements: REQ-B-9, REQ-B-10, REQ-B-12, REQ-T-2, ADR 0012, ADR 0014, ASM-056, ASM-073,
//! ASM-075, ASM-106.

#![forbid(unsafe_code)]

mod common;
mod graph_support;

use cgf_templates::{Diagnostic, LoaderOptions, SourceFile};
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

fn codes(items: &[(&str, &str)]) -> Vec<&'static str> {
    found(items).iter().map(|d| d.code.as_str()).collect()
}

// --- shared limits -----------------------------------------------------------------------------

/// ASM-106: the loader and the interpreter share the limits through `cgf-core`.
#[test]
fn limits_are_the_shared_constants() {
    assert_eq!(cgf_core::MAX_NODE_DEPTH, 128);
    assert_eq!(cgf_core::DEFAULT_MAX_SUBGRAPH_DEPTH, 16);
    assert_eq!(
        LoaderOptions::default().max_subgraph_depth,
        cgf_core::DEFAULT_MAX_SUBGRAPH_DEPTH
    );
}

// --- node depth --------------------------------------------------------------------------------

/// A graph whose root is the first of `levels` nodes `n1 .. n{levels}`: inverters, the last one an
/// action.
fn nested(levels: usize) -> String {
    let mut all = vec![
        "schema_version = 1".to_string(),
        "id = \"test.graph.deep\"".to_string(),
        "version = \"1.0.0\"".to_string(),
        "root = \"n1\"".to_string(),
    ];
    for i in 1..=levels {
        all.push(format!("[nodes.n{i}]"));
        if i == levels {
            all.push("type = \"action\"".into());
            all.push("ref = \"stub.action.noop\"".into());
        } else {
            all.push("type = \"inverter\"".into());
            all.push(format!("child = \"n{}\"", i + 1));
        }
    }
    let refs: Vec<&str> = all.iter().map(String::as_str).collect();
    lines(&refs)
}

/// ASM-095 d, ASM-106: 128 nested levels are accepted, 129 is `node_too_deep` at the reference
/// that crosses the limit, and the nodes below it cause no `unreachable_node`.
#[test]
fn node_depth_128_accepted_129_rejected_with_one_diagnostic() {
    let at_limit = nested(128);
    let over = nested(129);
    let far_over = nested(400);

    assert_eq!(ok(graphs(&files(&[("g", &at_limit)]))).len(), 1);
    let expected = |text: &str| [row("node_too_deep", path("g"), pos_of(text, "\"n129\""))];
    assert_eq!(rows_of(&[("g", &over)]), expected(&over));
    assert_eq!(rows_of(&[("g", &far_over)]), expected(&far_over));
}

/// A node tree can branch: each branch that crosses the limit is reported at its own crossing.
#[test]
fn node_depth_is_reported_per_branch() {
    // `n1` gets a second child `m1`, the head of a second chain of 130 nodes: two crossings.
    let text = nested(129).replace(
        "[nodes.n1]\ntype = \"inverter\"\nchild = \"n2\"\n",
        "[nodes.n1]\ntype = \"sequence\"\nchildren = [\"n2\", \"m1\"]\n",
    );
    let mut all = vec![text];
    for i in 1..=130 {
        all.push(if i == 130 {
            format!("[nodes.m{i}]\ntype = \"action\"\nref = \"stub.action.noop\"\n")
        } else {
            format!(
                "[nodes.m{i}]\ntype = \"inverter\"\nchild = \"m{}\"\n",
                i + 1
            )
        });
    }
    let text = all.join("\n");

    let d = found(&[("g", &text)]);

    assert_eq!(
        d.iter().map(|d| d.code.as_str()).collect::<Vec<_>>(),
        ["node_too_deep", "node_too_deep"]
    );
}

// --- guards and FSM ------------------------------------------------------------------------------

fn fsm(transitions: &[&str], extra: &[&str]) -> String {
    let mut all = vec![
        "schema_version = 1",
        "id = \"test.graph.fsm\"",
        "version = \"1.0.0\"",
        "root = \"m\"",
        "[nodes.m]",
        "type = \"fsm\"",
        "initial = \"a\"",
        "states = { a = \"x\", b = \"y\", c = \"z\" }",
        "exits = { done = \"success\" }",
        "[nodes.x]",
        "type = \"action\"",
        "ref = \"stub.action.noop\"",
        "[nodes.y]",
        "type = \"action\"",
        "ref = \"stub.action.noop\"",
        "[nodes.z]",
        "type = \"action\"",
        "ref = \"stub.action.noop\"",
    ];
    all.extend_from_slice(transitions);
    all.extend_from_slice(extra);
    lines(&all)
}

fn transition(from: &str, to: &str, trigger: &str) -> [String; 4] {
    [
        "[[nodes.m.transitions]]".to_string(),
        format!("from = \"{from}\""),
        format!("to = \"{to}\""),
        trigger.to_string(),
    ]
}

fn with_transitions(list: &[[String; 4]], extra: &[&str]) -> String {
    let flat: Vec<&str> = list.iter().flatten().map(String::as_str).collect();
    fsm(&flat, extra)
}

/// REQ-B-12: a state reached only through a chain of transitions is reachable; the exit is not a
/// state and needs no incoming edge.
#[test]
fn states_reached_through_a_chain_are_reachable() {
    let text = with_transitions(
        &[
            transition("a", "b", "on = \"success\""),
            transition("b", "c", "on = \"success\""),
            transition("c", "done", "on = \"success\""),
        ],
        &[],
    );

    assert_eq!(ok(graphs(&files(&[("g", &text)]))).len(), 1);
}

/// REQ-B-12: a transition out of an unreachable state does not make its target reachable.
#[test]
fn transition_from_an_unreachable_state_does_not_count() {
    let text = with_transitions(
        &[
            transition("a", "b", "on = \"success\""),
            transition("c", "b", "on = \"failure\""),
        ],
        &[],
    );

    assert_eq!(
        rows_of(&[("g", &text)]),
        [row(
            "unreachable_state",
            path("g"),
            pos_of(&text, "c = \"z\"")
        )]
    );
}

/// ADR 0012: conditions, inverters, sequences and selectors make a valid guard.
#[test]
fn guard_of_conditions_is_valid() {
    let text = with_transitions(
        &[
            transition("a", "b", "when = \"g1\""),
            transition("a", "c", "on = \"failure\""),
        ],
        &[
            "[nodes.g1]",
            "type = \"sequence\"",
            "children = [\"g2\", \"g3\"]",
            "[nodes.g2]",
            "type = \"condition\"",
            "ref = \"stub.cond.flag\"",
            "[nodes.g3]",
            "type = \"inverter\"",
            "child = \"g4\"",
            "[nodes.g4]",
            "type = \"condition\"",
            "ref = \"stub.cond.flag\"",
        ],
    );

    assert_eq!(ok(graphs(&files(&[("g", &text)]))).len(), 1);
}

/// ADR 0012: an action deep inside a guard subtree is `invalid_guard`, at the guard reference.
#[test]
fn action_deep_in_a_guard_is_invalid_guard() {
    let text = with_transitions(
        &[
            transition("a", "b", "when = \"g1\""),
            transition("a", "c", "on = \"failure\""),
        ],
        &[
            "[nodes.g1]",
            "type = \"selector\"",
            "children = [\"g2\"]",
            "[nodes.g2]",
            "type = \"action\"",
            "ref = \"stub.action.noop\"",
        ],
    );

    assert_eq!(
        rows_of(&[("g", &text)]),
        [row("invalid_guard", path("g"), pos_after(&text, "when = "))]
    );
}

/// ASM-075: FSM guards count as parents, so one guard node used by two transitions is
/// `node_reused` at the second.
#[test]
fn a_guard_used_by_two_transitions_is_node_reused() {
    let text = with_transitions(
        &[
            transition("a", "b", "when = \"g\""),
            transition("b", "c", "when = \"g\""),
        ],
        &[
            "[nodes.g]",
            "type = \"condition\"",
            "ref = \"stub.cond.flag\"",
        ],
    );

    assert_eq!(
        rows_of(&[("g", &text)]),
        [row("node_reused", path("g"), pos_of_nth(&text, "\"g\"", 1))]
    );
}

/// ASM-075: a cycle among nodes that `root` cannot reach is `unreachable_node` for each, not a
/// `node_cycle`.
#[test]
fn unreachable_nodes_in_a_loop_are_only_unreachable() {
    let mut text = minimal("test.graph.loose");
    text.push_str(
        "[nodes.p]\ntype = \"inverter\"\nchild = \"q\"\n[nodes.q]\ntype = \"inverter\"\nchild = \"p\"\n",
    );

    let d = found(&[("g", &text)]);

    assert_eq!(
        d.iter().map(|d| d.code.as_str()).collect::<Vec<_>>(),
        ["unreachable_node", "unreachable_node"]
    );
}

/// ASM-075: a child that is the root is a cycle, at the reference.
#[test]
fn root_used_as_a_child_is_node_cycle() {
    let text = lines(&[
        "schema_version = 1",
        "id = \"test.graph.loop\"",
        "version = \"1.0.0\"",
        "root = \"r\"",
        "[nodes.r]",
        "type = \"inverter\"",
        "child = \"r\"",
    ]);

    assert_eq!(
        rows_of(&[("g", &text)]),
        [row("node_cycle", path("g"), pos_after(&text, "child = "))]
    );
}

// --- interface -----------------------------------------------------------------------------------

fn with_output(from: &str, port_type: &str, blackboard: &[&str]) -> String {
    let mut all = vec![
        "schema_version = 1".to_string(),
        "id = \"test.graph.out\"".to_string(),
        "version = \"1.0.0\"".to_string(),
        "root = \"r\"".to_string(),
        "[interface]".to_string(),
        format!("outputs = {{ found = {{ type = \"{port_type}\", from = \"{from}\" }} }}"),
    ];
    all.extend(blackboard.iter().map(|l| (*l).to_string()));
    all.extend([
        "[nodes.r]".to_string(),
        "type = \"action\"".to_string(),
        "ref = \"stub.action.noop\"".to_string(),
    ]);
    let refs: Vec<&str> = all.iter().map(String::as_str).collect();
    lines(&refs)
}

/// ADR 0014: `outputs.from` must be a declared key of the port's type.
#[test]
fn output_from_must_be_a_declared_key_of_the_same_type() {
    let good = with_output("n", "integer", &["[blackboard]", "n = \"integer\""]);
    let undeclared = with_output("ghost", "integer", &["[blackboard]", "n = \"integer\""]);
    let wrong = with_output("n", "string", &["[blackboard]", "n = \"integer\""]);

    assert_eq!(ok(graphs(&files(&[("g", &good)]))).len(), 1);
    assert_eq!(
        rows_of(&[("g", &undeclared)]),
        [row(
            "invalid_binding",
            path("g"),
            pos_of(&undeclared, "\"ghost\"")
        )]
    );
    assert_eq!(
        rows_of(&[("g", &wrong)]),
        [row("invalid_binding", path("g"), pos_of(&wrong, "\"n\""))]
    );
}

/// ADR 0014: a leaf cannot write an interface param (a read-only key).
#[test]
fn leaf_writing_an_interface_param_is_invalid_binding() {
    let text = lines(&[
        "schema_version = 1",
        "id = \"test.graph.write\"",
        "version = \"1.0.0\"",
        "root = \"r\"",
        "[interface]",
        "params = { t = { type = \"entity\" } }",
        "[nodes.r]",
        "type = \"condition\"",
        "ref = \"stub.cond.near\"",
        "params = { range_m = 5.0, store_as = \"t\" }",
    ]);

    let d = found(&[("g", &text)]);

    assert_eq!(
        rows(&d),
        [row("invalid_binding", path("g"), pos_of(&text, "\"t\""))]
    );
    assert!(d[0].message.contains("read-only"), "{}", d[0].message);
}

// --- call sites ----------------------------------------------------------------------------------

/// ADR 0014: a registered behavior whose param is not declared bindable cannot be bound: a note
/// says so.
#[test]
fn binding_an_undeclared_registered_param_is_invalid_binding_with_note() {
    let p = caller(
        "test.graph.parent",
        "stub.behavior.plain",
        None,
        "{ speed = { key = \"n\" } }",
        &["[blackboard]", "n = \"number\""],
    );

    let d = found(&[("parent", &p)]);

    assert_eq!(
        rows(&d),
        [row(
            "invalid_binding",
            path("parent"),
            pos_of(&p, "{ key = \"n\" }")
        )]
    );
    assert!(d[0].notes[0].message.contains("not declared bindable"));
}

/// REQ-T-3: a registered behavior is dry-run through its factory (exactly 13.0 is rejected there,
/// not by the schema); a graph leaf too.
#[test]
fn registered_behavior_and_leaf_are_dry_run() {
    let behavior = caller(
        "test.graph.parent",
        "stub.behavior.plain",
        None,
        "{ speed = 13.0 }",
        &[],
    );
    let leaf = lines(&[
        "schema_version = 1",
        "id = \"test.graph.leaf\"",
        "version = \"1.0.0\"",
        "root = \"r\"",
        "[nodes.r]",
        "type = \"condition\"",
        "ref = \"stub.cond.near\"",
        "params = { range_m = 13.0 }",
    ]);

    assert_eq!(
        rows_of(&[("parent", &behavior)]),
        [row(
            "param_invalid",
            path("parent"),
            pos_after(&behavior, "speed = ")
        )]
    );
    assert_eq!(
        rows_of(&[("leaf", &leaf)]),
        [row(
            "param_invalid",
            path("leaf"),
            pos_after(&leaf, "range_m = ")
        )]
    );
}

/// ASM-076, ASM-106: the default of an interface param that a leaf rejects is reported at the
/// default, with a note at the `{ param }` reference.
#[test]
fn bad_default_used_by_a_leaf_is_reported_at_the_default_with_note() {
    let text = lines(&[
        "schema_version = 1",
        "id = \"test.graph.dflt\"",
        "version = \"1.0.0\"",
        "root = \"r\"",
        "[interface]",
        "params = { range = { type = \"number\", default = 500000.0 } }",
        "[nodes.r]",
        "type = \"condition\"",
        "ref = \"stub.cond.near\"",
        "params = { range_m = { param = \"range\" } }",
    ]);

    let d = found(&[("g", &text)]);

    assert_eq!(
        rows(&d),
        [row(
            "param_out_of_range",
            path("g"),
            pos_of(&text, "500000.0")
        )]
    );
    let note = d[0].notes[0].location.as_ref();
    assert_eq!(
        note.map(|l| (l.line, l.column)),
        Some(pos_of(&text, "{ param = \"range\" }"))
    );
}

/// ASM-106: a `{ param }` whose param has no default is not checked in its own graph (its value is
/// not known), and `integer` params may feed `number` properties.
#[test]
fn reference_to_a_param_without_default_is_not_checked_alone() {
    let text = lines(&[
        "schema_version = 1",
        "id = \"test.graph.req\"",
        "version = \"1.0.0\"",
        "root = \"r\"",
        "[interface]",
        "params = { range = { type = \"integer\" } }",
        "[nodes.r]",
        "type = \"condition\"",
        "ref = \"stub.cond.near\"",
        "params = { range_m = { param = \"range\" } }",
    ]);

    assert_eq!(ok(graphs(&files(&[("g", &text)]))).len(), 1);
}

// --- cascade, both ways (ASM-056) -----------------------------------------------------------------

/// ASM-056: a duplicated ID is reported once; graphs that call it get no `unknown_id` or other
/// error about it, and a graph in a syntax-error file whose ID cannot be recovered is named in a
/// note of the `unknown_id` for it.
#[test]
fn callers_of_a_duplicate_get_no_cascade() {
    let one = minimal("test.graph.same");
    let user = caller(
        "test.graph.user",
        "test.graph.same",
        Some("^9"),
        "{ x = 1 }",
        &[],
    );

    assert_eq!(
        codes(&[("a", &one), ("b", &one), ("user", &user)]),
        ["duplicate_id"]
    );
}

/// ASM-056 (the other direction): the same caller still reports what it states itself.
#[test]
fn callers_of_a_duplicate_still_report_their_own_errors() {
    let one = minimal("test.graph.same");
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
        "ref = \"test.graph.same\"",
        "[nodes.gone]",
        "type = \"action\"",
        "ref = \"stub.action.missing\"",
    ]);

    assert_eq!(
        codes(&[("a", &one), ("b", &one), ("user", &user)]),
        ["duplicate_id", "unknown_id"]
    );
}

/// ASM-056: a graph whose file has an error is not used to check its callers' version or params,
/// but is still known, so there is no `unknown_id` for the call.
#[test]
fn callers_of_a_graph_with_an_error_get_no_cascade() {
    let bad = minimal("test.graph.bad").replace("root = \"r\"\n", "root = \"r\"\ncolour = 1\n");
    let user = caller(
        "test.graph.user",
        "test.graph.bad",
        Some("^9"),
        "{ x = 1 }",
        &[],
    );

    assert_eq!(codes(&[("bad", &bad), ("user", &user)]), ["unknown_key"]);
}

/// ASM-054 rule 3, ASM-056: a file with a syntax error whose `id` is gone is named in the note of
/// an `unknown_id`, because it may define the target.
#[test]
fn unknown_target_notes_files_with_unrecoverable_ids() {
    let broken = "this is not toml\n";
    let user = caller("test.graph.user", "test.graph.lost", None, "", &[]);

    let d = found(&[("broken", broken), ("user", &user)]);

    let unknown = d.iter().find(|d| d.code.as_str() == "unknown_id");
    assert!(
        unknown.is_some_and(|d| d
            .notes
            .iter()
            .any(|n| n.message.contains("broken.behavior.toml"))),
        "{d:?}"
    );
}

// --- depth on the reference graph ------------------------------------------------------------------

/// A graph `id` with one subgraph node per target.
fn calling(id: &str, targets: &[&str]) -> String {
    let mut all = vec![
        "schema_version = 1".to_string(),
        format!("id = \"{id}\""),
        "version = \"1.0.0\"".to_string(),
        "root = \"s\"".to_string(),
        "[nodes.s]".to_string(),
        "type = \"sequence\"".to_string(),
    ];
    let names: Vec<String> = (0..targets.len()).map(|i| format!("\"c{i}\"")).collect();
    all.push(format!("children = [{}]", names.join(", ")));
    for (i, target) in targets.iter().enumerate() {
        all.push(format!("[nodes.c{i}]"));
        all.push("type = \"subgraph\"".into());
        all.push(format!("ref = \"{target}\""));
    }
    let refs: Vec<&str> = all.iter().map(String::as_str).collect();
    lines(&refs)
}

/// ASM-073: the reference that crosses the limit is reported once, even when two arms of a
/// diamond reach the graph that makes the call, and nothing below it is added.
#[test]
fn depth_reports_each_crossing_reference_once() {
    let top = calling("test.g1", &["test.g2", "test.g3"]);
    let left = calling("test.g2", &["test.g4"]);
    let right = calling("test.g3", &["test.g4"]);
    let bottom = calling("test.g4", &["test.g5"]);
    let leaf = minimal("test.g5");
    let all = [
        ("g1", top.as_str()),
        ("g2", left.as_str()),
        ("g3", right.as_str()),
        ("g4", bottom.as_str()),
        ("g5", leaf.as_str()),
    ];
    let given = files(&all);

    let d = errors(graphs_with_depth(&given, 3));

    assert_eq!(
        rows(&d),
        [row(
            "subgraph_too_deep",
            path("g4"),
            pos_of(&bottom, "\"test.g5\"")
        )]
    );
    assert_eq!(ok(graphs_with_depth(&given, 4)).len(), 5);
}

/// ASM-073, ASM-106: graphs on a cycle are not also reported as too deep; a limit of 0 means 1.
#[test]
fn depth_ignores_cycles_and_a_zero_limit_means_one() {
    let a = calling("test.g1", &["test.g2"]);
    let b = calling("test.g2", &["test.g1"]);
    let lone = minimal("test.g3");
    let caller3 = calling("test.g4", &["test.g3"]);
    let given = files(&[("g1", &a), ("g2", &b)]);

    let d = errors(graphs_with_depth(&given, 1));
    let zero = errors(graphs_with_depth(
        &files(&[("g3", &lone), ("g4", &caller3)]),
        0,
    ));

    assert_eq!(
        d.iter().map(|d| d.code.as_str()).collect::<Vec<_>>(),
        ["subgraph_cycle"]
    );
    assert_eq!(
        zero.iter().map(|d| d.code.as_str()).collect::<Vec<_>>(),
        ["subgraph_too_deep"]
    );
}

/// REQ-K-4: a program of any size terminates and is deterministic: a long chain is analysed
/// without recursion.
#[test]
fn a_long_chain_is_analysed_without_deep_recursion() {
    let n = 3000;
    let texts: Vec<(String, String)> = (0..n)
        .map(|i| {
            let id = format!("test.g{i:05}");
            let text = if i + 1 == n {
                minimal(&id)
            } else {
                calling(&id, &[&format!("test.g{:05}", i + 1)])
            };
            (format!("g{i:05}"), text)
        })
        .collect();
    let given: Vec<SourceFile> = texts.iter().map(|(n, t)| src(&path(n), t)).collect();

    let d = errors(graphs_with_depth(&given, 16));

    assert_eq!(d.len(), 1, "one crossing reference");
    assert_eq!(d[0].code.as_str(), "subgraph_too_deep");
}

/// ADR 0014: an interface with binding-only params used by a graph on its own is fine.
#[test]
fn a_graph_with_binding_only_params_loads_alone() {
    let text = with_interface(
        "test.graph.child",
        "params = { target = { type = \"entity\" } }",
    );

    assert_eq!(ok(graphs(&files(&[("g", &text)]))).len(), 1);
}
