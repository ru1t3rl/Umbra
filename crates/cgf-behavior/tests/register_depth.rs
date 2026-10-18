//! `register_graphs` caps the `subgraph` chain at 16 behaviors (ASM-073, ASM-099 e), also when the
//! lower part of the chain is already registered, and refuses before registering anything.
//! Requirements: REQ-B-12 (depth), ADR 0014.

#![forbid(unsafe_code)]

mod common;

use std::sync::Arc;

use cgf_behavior::{GraphRegistrationError, register_graphs};
use cgf_core::GraphDef;
use common::{B, Bench, call, cid, script_graph};
use serde_json::json;

/// `test.chain.g00 -> g01 -> ... -> g{n-1}`, the last one a leaf graph.
fn chain(n: usize) -> Vec<GraphDef> {
    (0..n)
        .map(|i| {
            let id = format!("test.chain.g{i:02}");
            if i + 1 == n {
                return script_graph(&id, "leaf", &["success"]);
            }
            let mut b = B::new();
            let node = b.subgraph(
                "call",
                call(&format!("test.chain.g{:02}", i + 1), json!({})),
            );
            b.graph(&id, node)
        })
        .collect()
}

fn register(bench: &mut Bench, graphs: Vec<GraphDef>) -> Result<(), GraphRegistrationError> {
    let graphs: Vec<Arc<GraphDef>> = graphs.into_iter().map(Arc::new).collect();
    register_graphs(&mut bench.registry, graphs.iter())
}

/// A chain of 16 is accepted and a chain of 17 is `TooDeep`, with nothing registered.
#[test]
fn sixteen_levels_are_accepted_and_seventeen_are_refused() {
    let mut ok = Bench::new();
    assert!(register(&mut ok, chain(16)).is_ok());

    let mut too_deep = Bench::new();
    let result = register(&mut too_deep, chain(17));

    assert!(
        matches!(result, Err(GraphRegistrationError::TooDeep { .. })),
        "{result:?}"
    );
    assert!(too_deep.registry.behavior(&cid("test.chain.g16")).is_none());
}

/// The registry's own chain counts: 16 registered graphs plus one new caller is 17.
#[test]
fn already_registered_levels_count() {
    let mut bench = Bench::new();
    register(&mut bench, chain(16)).expect("16 levels");
    let mut b = B::new();
    let node = b.subgraph("call", call("test.chain.g00", json!({})));

    let result = register(&mut bench, vec![b.graph("test.chain.top", node)]);

    assert!(
        matches!(result, Err(GraphRegistrationError::TooDeep { .. })),
        "{result:?}"
    );
}
