//! WI-12 reader details beyond `graph_file.rs` (AT-1): name grammar of states and exits, name
//! lookups, FSM settings, interface fragments, reserved forms and the order of nodes.
//! Requirements: REQ-B-6, REQ-B-7, REQ-B-12 (reading side), REQ-B-14, REQ-T-2, ADR 0012, ADR 0014,
//! ASM-075, ASM-076, ASM-101.
//!
//! Every assertion checks code, path, line and column (the location of an absent key is left
//! open in the plan, so there only the code and the message are pinned).

#![forbid(unsafe_code)]

mod common;
mod graph_support;

use cgf_core::{BbType, NodeKind, TransitionTarget, Trigger};
use common::{lines, pos_after, pos_of, pos_of_nth, row, src};
use graph_support::{PATH, graph_errors, graph_rows, graphs, minimal, one_graph, with_interface};

/// A graph whose root `m` is the fsm described by `fsm_lines`, with leaf nodes `x`, `y`, `g`.
fn fsm_graph(fsm_lines: &[&str]) -> String {
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
        "[nodes.g]",
        "type = \"condition\"",
        "ref = \"stub.cond.flag\"",
    ]);
    lines(&all)
}

/// Rows of loading one graph file without the shape errors of the set checks (WI-13): the small
/// fsm fixtures here leave nodes and states unused, and these tests are about the reader.
fn reader_rows(text: &str) -> Vec<common::Row> {
    graph_rows(text)
        .into_iter()
        .filter(|r| !matches!(r.0.as_str(), "unreachable_node" | "unreachable_state"))
        .collect()
}

fn code_at(found: &[cgf_templates::Diagnostic], code: &str) -> usize {
    found.iter().filter(|d| d.code.as_str() == code).count()
}

// --- names and lookups -------------------------------------------------------------------------

/// ADR 0012: state and exit names follow the segment grammar, `invalid_value` at the name.
#[test]
fn state_and_exit_names_follow_the_segment_grammar() {
    let text = fsm_graph(&[
        "initial = \"a\"",
        "states = { a = \"x\", Bad = \"y\" }",
        "exits = { Worse = \"success\" }",
    ]);

    let rows = reader_rows(&text);

    assert_eq!(
        rows,
        [
            row("invalid_value", PATH, pos_of(&text, "Bad")),
            row("invalid_value", PATH, pos_of(&text, "Worse")),
        ]
    );
}

/// A child name that no `[nodes.*]` table defines is `unknown_node` at the name (needed to build
/// the `GraphDef`); so is an unknown `root` and an unknown state node.
#[test]
fn unknown_node_names_are_located() {
    let text = lines(&[
        "schema_version = 1",
        "id = \"test.graph.names\"",
        "version = \"1.0.0\"",
        "root = \"nowhere\"",
        "[nodes.r]",
        "type = \"sequence\"",
        "children = [\"ghost\"]",
    ]);

    assert_eq!(
        graph_rows(&text),
        [
            row("unknown_node", PATH, pos_of(&text, "\"nowhere\"")),
            row("unknown_node", PATH, pos_of(&text, "\"ghost\"")),
        ]
    );
}

/// A huge unknown node name is echoed in a bounded way.
#[test]
fn unknown_node_echo_is_bounded() {
    let long = "n".repeat(5000);
    let text = lines(&[
        "schema_version = 1",
        "id = \"test.graph.names\"",
        "version = \"1.0.0\"",
        "root = \"r\"",
        "[nodes.r]",
        "type = \"inverter\"",
        &format!("child = \"{long}\""),
    ]);

    let found = graph_errors(&text);

    assert_eq!(found.len(), 1);
    assert!(found[0].message.len() < 300, "{}", found[0].message.len());
}

/// An unknown node type is `invalid_value` at the value and lists the types; a long one is not
/// echoed in full.
#[test]
fn unknown_node_type_lists_the_types() {
    let long = "t".repeat(5000);
    let text = lines(&[
        "schema_version = 1",
        "id = \"test.graph.type\"",
        "version = \"1.0.0\"",
        "root = \"r\"",
        "[nodes.r]",
        &format!("type = \"{long}\""),
    ]);

    let found = graph_errors(&text);

    assert_eq!(found.len(), 1);
    assert_eq!(found[0].code.as_str(), "invalid_value");
    assert!(
        found[0].message.contains("sequence"),
        "{}",
        found[0].message
    );
    assert!(found[0].message.len() < 500, "{}", found[0].message.len());
}

