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

/// A deep subtree that sorts first by ID must not hide from the cap when a later graph reaches it
/// through a long path: `test.z.g00 -> .. -> g08 -> test.m.g00 -> .. -> g10` is 9 + 11 = 20 levels.
#[test]
fn a_deep_subtree_visited_first_does_not_hide_from_the_cap() {
    let link = |id: String, next: String| {
        let mut b = B::new();
        let node = b.subgraph("call", call(&next, json!({})));
        b.graph(&id, node)
    };
    let mut graphs = Vec::new();
    for i in 0..10 {
        graphs.push(link(
            format!("test.m.g{i:02}"),
            format!("test.m.g{:02}", i + 1),
        ));
    }
    graphs.push(script_graph("test.m.g10", "leaf", &["success"]));
    for i in 0..8 {
        graphs.push(link(
            format!("test.z.g{i:02}"),
            format!("test.z.g{:02}", i + 1),
        ));
    }
    graphs.push(link("test.z.g08".to_string(), "test.m.g00".to_string()));
    let mut bench = Bench::new();

    let result = register(&mut bench, graphs.clone());
    graphs.reverse();
    let reversed = register(&mut bench, graphs);

    for r in [result, reversed] {
        assert!(
            matches!(r, Err(GraphRegistrationError::TooDeep { .. })),
            "{r:?}"
        );
    }
    assert!(
        bench.registry.behavior(&cid("test.m.g00")).is_none(),
        "nothing registered"
    );
}

/// The same shape exactly at the limit (5 + 11 = 16) is accepted.
#[test]
fn a_shared_subtree_reached_at_exactly_the_limit_is_accepted() {
    let link = |id: String, next: String| {
        let mut b = B::new();
        let node = b.subgraph("call", call(&next, json!({})));
        b.graph(&id, node)
    };
    let mut graphs = Vec::new();
    for i in 0..10 {
        graphs.push(link(
            format!("test.m.g{i:02}"),
            format!("test.m.g{:02}", i + 1),
        ));
    }
    graphs.push(script_graph("test.m.g10", "leaf", &["success"]));
    for i in 0..4 {
        graphs.push(link(
            format!("test.z.g{i:02}"),
            format!("test.z.g{:02}", i + 1),
        ));
    }
    graphs.push(link("test.z.g04".to_string(), "test.m.g00".to_string()));
    let mut bench = Bench::new();

    assert!(register(&mut bench, graphs).is_ok());
}

/// ASM-099 e: metadata of a registered behavior that loops shows as `TooDeep` (the depth cap
/// stops the walk); a reference inside registered metadata that is not registered is skipped.
#[test]
fn looping_or_dangling_registered_metadata_is_capped_and_skipped() {
    let mut bench = Bench::new();
    let described =
        |target: &str| common::info(cgf_core::Provenance::Plugin).with_subgraph(cid(target), None);
    bench.register_plugin_behavior_with("test.reg.loop", described("test.reg.loop"));
    bench.register_plugin_behavior_with("test.reg.dangling", described("test.reg.nothing"));
    let caller = |name: &str, target: &str| {
        let mut b = B::new();
        let node = b.subgraph("call", call(target, json!({})));
        b.graph(name, node)
    };

    let looped = register(&mut bench, vec![caller("test.reg.c1", "test.reg.loop")]);
    let dangling = register(&mut bench, vec![caller("test.reg.c2", "test.reg.dangling")]);

    assert!(
        matches!(looped, Err(GraphRegistrationError::TooDeep { .. })),
        "{looped:?}"
    );
    assert!(dangling.is_ok(), "{dangling:?}");
}
