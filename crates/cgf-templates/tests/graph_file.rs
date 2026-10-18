//! AT-1 (reading one behavior graph file), AT-2 (`every_node_type_is_readable`) and AT-10
//! (`version_is_required`, `version_must_be_semver`).
//! Requirements: REQ-B-6, REQ-B-7, REQ-B-14, REQ-B-16, REQ-T-2, REQ-T-5, ADR 0012, ASM-057,
//! ASM-074, ASM-075.
//!
//! Step 4 of M3: written before WI-12. `Loader::graphs_from_sources` is a `todo!()` stub, so tests
//! that load a graph are ignored with that work item; the discovery tests go through
//! `templates_from_sources` and fail on the missing classification instead of a stub (also WI-12).
//!
//! Every assertion checks code, path, line and column, except for an absent key, whose location
//! the plan leaves open (as in M2): there the code, the file and the message are pinned. Positions
//! follow the M2 conventions: an unknown key at the key, a wrong or invalid value at the value, a
//! table name at its name in the header. Positions are found in the source text with `pos_of`.

#![forbid(unsafe_code)]

mod common;
mod graph_support;

use std::collections::BTreeMap;

use cgf_core::{
    BbType, GraphDef, NODE_TYPES, NodeId, NodeKind, NodeKindTag, Provenance, Version, VersionReq,
};
use cgf_templates::SourceFile;
use common::{
    alpha_template, errors, lines, ok, pos_after, pos_of, pos_of_nth, row, rows, src, templates,
};
use graph_support::{PATH, graph_errors, graph_rows, graphs, minimal, one_graph};
use proptest::prelude::*;
use serde_json::json;

/// A valid graph with a selector, a memory sequence, a condition that stores a key, a subgraph
/// call with an input binding and an output binding, a leaf with a literal, an `[editor]` table.
fn valid() -> String {
    lines(&[
        "schema_version = 1",
        "id = \"test.graph.patrol\"",
        "version = \"1.2.3\"",
        "description = \"Follow the route; engage hostiles\"",
        "root = \"main\"",
        "[interface]",
        "params = { engage_range_m = { type = \"number\", exclusiveMinimum = 0, default = 2500.0 } }",
        "result = \"Failure when the target is lost\"",
        "[blackboard]",
        "target = \"entity\"",
        "[nodes.main]",
        "type = \"selector\"",
        "children = [\"engage\", \"patrol\"]",
        "[nodes.engage]",
        "type = \"sequence\"",
        "memory = true",
        "children = [\"enemy_near\", \"attack\"]",
        "[nodes.enemy_near]",
        "type = \"condition\"",
        "ref = \"stub.cond.near\"",
        "params = { range_m = { param = \"engage_range_m\" }, store_as = \"target\" }",
        "[nodes.attack]",
        "type = \"subgraph\"",
        "ref = \"stub.behavior.ext\"",
        "version = \"^1.0\"",
        "params = { target = { key = \"target\" }, speed = 3.0 }",
        "outputs = { found = \"target\" }",
        "[nodes.patrol]",
        "type = \"action\"",
        "ref = \"stub.action.go\"",
        "params = { speed_mps = 5.0 }",
        "[editor]",
        "positions = { main = [0, 0], patrol = [10, 20] }",
    ])
}

fn node<'a>(def: &'a GraphDef, name: &str) -> &'a cgf_core::NodeDef {
    def.nodes
        .iter()
        .find(|n| n.name == name)
        .unwrap_or_else(|| panic!("no node `{name}` in {:?}", def.nodes))
}

fn names(def: &GraphDef, ids: &[NodeId]) -> Vec<String> {
    ids.iter()
        .map(|id| def.node(*id).map_or_else(String::new, |n| n.name.clone()))
        .collect()
}

// --- the happy path ----------------------------------------------------------------------------