/// A graph needs a `[nodes]` table.
#[test]
fn missing_nodes_is_missing_key() {
    let text = lines(&[
        "schema_version = 1",
        "id = \"test.graph.empty\"",
        "version = \"1.0.0\"",
        "root = \"r\"",
    ]);

    let found = graph_errors(&text);

    assert_eq!(code_at(&found, "missing_key"), 1, "{found:?}");
    assert!(found[0].message.contains("`nodes`"));
}

/// `GraphDef::nodes` are in file order (not alphabetical), so `NodeId`s follow the file.
#[test]
fn nodes_are_in_file_order() {
    let text = lines(&[
        "schema_version = 1",
        "id = \"test.graph.order\"",
        "version = \"1.0.0\"",
        "root = \"z\"",
        "[nodes.z]",
        "type = \"sequence\"",
        "children = [\"b\", \"a\"]",
        "[nodes.b]",
        "type = \"action\"",
        "ref = \"stub.action.noop\"",
        "[nodes.a]",
        "type = \"action\"",
        "ref = \"stub.action.noop\"",
    ]);

    let def = one_graph(&text);

    let names: Vec<&str> = def.nodes.iter().map(|n| n.name.as_str()).collect();
    assert_eq!(names, ["z", "b", "a"]);
    assert_eq!(def.root.0, 0);
    let NodeKind::Sequence(seq) = &def.nodes[0].kind else {
        panic!("sequence")
    };
    assert_eq!(seq.children.iter().map(|c| c.0).collect::<Vec<_>>(), [1, 2]);
}

// --- fsm ---------------------------------------------------------------------------------------

/// An fsm reads into state and exit indices; transitions use them.
#[test]
fn fsm_reads_states_exits_and_transitions() {
    let text = fsm_graph(&[
        "initial = \"b\"",
        "states = { a = \"x\", b = \"y\" }",
        "exits = { done = \"failure\" }",
        "[[nodes.m.transitions]]",
        "from = \"a\"",
        "to = \"b\"",
        "when = \"g\"",
        "[[nodes.m.transitions]]",
        "from = \"b\"",
        "to = \"done\"",
        "on = \"success\"",
        // WI-13: `a` must be reachable; this transition is appended after the two asserted below.
        "[[nodes.m.transitions]]",
        "from = \"b\"",
        "to = \"a\"",
        "on = \"failure\"",
    ]);

    let def = one_graph(&text);

    let NodeKind::Fsm(fsm) = &def.nodes[0].kind else {
        panic!("fsm")
    };
    assert_eq!(fsm.initial, 1);
    assert_eq!(fsm.transitions[0].to, TransitionTarget::State(1));
    assert!(matches!(fsm.transitions[0].trigger, Trigger::When(_)));
    assert_eq!(fsm.transitions[1].to, TransitionTarget::Exit(0));
    assert_eq!(fsm.exits[0].result, cgf_core::Status::Failure);
}

/// REQ-B-12: an fsm without `initial` is `no_initial_state`; an unknown one is too, at the value.
#[test]
fn fsm_initial_must_name_a_state() {
    let missing = fsm_graph(&["states = { a = \"x\" }"]);
    let unknown = fsm_graph(&["initial = \"zz\"", "states = { a = \"x\" }"]);

    let found = reader_rows(&missing);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].0, "no_initial_state");
    assert_eq!(
        reader_rows(&unknown),
        [row("no_initial_state", PATH, pos_of(&unknown, "\"zz\""))]
    );
}

/// ASM-072: an exit named like a state is `duplicate_name` at the exit.
#[test]
fn exit_named_like_a_state_is_duplicate_name() {
    let text = fsm_graph(&[
        "initial = \"a\"",
        "states = { a = \"x\" }",
        "exits = { a = \"success\" }",
    ]);

    assert_eq!(
        reader_rows(&text),
        [row(
            "duplicate_name",
            PATH,
            pos_of(&text, "a = \"success\"")
        )]
    );
}

