//! `register_graphs` refuses bindings the runtime could not honour, requires the interface schema
//! to agree with the defaults, and is all-or-nothing (WI-10 review). Requirements: REQ-B-10,
//! REQ-B-11, REQ-B-12, ADR 0014, ASM-099.

#![forbid(unsafe_code)]

mod common;

use std::sync::Arc;

use common::{B, Bench, call, cid, iface, no_interface, output, script_graph};
use serde_json::json;
use umbra_behavior::{GraphRegistrationError, register_graphs};
use umbra_core::{BbType, GraphDef, SubgraphRef};

fn register(bench: &mut Bench, graphs: Vec<GraphDef>) -> Result<(), GraphRegistrationError> {
    let graphs: Vec<Arc<GraphDef>> = graphs.into_iter().map(Arc::new).collect();
    register_graphs(&mut bench.registry, graphs.iter())
}

/// A child with an `entity` param `target` and an `integer` output `n` (from its key `n`).
fn child() -> GraphDef {
    let mut b = B::new();
    let leaf = b.act("leaf", &["success"]);
    let interface = iface(&json!({ "target": { "type": "entity" } }))
        .with_output("n", output(BbType::Integer, "n"));
    b.graph_with("test.graph.child", interface, leaf)
        .with_blackboard_key("n", BbType::Integer)
}

/// A parent whose only node calls the child with `sub`; its keys are `keys`.
fn parent(sub: SubgraphRef, keys: &[(&str, BbType)]) -> GraphDef {
    let mut b = B::new();
    let node = b.subgraph("sub", sub);
    let mut g = b.graph_with(
        "test.graph.parent",
        iface(&json!({ "ro": { "type": "integer", "default": 1 } })),
        node,
    );
    for (key, ty) in keys {
        g = g.with_blackboard_key(*key, *ty);
    }
    g
}

fn malformed(keys: &[(&str, BbType)], sub: SubgraphRef) -> String {
    let mut bench = Bench::new();
    match register(&mut bench, vec![child(), parent(sub, keys)]) {
        Err(GraphRegistrationError::Malformed { reason, .. }) => reason,
        other => panic!("expected Malformed, got {other:?}"),
    }
}

fn base() -> SubgraphRef {
    call("test.graph.child", json!({}))
}

#[test]
fn input_from_an_undeclared_parent_key_is_refused() {
    let reason = malformed(&[], base().with_input("target", "nowhere"));
    assert!(reason.contains("nowhere"), "{reason}");
}

#[test]
fn input_of_an_incompatible_type_is_refused() {
    let reason = malformed(&[("k", BbType::Bool)], base().with_input("target", "k"));
    assert!(
        reason.contains("`target`") && reason.contains("`k`"),
        "{reason}"
    );
}

#[test]
fn output_port_the_child_does_not_have_is_refused() {
    let reason = malformed(
        &[("k", BbType::Integer), ("e", BbType::Entity)],
        base().with_input("target", "e").with_output("zz", "k"),
    );
    assert!(reason.contains("zz"), "{reason}");
}

#[test]
fn output_of_a_mismatched_type_is_refused() {
    let reason = malformed(&[("k", BbType::Bool)], base().with_output("n", "k"));
    assert!(reason.contains("`n`") && reason.contains("`k`"), "{reason}");
}

#[test]
fn output_to_an_undeclared_parent_key_is_refused() {
    let reason = malformed(&[], base().with_output("n", "nowhere"));
    assert!(reason.contains("nowhere"), "{reason}");
}

#[test]
fn output_to_a_read_only_param_key_is_refused() {
    let reason = malformed(&[], base().with_output("n", "ro"));
    assert!(reason.contains("read-only"), "{reason}");
}

/// A compatible binding registers: an `integer` parent key feeds a `number` param.
#[test]
fn compatible_bindings_register() {
    let mut bench = Bench::new();
    let mut b = B::new();
    let leaf = b.act("leaf", &["success"]);
    let number_child = b.graph_with(
        "test.graph.num",
        iface(&json!({ "p": { "type": "number" } })),
        leaf,
    );
    let sub = call("test.graph.num", json!({})).with_input("p", "i");
    let mut pb = B::new();
    let node = pb.subgraph("sub", sub);
    let top = pb
        .graph("test.graph.top", node)
        .with_blackboard_key("i", BbType::Integer);

    assert!(register(&mut bench, vec![number_child, top]).is_ok());
}

/// m3: the schema's `required` must be exactly the params without a default.
#[test]
fn schema_required_that_disagrees_with_defaults_is_refused() {
    let mut b = B::new();
    let leaf = b.act("leaf", &["success"]);
    // `speed` has a default, but the schema (built by hand) still requires it.
    let mut interface = iface(&json!({ "speed": { "type": "number", "default": 1.0 } }));
    interface.params = umbra_core::ParamSchema::from_json_str(
        r#"{"type":"object","properties":{"speed":{"type":"number"}},"required":["speed"]}"#,
    )
    .expect("schema");
    let mut bench = Bench::new();

    let result = register(
        &mut bench,
        vec![b.graph_with("test.graph.req", interface, leaf)],
    );

    assert!(
        matches!(result, Err(GraphRegistrationError::Malformed { .. })),
        "{result:?}"
    );
}

/// m1: a failing batch registers nothing, whether the failing graph is an unresolved leaf or a
/// malformed arena, and the graphs that sort before it are not left behind. (Registration does not
/// build leaves, so a leaf `BuildError` cannot occur here.)
#[test]
fn a_failing_batch_registers_nothing() {
    let mut unresolved = B::new();
    let missing = unresolved.leaf_action("x", "stub.action.missing", json!({}));
    let unresolved = unresolved.graph("test.graph.z_unresolved", missing);
    let mut bad_root = B::new();
    let c = bad_root.seq("loop", false, &[]);
    let mut bad_root = bad_root.graph("test.graph.z_bad_root", c);
    bad_root.root = umbra_core::NodeId(7);
    for bad in [unresolved, bad_root] {
        let mut bench = Bench::new();
        let before = bench
            .registry
            .ids(umbra_core::ComponentKind::Behavior)
            .len();

        let result = register(
            &mut bench,
            vec![
                script_graph("test.graph.a_good", "leaf", &["success"]),
                no_graph(),
                bad,
            ],
        );

        assert!(result.is_err(), "{result:?}");
        assert_eq!(
            bench
                .registry
                .ids(umbra_core::ComponentKind::Behavior)
                .len(),
            before
        );
        assert!(bench.registry.behavior(&cid("test.graph.a_good")).is_none());
        assert!(bench.registry.behavior(&cid("test.graph.b_good")).is_none());
    }
}

fn no_graph() -> GraphDef {
    let mut b = B::new();
    let leaf = b.act("leaf", &["success"]);
    b.graph_with("test.graph.b_good", no_interface(), leaf)
}

/// ADR 0014: a key may not be both in `[blackboard]` and an interface param.
#[test]
fn blackboard_key_that_is_also_a_param_is_refused() {
    let mut b = B::new();
    let leaf = b.act("leaf", &["success"]);
    let graph = b
        .graph_with(
            "test.graph.dup",
            iface(&json!({ "n": { "type": "integer", "default": 1 } })),
            leaf,
        )
        .with_blackboard_key("n", BbType::Integer);
    let mut bench = Bench::new();

    let result = register(&mut bench, vec![graph]);

    assert!(
        matches!(&result, Err(GraphRegistrationError::Malformed { reason, .. }) if reason.contains("`n`")),
        "{result:?}"
    );
}
