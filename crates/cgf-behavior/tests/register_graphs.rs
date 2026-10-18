//! WI-10 rows of AT-5/AT-9: `register_graphs` registers graphs as behaviors in dependency order,
//! fills `ComponentInfo`, and rejects cycles, duplicates and dangling references.
//! Requirements: REQ-B-9, REQ-B-10, REQ-B-13, ADR 0011, ADR 0014, ASM-091.
//!
//! Step 4 of M3: written before WI-10; un-ignored in WI-10.

#![forbid(unsafe_code)]

mod common;

use std::sync::Arc;

use cgf_behavior::{GraphRegistrationError, register_graphs};
use cgf_core::{BbType, ComponentKind, GraphDef, Provenance, RegistryError, Version, VersionReq};
use common::{B, Bench, call, cid, iface, output, script_graph};
use serde_json::json;

/// A graph `id` whose root is a `subgraph` node calling `target`.
fn caller(id: &str, target: &str) -> GraphDef {
    let mut b = B::new();
    let node = b.subgraph("call", call(target, json!({})));
    b.graph(id, node)
}

fn register(bench: &mut Bench, graphs: Vec<GraphDef>) -> Result<(), GraphRegistrationError> {
    let graphs: Vec<Arc<GraphDef>> = graphs.into_iter().map(Arc::new).collect();
    register_graphs(&mut bench.registry, graphs.iter())
}

/// Every permutation of three graphs a -> b -> c registers, whatever the input order.
#[test]
fn registers_in_dependency_order_regardless_of_input_order() {
    let orders = [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ];
    for order in orders {
        let mut bench = Bench::new();
        let graphs = [
            caller("test.graph.a", "test.graph.b"),
            caller("test.graph.b", "test.graph.c"),
            script_graph("test.graph.c", "leaf", &["success"]),
        ];
        let permuted: Vec<GraphDef> = order.iter().map(|&i| graphs[i].clone()).collect();

        let result = register(&mut bench, permuted);

        assert!(result.is_ok(), "{order:?}: {result:?}");
        for id in ["test.graph.a", "test.graph.b", "test.graph.c"] {
            assert!(
                bench.registry.behavior(&cid(id)).is_some(),
                "{order:?}: {id}"
            );
        }
    }
}

/// REQ-B-13, ADR 0014: the registered `ComponentInfo` reflects the graph: provenance, version,
/// description, `param_types` from `Interface::types` (binding-only `entity` and `position`
/// params included), outputs, subgraph references and result text.
#[test]
fn fills_component_info_from_the_graph() {
    let mut bench = Bench::new();
    let mut child = B::new();
    let leaf = child.act("leaf", &["success"]);
    let child_graph = child.graph("test.graph.child", leaf);
    let mut b = B::new();
    let sub = b.subgraph("call", call("test.graph.child", json!({})));
    let interface = iface(&json!({
        "target": { "type": "entity" },
        "where": { "type": "position" },
        "speed": { "type": "number", "default": 1.0 },
    }))
    .with_output("found", output(BbType::Integer, "n"))
    .with_result("Failure when the target is lost");
    let mut graph = b
        .graph_as("test.graph.main", interface, sub, Provenance::Builtin)
        .with_description("Main graph")
        .with_blackboard_key("n", BbType::Integer);
    graph.version = Version::new(2, 1, 0);

    register(&mut bench, vec![graph, child_graph]).expect("register");
    let info = bench.registry.info(&cid("test.graph.main")).expect("info");

    assert_eq!(info.source, Provenance::Builtin);
    assert_eq!(info.version, Version::new(2, 1, 0));
    assert_eq!(info.description, "Main graph");
    assert_eq!(
        info.param_types
            .iter()
            .map(|(k, v)| (k.as_str(), *v))
            .collect::<Vec<_>>(),
        [
            ("speed", BbType::Number),
            ("target", BbType::Entity),
            ("where", BbType::Position),
        ]
    );
    assert_eq!(
        info.outputs
            .iter()
            .map(|(k, v)| (k.as_str(), *v))
            .collect::<Vec<_>>(),
        [("found", BbType::Integer)]
    );
    assert_eq!(
        info.subgraphs,
        vec![(cid("test.graph.child"), None::<VersionReq>)]
    );
    assert_eq!(
        info.result.as_deref(),
        Some("Failure when the target is lost")
    );
}

