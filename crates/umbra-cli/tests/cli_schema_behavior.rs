//! WI-15: `umbra schema behavior` and `schema component` for the M3 component kinds.
//! Requirements: REQ-X-2, REQ-B-17, D-14, ASM-077.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod common;

use common::{cli, stderr, stdout};
use serde_json::Value;

fn json_of(args: &[&str]) -> Value {
    let out = cli(args);
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    serde_json::from_str(&stdout(&out)).expect("stdout is one JSON document")
}

/// `schema behavior` prints the published behavior file schema, which is valid JSON.
#[test]
fn schema_behavior_prints_the_published_schema() {
    let printed = json_of(&["schema", "behavior"]);

    assert_eq!(printed["title"], "Umbra behavior graph file, format v1");
    let published: Value =
        serde_json::from_str(umbra_engine::BEHAVIOR_FILE_SCHEMA).expect("published schema is JSON");
    assert_eq!(printed, published);
}

/// `schema component` finds conditions, actions, behaviors (graphs) and the visual sensor.
#[test]
fn schema_component_finds_the_m3_kinds() {
    for id in [
        "builtin.cond.enemy_in_range",
        "builtin.action.fire",
        "builtin.fsm.patrol_and_engage",
        "builtin.subgraph.engage_target",
        "builtin.visual",
    ] {
        let schema = json_of(&["schema", "component", id]);

        assert_eq!(schema["type"], "object", "{id}");
        assert!(schema["examples"].is_array(), "{id}");
    }
}

/// The built-in graphs publish their interface: the spec 7.1 `aggression` parameter is a number in
/// [0, 1] with a default, and the binding-only `target` is not in the schema.
#[test]
fn builtin_graph_interfaces_are_published() {
    let patrol = json_of(&["schema", "component", "builtin.fsm.patrol_and_engage"]);
    let engage = json_of(&["schema", "component", "builtin.subgraph.engage_target"]);

    assert_eq!(patrol["properties"]["aggression"]["maximum"], 1);
    assert_eq!(patrol["properties"]["engage_range_m"]["default"], 3000.0);
    assert!(engage["properties"].get("target").is_none());
    assert_eq!(engage["properties"]["fire_range_m"]["default"], 1500.0);
}

/// An unknown component is still exit 1, as in M2.
#[test]
fn schema_component_unknown_is_still_exit_1() {
    let out = cli(&["schema", "component", "builtin.cond.nope"]);

    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("no component with id"));
}