/// Transitions: unknown names, both or neither of `when` and `on`, and `on = "running"`.
#[test]
fn transition_errors_are_located() {
    let text = fsm_graph(&[
        "initial = \"a\"",
        "states = { a = \"x\", b = \"y\" }",
        "exits = { done = \"running\" }",
        "[[nodes.m.transitions]]",
        "from = \"q\"",
        "to = \"b\"",
        "when = \"g\"",
        "[[nodes.m.transitions]]",
        "from = \"a\"",
        "to = \"nope\"",
        "on = \"running\"",
        "[[nodes.m.transitions]]",
        "from = \"a\"",
        "to = \"b\"",
    ]);

    assert_eq!(
        graph_rows(&text),
        [
            row("invalid_value", PATH, pos_of_nth(&text, "\"running\"", 0)),
            row("invalid_value", PATH, pos_of(&text, "\"q\"")),
            row("invalid_value", PATH, pos_of(&text, "\"nope\"")),
            row("invalid_value", PATH, pos_of_nth(&text, "\"running\"", 1)),
            row(
                "invalid_value",
                PATH,
                pos_of_nth(&text, "[[nodes.m.transitions]]", 2)
            ),
        ]
    );
    let found = graph_errors(&text);
    assert!(
        found
            .iter()
            .any(|d| d.message.contains("exactly one of `when` or `on`")),
        "{found:?}"
    );
}

/// A transition without `from` or `to` is `missing_key`.
#[test]
fn transition_without_from_and_to_is_missing_key() {
    let text = fsm_graph(&[
        "initial = \"a\"",
        "states = { a = \"x\" }",
        "[[nodes.m.transitions]]",
        "on = \"success\"",
    ]);

    let found = graph_errors(&text);

    assert_eq!(code_at(&found, "missing_key"), 2, "{found:?}");
}

// --- subgraph, params, reserved forms ----------------------------------------------------------

/// A bad version range is `invalid_version` at the value.
#[test]
fn subgraph_version_range_must_be_semver() {
    let text = graph_support::caller("test.graph.p", "stub.behavior.ext", Some("^x"), "", &[]);

    assert_eq!(
        graph_rows(&text),
        [row("invalid_version", PATH, pos_of(&text, "\"^x\""))]
    );
}

/// ADR 0014: `{ key = .. }` is not allowed in a leaf's `params` (`invalid_binding` at the value),
/// nor nested inside a subgraph parameter; `{ param = .. }` is fine in both.
#[test]
fn key_form_outside_a_subgraph_parameter_is_invalid_binding() {
    let leaf = lines(&[
        "schema_version = 1",
        "id = \"test.graph.leaf\"",
        "version = \"1.0.0\"",
        "root = \"r\"",
        "[nodes.r]",
        "type = \"condition\"",
        "ref = \"stub.cond.near\"",
        "params = { range_m = { key = \"k\" } }",
    ]);
    let nested = graph_support::caller(
        "test.graph.nested",
        "stub.behavior.ext",
        None,
        "{ speed = [ { key = \"k\" } ] }",
        &[],
    );

    assert_eq!(
        graph_rows(&leaf),
        [row(
            "invalid_binding",
            PATH,
            pos_of(&leaf, "{ key = \"k\" }")
        )]
    );
    assert_eq!(
        graph_rows(&nested),
        [row(
            "invalid_binding",
            PATH,
            pos_of(&nested, "{ key = \"k\" }")
        )]
    );
}

/// ASM-076: only a one-key table of a string is a reserved form; `{ key = 5 }` is a literal.
///
/// WI-13: the graph is checked against its target now, so the literal shows as an unknown param
/// `cfg` (not an input binding: `target` stays unbound and is missing) instead of a loaded graph.
#[test]
fn key_with_a_non_string_value_is_a_literal() {
    let text = graph_support::caller(
        "test.graph.lit",
        "stub.behavior.ext",
        None,
        "{ cfg = { key = 5 } }",
        &[],
    );

    assert_eq!(
        graph_rows(&text),
        [
            row("missing_param", PATH, pos_after(&text, "params = ")),
            row("unknown_param", PATH, pos_of(&text, "cfg")),
        ]
    );
}