/// REQ-B-6: a valid file reads into the `GraphDef` the file says, `[editor]` excluded.
#[test]
fn valid_graph_reads_into_graph_def() {
    let def = one_graph(&valid());

    assert_eq!(def.id.as_str(), "test.graph.patrol");
    assert_eq!(def.version, Version::new(1, 2, 3));
    assert_eq!(
        def.description.as_deref(),
        Some("Follow the route; engage hostiles")
    );
    assert_eq!(def.source, Provenance::File);
    assert_eq!(def.origin.as_deref(), Some(std::path::Path::new(PATH)));
    assert_eq!(def.node(def.root).map(|n| n.name.as_str()), Some("main"));
    assert_eq!(def.nodes.len(), 5);
    assert_eq!(
        def.blackboard,
        BTreeMap::from([("target".to_string(), BbType::Entity)])
    );
    assert_eq!(
        def.interface.types,
        BTreeMap::from([("engage_range_m".to_string(), BbType::Number)])
    );
    assert_eq!(
        def.interface.defaults.get("engage_range_m"),
        Some(&json!(2500.0))
    );
    assert!(!def.interface.is_required("engage_range_m"));
    assert_eq!(
        def.interface.result.as_deref(),
        Some("Failure when the target is lost")
    );

    let NodeKind::Selector(main) = &node(&def, "main").kind else {
        panic!("main is a selector");
    };
    assert!(!main.memory);
    assert_eq!(names(&def, &main.children), ["engage", "patrol"]);
    let NodeKind::Sequence(engage) = &node(&def, "engage").kind else {
        panic!("engage is a sequence");
    };
    assert!(engage.memory);
    assert_eq!(names(&def, &engage.children), ["enemy_near", "attack"]);
    let NodeKind::Condition(near) = &node(&def, "enemy_near").kind else {
        panic!("enemy_near is a condition");
    };
    assert_eq!(near.id.as_str(), "stub.cond.near");
    assert_eq!(
        near.params.as_json(),
        &json!({ "range_m": { "param": "engage_range_m" }, "store_as": "target" })
    );
    let NodeKind::Subgraph(attack) = &node(&def, "attack").kind else {
        panic!("attack is a subgraph");
    };
    assert_eq!(attack.id.as_str(), "stub.behavior.ext");
    assert_eq!(attack.version, VersionReq::parse("^1.0").ok());
    assert_eq!(
        attack.inputs,
        BTreeMap::from([("target".to_string(), "target".to_string())])
    );
    assert_eq!(
        attack.outputs,
        BTreeMap::from([("found".to_string(), "target".to_string())])
    );
    assert_eq!(
        attack.params.as_json(),
        &json!({ "speed": 3.0 }),
        "the input binding left params"
    );
    assert_eq!(def.subgraph_refs().len(), 1);
    assert_eq!(def.leaf_refs().len(), 2);
}

/// REQ-B-16: the `[editor]` table never reaches the `GraphDef`: with and without it the graphs
/// are equal.
#[test]
fn editor_table_contents_are_dropped() {
    let with = valid();
    let without = with
        .split("[editor]")
        .next()
        .map(str::to_string)
        .unwrap_or_default();

    assert_eq!(one_graph(&with), one_graph(&without));
}

/// ASM-075: a datetime inside `[editor]` is still rejected (the M2 parse-stage value rules apply),
/// at the value.
#[test]
fn datetime_in_editor_is_unsupported_value() {
    let text = format!(
        "{}[editor]\nsaved = 1979-05-27T07:32:00Z\n",
        minimal("test.graph.ed")
    );

    assert_eq!(
        graph_rows(&text),
        [row("unsupported_value", PATH, pos_after(&text, "saved = "))]
    );
}

/// ASM-075: `[editor]` must be a table.
#[test]
fn editor_that_is_not_a_table_is_wrong_type() {
    let text = lines(&[
        "schema_version = 1",
        "id = \"test.graph.ed\"",
        "version = \"1.0.0\"",
        "root = \"r\"",
        "editor = 5",
        "[nodes.r]",
        "type = \"action\"",
        "ref = \"stub.action.noop\"",
    ]);

    assert_eq!(
        graph_rows(&text),
        [row("wrong_type", PATH, pos_after(&text, "editor = "))]
    );
}