/// REQ-T-3: the registered behavior's parameter schema is the interface's object schema, so the
/// loader validates call sites against it; binding-only params are not in it.
#[test]
fn param_schema_of_a_registered_graph_is_the_interface_schema() {
    let mut bench = Bench::new();
    let mut b = B::new();
    let leaf = b.act("leaf", &["success"]);
    let interface = iface(&json!({
        "target": { "type": "entity" },
        "speed": { "type": "number", "default": 1.0 },
    }));
    register(
        &mut bench,
        vec![b.graph_with("test.graph.main", interface, leaf)],
    )
    .expect("register");

    let schema = bench
        .registry
        .param_schema(ComponentKind::Behavior, &cid("test.graph.main"))
        .expect("schema");

    let properties = schema.as_json()["properties"]
        .as_object()
        .expect("properties");
    assert_eq!(properties.keys().collect::<Vec<_>>(), ["speed"]);
}

/// ASM-091: a subgraph cycle cannot come out of the loader, but `register_graphs` rejects it
/// anyway instead of recursing: a two-graph cycle and a self-reference.
#[test]
fn rejects_a_cycle_defensively() {
    for graphs in [
        vec![
            caller("test.graph.x", "test.graph.y"),
            caller("test.graph.y", "test.graph.x"),
        ],
        vec![caller("test.graph.x", "test.graph.x")],
    ] {
        let mut bench = Bench::new();

        let result = register(&mut bench, graphs);

        assert!(
            matches!(result, Err(GraphRegistrationError::Cycle { .. })),
            "{result:?}"
        );
    }
}

/// An ID already registered (under any kind) is a registry error and the first one is kept.
#[test]
fn rejects_a_duplicate_id() {
    let mut bench = Bench::new();
    register(
        &mut bench,
        vec![script_graph("test.graph.once", "leaf", &["success"])],
    )
    .expect("first");

    let again = register(
        &mut bench,
        vec![script_graph("test.graph.once", "leaf", &["failure"])],
    );
    let clash = register(
        &mut bench,
        vec![script_graph("stub.cond.flag", "leaf", &["failure"])],
    );

    for result in [again, clash] {
        assert!(
            matches!(
                result,
                Err(GraphRegistrationError::Registry(
                    RegistryError::Duplicate { .. }
                ))
            ),
            "{result:?}"
        );
    }
}

/// A subgraph or leaf the registry does not know is `Unresolved`, naming the graph and the ID.
#[test]
fn rejects_dangling_references() {
    let mut bench = Bench::new();
    let mut b = B::new();
    let leaf = b.leaf_action("x", "stub.action.missing", json!({}));
    let missing_leaf = b.graph("test.graph.leaf", leaf);

    for (graph, missing) in [
        (
            caller("test.graph.sub", "test.graph.nowhere"),
            "test.graph.nowhere",
        ),
        (missing_leaf, "stub.action.missing"),
    ] {
        let result = register(&mut bench, vec![graph]);

        match result {
            Err(GraphRegistrationError::Unresolved { id, .. }) => assert_eq!(id.as_str(), missing),
            other => panic!("expected Unresolved({missing}), got {other:?}"),
        }
    }
}

/// ADR 0014: a `{ param }` reference with no argument and no default is a build error, not a
/// panic (the loader prevents it; the factory still checks).
#[test]
fn unbound_param_reference_is_a_build_error() {
    let mut bench = Bench::new();
    let mut b = B::new();
    let leaf = b.leaf_action(
        "leaf",
        "stub.action.script",
        json!({ "name": { "param": "who" }, "script": ["success"] }),
    );
    let interface = iface(&json!({ "who": { "type": "string" } }));
    register(
        &mut bench,
        vec![b.graph_with("test.graph.main", interface, leaf)],
    )
    .expect("register");

    let built = bench
        .registry
        .behavior(&cid("test.graph.main"))
        .expect("registered")
        .build(&json!({}));

    assert!(built.is_err());
}