/// A bound key must be a valid blackboard key name.
#[test]
fn bound_key_name_is_checked() {
    let text = graph_support::caller(
        "test.graph.badkey",
        "stub.behavior.ext",
        None,
        "{ target = { key = \"Bad Key\" } }",
        &[],
    );

    assert_eq!(
        graph_rows(&text),
        [row(
            "invalid_value",
            PATH,
            pos_of(&text, "{ key = \"Bad Key\" }")
        )]
    );
}

// --- interface ---------------------------------------------------------------------------------

/// ADR 0014: fragments become one object schema (required = no default); `entity` and `position`
/// are listed in `types` and kept out of the schema.
#[test]
fn interface_fragments_assemble_into_one_schema() {
    let text = with_interface(
        "test.graph.iface",
        "params = { n = { type = \"integer\", minimum = 1 }, flag = { type = \"boolean\", default = true }, target = { type = \"entity\" }, where_to = { type = \"position\", description = \"a place\" } }",
    );

    let def = one_graph(&text);

    let schema = def.interface.params.as_json();
    assert_eq!(schema["additionalProperties"], false);
    assert_eq!(schema["required"], serde_json::json!(["n"]));
    let props = schema["properties"].as_object().map(serde_json::Map::len);
    assert_eq!(props, Some(2), "entity and position are not in the schema");
    assert_eq!(def.interface.types["flag"], BbType::Bool);
    assert_eq!(def.interface.types["target"], BbType::Entity);
    assert_eq!(def.interface.types["where_to"], BbType::Position);
    assert!(def.interface.is_required("n"));
    assert!(def.interface.is_binding_only("target"));
    assert_eq!(def.interface.defaults["flag"], serde_json::json!(true));
}

/// ADR 0014 amendment: a binding-only param cannot have a default (`invalid_value` at it).
#[test]
fn binding_only_param_with_default_is_invalid_value() {
    let text = with_interface(
        "test.graph.iface",
        "params = { target = { type = \"entity\", default = 1 } }",
    );

    assert_eq!(
        graph_rows(&text),
        [row("invalid_value", PATH, pos_after(&text, "default = "))]
    );
}

/// A default that breaks its own fragment is `invalid_value` at the default; a keyword outside the
/// ADR 0009 subset at the keyword; a type outside the six is `invalid_value` at the type.
#[test]
fn interface_fragment_errors_are_located() {
    let bad_default = with_interface(
        "test.graph.iface",
        "params = { n = { type = \"number\", minimum = 1, default = 0 } }",
    );
    let bad_keyword = with_interface(
        "test.graph.iface",
        "params = { n = { type = \"number\", pattern = \"x\" } }",
    );
    let bad_type = with_interface("test.graph.iface", "params = { n = { type = \"object\" } }");
    let no_type = with_interface("test.graph.iface", "params = { n = { minimum = 1 } }");

    assert_eq!(
        graph_rows(&bad_default),
        [row(
            "invalid_value",
            PATH,
            pos_after(&bad_default, "default = ")
        )]
    );
    assert_eq!(
        graph_rows(&bad_keyword),
        [row("invalid_value", PATH, pos_of(&bad_keyword, "pattern"))]
    );
    assert_eq!(
        graph_rows(&bad_type),
        [row("invalid_value", PATH, pos_of(&bad_type, "\"object\""))]
    );
    let found = graph_errors(&no_type);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].code.as_str(), "missing_key");
}

/// Interface parameter names follow the segment grammar.
#[test]
fn interface_param_names_follow_the_segment_grammar() {
    let text = with_interface(
        "test.graph.iface",
        "params = { Speed = { type = \"number\", default = 1.0 } }",
    );

    assert_eq!(
        graph_rows(&text),
        [row("invalid_value", PATH, pos_of(&text, "Speed"))]
    );
}