/// One graph that uses each of the twelve node types.
const ALL_TYPES: &[&str] = &[
    "schema_version = 1",
    "id = \"test.graph.all\"",
    "version = \"1.0.0\"",
    "root = \"root\"",
    "[nodes.root]",
    "type = \"sequence\"",
    "children = [\"c1\", \"a1\", \"sl\", \"p\", \"i\", \"s\", \"r\", \"cd\", \"to\", \"sg\", \"f\"]",
    "[nodes.c1]",
    "type = \"condition\"",
    "ref = \"stub.cond.flag\"",
    "[nodes.a1]",
    "type = \"action\"",
    "ref = \"stub.action.noop\"",
    "[nodes.sl]",
    "type = \"selector\"",
    "children = [\"sl1\"]",
    "[nodes.sl1]",
    "type = \"action\"",
    "ref = \"stub.action.noop\"",
    "[nodes.p]",
    "type = \"parallel\"",
    "policy = \"any\"",
    "children = [\"pa\", \"pb\"]",
    "[nodes.pa]",
    "type = \"action\"",
    "ref = \"stub.action.noop\"",
    "[nodes.pb]",
    "type = \"action\"",
    "ref = \"stub.action.noop\"",
    "[nodes.i]",
    "type = \"inverter\"",
    "child = \"ic\"",
    "[nodes.ic]",
    "type = \"condition\"",
    "ref = \"stub.cond.flag\"",
    "[nodes.s]",
    "type = \"succeed_always\"",
    "child = \"sc\"",
    "[nodes.sc]",
    "type = \"action\"",
    "ref = \"stub.action.noop\"",
    "[nodes.r]",
    "type = \"repeat\"",
    "count = 3",
    "child = \"rc\"",
    "[nodes.rc]",
    "type = \"action\"",
    "ref = \"stub.action.noop\"",
    "[nodes.cd]",
    "type = \"cooldown\"",
    "duration_s = 1.5",
    "child = \"cc\"",
    "[nodes.cc]",
    "type = \"action\"",
    "ref = \"stub.action.noop\"",
    "[nodes.to]",
    "type = \"timeout\"",
    "duration_s = 2.0",
    "child = \"tc\"",
    "[nodes.tc]",
    "type = \"action\"",
    "ref = \"stub.action.noop\"",
    "[nodes.sg]",
    "type = \"subgraph\"",
    "ref = \"stub.behavior.ext\"",
    "version = \"^1\"",
    "params = { speed = 2.0 }",
    "[nodes.f]",
    "type = \"fsm\"",
    "initial = \"s0\"",
    "states = { s0 = \"fa\", s1 = \"fb\" }",
    "exits = { done = \"success\" }",
    "[[nodes.f.transitions]]",
    "from = \"s0\"",
    "to = \"s1\"",
    "when = \"g\"",
    "[[nodes.f.transitions]]",
    "from = \"s1\"",
    "to = \"done\"",
    "on = \"success\"",
    "[nodes.fa]",
    "type = \"action\"",
    "ref = \"stub.action.noop\"",
    "[nodes.fb]",
    "type = \"action\"",
    "ref = \"stub.action.noop\"",
    "[nodes.g]",
    "type = \"condition\"",
    "ref = \"stub.cond.flag\"",
];

/// AT-2, REQ-B-7: all twelve node types are readable: one graph uses each, and the reader keeps
/// the settings.
#[test]
fn every_node_type_is_readable() {
    let text = lines(ALL_TYPES);

    let def = one_graph(&text);

    let tags: std::collections::BTreeSet<NodeKindTag> =
        def.nodes.iter().map(|n| n.kind.tag()).collect();
    let documented: std::collections::BTreeSet<&str> = NODE_TYPES.iter().map(|n| n.name).collect();
    assert_eq!(
        tags.iter()
            .map(|t| t.name())
            .collect::<std::collections::BTreeSet<_>>(),
        documented
    );
    assert_eq!(documented.len(), 12);
    let NodeKind::Repeat(repeat) = &node(&def, "r").kind else {
        panic!("repeat")
    };
    assert_eq!(repeat.count.map(std::num::NonZeroU32::get), Some(3));
    let NodeKind::Fsm(fsm) = &node(&def, "f").kind else {
        panic!("fsm")
    };
    assert_eq!(fsm.initial, 0);
    assert_eq!(fsm.states.len(), 2);
    assert_eq!(fsm.exits.len(), 1);
    assert_eq!(fsm.transitions.len(), 2);
}