/// Output ports read into `OutputPort`; a missing `from` is `missing_key`.
#[test]
fn interface_outputs_are_read() {
    let text = lines(&[
        "schema_version = 1",
        "id = \"test.graph.out\"",
        "version = \"1.0.0\"",
        "root = \"r\"",
        "[interface]",
        "outputs = { found = { type = \"entity\", from = \"t\" } }",
        "[blackboard]",
        "t = \"entity\"",
        "[nodes.r]",
        "type = \"action\"",
        "ref = \"stub.action.noop\"",
    ]);
    let missing = text.replace(", from = \"t\"", "");

    let def = one_graph(&text);

    assert_eq!(def.interface.outputs["found"].ty, BbType::Entity);
    assert_eq!(def.interface.outputs["found"].from, "t");
    let found = graph_errors(&missing);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].code.as_str(), "missing_key");
}

// --- sets --------------------------------------------------------------------------------------

/// REQ-K-4: the order the files are given in does not change the result.
#[test]
fn file_order_does_not_change_the_set_or_the_errors() {
    let a = src("b/a.behavior.toml", &minimal("test.graph.a"));
    let b = src("a/b.behavior.toml", &minimal("test.graph.b"));
    let bad = src("c/c.behavior.toml", "schema_version = 1\ncolour = 1\n");

    let forward = graphs(&[a.clone(), b.clone()]);
    let reverse = graphs(&[b, a]);
    let errs_forward = graphs(&[bad.clone(), src("d.behavior.toml", "x")]);
    let errs_reverse = graphs(&[src("d.behavior.toml", "x"), bad]);

    assert_eq!(forward, reverse);
    assert_eq!(errs_forward, errs_reverse);
    assert_eq!(forward.map(|s| s.len()).ok(), Some(2));
}

/// REQ-T-2: errors of several graph files come out in one list, sorted by path.
#[test]
fn errors_of_several_graph_files_are_reported_together() {
    let one = "schema_version = 1\ncolour = 1\n";
    let two = "schema_version = 1\nid = 5\n";

    let found = common::errors(graphs(&[
        src("b.behavior.toml", two),
        src("a.behavior.toml", one),
    ]));

    let paths: Vec<String> = found
        .iter()
        .filter_map(|d| d.location.as_ref().map(|l| l.path.display().to_string()))
        .collect();
    let mut sorted = paths.clone();
    sorted.sort();
    assert_eq!(paths, sorted);
    assert!(paths.iter().any(|p| p == "a.behavior.toml"));
    assert!(paths.iter().any(|p| p == "b.behavior.toml"));
}

// --- review follow-up (WI-12 m4) ---------------------------------------------------------------

/// An unknown key in an output-port table is `unknown_key` at the key.
#[test]
fn unknown_key_in_an_output_port_is_located() {
    let text = lines(&[
        "schema_version = 1",
        "id = \"test.graph.out\"",
        "version = \"1.0.0\"",
        "root = \"r\"",
        "[interface]",
        "outputs = { found = { type = \"entity\", from = \"t\", colour = 1 } }",
        "[blackboard]",
        "t = \"entity\"",
        "[nodes.r]",
        "type = \"action\"",
        "ref = \"stub.action.noop\"",
    ]);

    assert_eq!(
        graph_rows(&text),
        [row("unknown_key", PATH, pos_of(&text, "colour"))]
    );
}

/// An unknown key in a binding-only (`entity`) fragment is `unknown_key` at the key.
#[test]
fn unknown_key_in_a_binding_only_fragment_is_located() {
    let text = with_interface(
        "test.graph.iface",
        "params = { target = { type = \"entity\", minimum = 1 } }",
    );

    assert_eq!(
        graph_rows(&text),
        [row("unknown_key", PATH, pos_of(&text, "minimum"))]
    );
}

/// A `subgraph` node without `ref` is `missing_key` (the place of an absent key is open).
#[test]
fn subgraph_without_ref_is_missing_key() {
    let text = lines(&[
        "schema_version = 1",
        "id = \"test.graph.noref\"",
        "version = \"1.0.0\"",
        "root = \"r\"",
        "[nodes.r]",
        "type = \"subgraph\"",
    ]);

    let found = graph_errors(&text);

    assert_eq!(found.len(), 1);
    assert_eq!(found[0].code.as_str(), "missing_key");
    assert!(found[0].message.contains("`ref`"), "{}", found[0].message);
}