// --- schema_version (REQ-T-5, ASM-057) ---------------------------------------------------------

/// REQ-T-5: version 1 is accepted.
#[test]
fn graph_schema_version_1_accepted() {
    let set = ok(graphs(&[src(PATH, &minimal("test.graph.v1"))]));

    assert_eq!(set.len(), 1);
}

/// REQ-T-5: a newer version is rejected at its value and the file stops there (the unknown key
/// after it is not reported).
#[test]
fn graph_schema_version_newer_rejected_and_file_stops() {
    let text = lines(&["schema_version = 2", "colour = \"red\"", "id = 5"]);

    let found = graph_errors(&text);

    assert_eq!(
        rows(&found),
        [row("unsupported_schema_version", PATH, (1, 18))]
    );
    assert_eq!(
        found[0].message,
        "schema_version 2 is newer than the newest supported version 1"
    );
}

/// REQ-T-5: a missing `schema_version` is `missing_key` (location of an absent key: code, file and
/// message only).
#[test]
fn graph_schema_version_missing_is_missing_key() {
    let text = minimal("test.graph.v").replacen("schema_version = 1\n", "", 1);

    let found = graph_errors(&text);

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

/// REQ-T-5: `schema_version = 0` is `invalid_value` at the value.
#[test]
fn graph_schema_version_zero_is_invalid_value() {
    let text = minimal("test.graph.v").replacen("schema_version = 1", "schema_version = 0", 1);

    assert_eq!(graph_rows(&text), [row("invalid_value", PATH, (1, 18))]);
}

/// REQ-T-5: a string `schema_version` is `wrong_type` at the value.
#[test]
fn graph_schema_version_string_is_wrong_type() {
    let text = minimal("test.graph.v").replacen("schema_version = 1", "schema_version = \"1\"", 1);

    assert_eq!(graph_rows(&text), [row("wrong_type", PATH, (1, 18))]);
}

// --- required top-level keys, versions ---------------------------------------------------------

/// REQ-B-6: a file without `id`, `version` and `root` reports each as `missing_key`, all at once.
#[test]
fn missing_id_version_root_each_reported() {
    let text = lines(&[
        "schema_version = 1",
        "[nodes.r]",
        "type = \"action\"",
        "ref = \"stub.action.noop\"",
    ]);

    let found = graph_errors(&text);

    let missing: Vec<&str> = found
        .iter()
        .filter(|d| d.code.as_str() == "missing_key")
        .map(|d| d.message.as_str())
        .collect();
    assert_eq!(missing.len(), 3, "{missing:?}");
    for key in ["`id`", "`version`", "`root`"] {
        assert!(
            missing.iter().any(|m| m.contains(key)),
            "{key} in {missing:?}"
        );
    }
}

/// REQ-B-14 (AT-10): `version` is required.
#[test]
fn version_is_required() {
    let text = minimal("test.graph.nover").replacen("version = \"1.0.0\"\n", "", 1);

    let found = graph_errors(&text);

    assert_eq!(found.len(), 1);
    assert_eq!(found[0].code.as_str(), "missing_key");
    assert!(
        found[0].message.contains("`version`"),
        "{}",
        found[0].message
    );
}

/// REQ-B-14 (AT-10): `version` must be semver (`MAJOR.MINOR.PATCH`), `invalid_version` at the
/// value; a non-string version is `wrong_type`. The value is at 3:11 in `minimal` (the plan's
/// `pos_after(&text, "version = ")` would find the end of `schema_version = ` on line 1 first).
#[test]
fn version_must_be_semver() {
    for bad in ["1.0", "v1.0.0", "latest", "1.0.0.0", "01.0.0", ""] {
        let text = minimal("test.graph.v").replacen(
            "version = \"1.0.0\"",
            &format!("version = \"{bad}\""),
            1,
        );

        assert_eq!(
            graph_rows(&text),
            [row("invalid_version", PATH, (3, 11))],
            "{bad:?}"
        );
    }
    let text = minimal("test.graph.v").replacen("version = \"1.0.0\"", "version = 1", 1);
    assert_eq!(graph_rows(&text), [row("wrong_type", PATH, (3, 11))]);
}

/// AT-1: `invalid_semver_is_invalid_version_at_value`: the plan's name for the single case.
#[test]
fn invalid_semver_is_invalid_version_at_value() {
    let text = minimal("test.graph.v").replacen("version = \"1.0.0\"", "version = \"1.0\"", 1);

    assert_eq!(
        graph_rows(&text),
        [row("invalid_version", PATH, pos_of(&text, "\"1.0\""))]
    );
}

/// ADR 0002: the graph `id` follows the component ID grammar: `invalid_id` at the offending
/// character (the M2 convention of `Reader::id`: here the `T` after the opening quote).
#[test]
fn graph_id_that_breaks_the_grammar_is_invalid_id() {
    let text = minimal("Test.Graph");

    assert_eq!(
        graph_rows(&text),
        [row("invalid_id", PATH, pos_of(&text, "Test.Graph"))]
    );
}

// --- unknown keys, settings, names -------------------------------------------------------------

/// REQ-T-2: unknown keys are errors at every level (top level, `[interface]`, a node, an FSM node,
/// a transition), a bad `[blackboard]` type is `invalid_value`; all are reported together, sorted.
#[test]
fn unknown_key_in_node_interface_blackboard_fsm_reported() {
    let text = lines(&[
        "schema_version = 1",
        "id = \"test.graph.unknown\"",
        "version = \"1.0.0\"",
        "root = \"m\"",
        "top_colour = 1",
        "[interface]",
        "if_colour = 1",
        "[blackboard]",
        "target = \"robot\"",
        "[nodes.m]",
        "type = \"fsm\"",
        "initial = \"a\"",
        "states = { a = \"x\", b = \"y\" }",
        "fsm_colour = 1",
        "[[nodes.m.transitions]]",
        "from = \"a\"",
        "to = \"b\"",
        "when = \"g\"",
        "tr_colour = 1",
        "[nodes.x]",
        "type = \"action\"",
        "ref = \"stub.action.noop\"",
        "node_colour = 1",
        "[nodes.y]",
        "type = \"action\"",
        "ref = \"stub.action.noop\"",
        "[nodes.g]",
        "type = \"condition\"",
        "ref = \"stub.cond.flag\"",
    ]);

    assert_eq!(
        graph_rows(&text),
        [
            row("unknown_key", PATH, pos_of(&text, "top_colour")),
            row("unknown_key", PATH, pos_of(&text, "if_colour")),
            row("invalid_value", PATH, pos_of(&text, "\"robot\"")),
            row("unknown_key", PATH, pos_of(&text, "fsm_colour")),
            row("unknown_key", PATH, pos_of(&text, "tr_colour")),
            row("unknown_key", PATH, pos_of(&text, "node_colour")),
        ]
    );
}

/// Builds a graph whose root `r` is the node described by `settings`; `with_leaf` adds a node
/// `leaf` for settings that reference it.
fn root_node(settings: &[&str], with_leaf: bool) -> String {
    let mut all = vec![
        "schema_version = 1",
        "id = \"test.graph.settings\"",
        "version = \"1.0.0\"",
        "root = \"r\"",
        "[nodes.r]",
    ];
    all.extend_from_slice(settings);
    if with_leaf {
        all.extend_from_slice(&[
            "[nodes.leaf]",
            "type = \"action\"",
            "ref = \"stub.action.noop\"",
        ]);
    }
    lines(&all)
}

/// (settings of node `r`, node `leaf` exists, expected code, text before the bad value)
const SETTINGS_CASES: [(&[&str], bool, &str, &str); 13] = [
    (
        &["type = \"sequence\"", "children = \"leaf\""],
        false,
        "wrong_type",
        "children = ",
    ),
    (
        &["type = \"sequence\"", "children = []"],
        false,
        "invalid_value",
        "children = ",
    ),
    (
        &[
            "type = \"sequence\"",
            "children = [\"leaf\"]",
            "memory = \"yes\"",
        ],
        true,
        "wrong_type",
        "memory = ",
    ),
    (
        &[
            "type = \"parallel\"",
            "children = [\"leaf\"]",
            "policy = \"some\"",
        ],
        true,
        "invalid_value",
        "policy = ",
    ),
    (
        &["type = \"repeat\"", "child = \"leaf\"", "count = 0"],
        true,
        "invalid_value",
        "count = ",
    ),
    (
        &["type = \"repeat\"", "child = \"leaf\"", "count = 1.5"],
        true,
        "wrong_type",
        "count = ",
    ),
    (
        &[
            "type = \"cooldown\"",
            "child = \"leaf\"",
            "duration_s = 0.0",
        ],
        true,
        "invalid_value",
        "duration_s = ",
    ),
    (
        &[
            "type = \"timeout\"",
            "child = \"leaf\"",
            "duration_s = \"1\"",
        ],
        true,
        "wrong_type",
        "duration_s = ",
    ),
    (
        &[
            "type = \"timeout\"",
            "child = \"leaf\"",
            "duration_s = -1.0",
        ],
        true,
        "invalid_value",
        "duration_s = ",
    ),
    (
        &["type = \"inverter\"", "child = 5"],
        false,
        "wrong_type",
        "child = ",
    ),
    (&["type = \"bogus\""], false, "invalid_value", "type = "),
    (
        &[
            "type = \"succeed_always\"",
            "child = \"leaf\"",
            "memory = true",
        ],
        true,
        "unknown_key",
        "memory",
    ),
    (
        &[
            "type = \"sequence\"",
            "children = [\"leaf\"]",
            "memory = true",
            "colour = 1",
        ],
        true,
        "unknown_key",
        "colour",
    ),
];

/// REQ-B-7: node settings of the wrong type or out of range are reported at the value; a missing
/// required setting is `missing_key`.
#[test]
fn node_settings_wrong_type_and_range_reported() {
    let cases = SETTINGS_CASES;
    for (settings, with_leaf, code, before) in cases {
        let text = root_node(settings, with_leaf);
        let expected = if code == "unknown_key" {
            pos_of_nth(&text, before, 0)
        } else {
            pos_after(&text, before)
        };

        assert_eq!(
            graph_rows(&text),
            [row(code, PATH, expected)],
            "{settings:?}"
        );
    }
}

/// REQ-T-2: absent required settings are `missing_key` (code, file and message): `child` of an
/// inverter, `ref` of a condition and `type` of a node.
#[test]
fn node_missing_required_settings_are_missing_key() {
    let cases: [(&[&str], &str); 3] = [
        (&["type = \"inverter\""], "child"),
        (&["type = \"condition\""], "ref"),
        (&["colour = 1"], "type"),
    ];
    for (settings, key) in cases {
        let found = graph_errors(&root_node(settings, false));

        let missing: Vec<_> = found
            .iter()
            .filter(|d| d.code.as_str() == "missing_key")
            .collect();
        assert_eq!(missing.len(), 1, "{settings:?}: {found:?}");
        assert!(
            missing[0].message.contains(&format!("`{key}`")),
            "{}",
            missing[0].message
        );
    }
}

/// ADR 0012: node, state, exit and blackboard key names are `[a-z][a-z0-9_]*` (and at most
/// `MAX_ID_LEN` bytes, with a bounded echo of the offender); `invalid_value` at the name.
#[test]
fn names_follow_the_segment_grammar_with_a_bounded_echo() {
    let long = "k".repeat(129);
    let blackboard = |key: &str| {
        lines(&[
            "schema_version = 1",
            "id = \"test.graph.names\"",
            "version = \"1.0.0\"",
            "root = \"r\"",
            "[blackboard]",
            &format!("{key} = \"entity\""),
            "[nodes.r]",
            "type = \"action\"",
            "ref = \"stub.action.noop\"",
        ])
    };
    for key in ["Bad", "9lives", "has-dash", &long] {
        let text = blackboard(&if key.contains('-') || key.starts_with(char::is_numeric) {
            format!("\"{key}\"")
        } else {
            key.to_string()
        });

        let found = graph_errors(&text);

        let hit = found.iter().find(|d| d.code.as_str() == "invalid_value");
        let Some(hit) = hit else {
            panic!("{key}: {found:?}")
        };
        assert!(
            hit.message.len() < 400,
            "bounded echo: {}",
            hit.message.len()
        );
        assert!(
            !hit.message.contains(&"k".repeat(100)),
            "the 129-byte key is not echoed in full"
        );
    }
    let text = lines(&[
        "schema_version = 1",
        "id = \"test.graph.names\"",
        "version = \"1.0.0\"",
        "root = \"Bad\"",
        "[nodes.Bad]",
        "type = \"action\"",
        "ref = \"stub.action.noop\"",
    ]);
    assert!(
        graph_rows(&text).contains(&row("invalid_value", PATH, pos_of(&text, "Bad]"))),
        "node name"
    );
}

// --- discovery and the shared report -----------------------------------------------------------

/// ASM-074: a `*.behavior.toml` source is a graph, any other `.toml` is a template; the suffix is
/// case-sensitive. The same text is accepted as a graph and rejected as a template.
#[test]
fn behavior_toml_suffix_is_a_graph_other_toml_is_a_template() {
    let template = alpha_template("tank.base", None, "1.0");
    let graph = minimal("test.graph.suffix");

    let as_graph = templates(&[
        src("t.toml", &template),
        src("behaviors/g.behavior.toml", &graph),
    ]);
    let as_template = errors(templates(&[src("behaviors/g.toml", &graph)]));
    let wrong_case = errors(templates(&[src("behaviors/g.Behavior.toml", &graph)]));

    assert_eq!(ok(as_graph).len(), 1, "only the template is a template");
    assert!(as_template.iter().any(|d| d.code.as_str() == "unknown_key"));
    assert!(wrong_case.iter().any(|d| d.code.as_str() == "unknown_key"));
}

/// REQ-T-2: errors of graph files and template files come out in one list, sorted by path.
#[test]
fn errors_in_graph_and_template_files_reported_in_one_sorted_list() {
    let template = lines(&[
        "schema_version = 1",
        "id = \"tank.base\"",
        "colour = \"red\"",
        "[motion]",
        "model = \"stub.motion.alpha\"",
        "params = { a = 1.0 }",
    ]);
    let graph = minimal("test.graph.sorted").replacen("version = \"1.0.0\"", "version = 1", 1);

    let found = errors(templates(&[
        src("b/g.behavior.toml", &graph),
        src("a/t.toml", &template),
    ]));

    assert_eq!(
        rows(&found),
        [
            row("unknown_key", "a/t.toml", (3, 1)),
            row("wrong_type", "b/g.behavior.toml", (3, 11)),
        ]
    );
}

/// REQ-T-2: a graph diagnostic renders in the M2 format `path:line:col: error[code]: message`.
#[test]
fn rendered_graph_diagnostic_uses_the_m2_format() {
    let text = minimal("test.graph.render").replacen("root = \"r\"", "root = \"r\"\ncolour = 1", 1);

    let found = graph_errors(&text);

    assert_eq!(found.len(), 1);
    assert_eq!(
        found[0].to_string(),
        format!("{PATH}:5:1: error[unknown_key]: {}", found[0].message)
    );
}

// --- no panics ---------------------------------------------------------------------------------

/// A valid graph with its text cut and a fragment inserted, or plain noise.
fn graph_text() -> impl Strategy<Value = String> {
    let base = valid();
    let len = base.len();
    prop_oneof![
        "[ -~\n]{0,400}",
        (0..len, 0..40usize, "[ -~\n]{0,12}").prop_map(move |(at, cut, insert)| {
            let end = (at + cut).min(base.len());
            format!("{}{}{}", &base[..at], insert, &base[end..])
        }),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// NFR-2: no input makes the graph reader or the graph set resolution panic; the result is a
    /// graph set or diagnostics.
    #[test]
    fn graph_parse_never_panics(text in graph_text()) {
        let result = graphs(&[SourceFile::new(PATH, text)]);
        prop_assert!(result.is_ok() || result.is_err());
    }
}
