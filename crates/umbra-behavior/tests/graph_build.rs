//! WI-8 factory behavior (ASM-095 e, f): empty composites in code-built graphs, non-object
//! params, and a leaf build error that names its node.

#![forbid(unsafe_code)]

mod common;

use std::sync::Arc;

use common::{B, Bench};
use serde_json::json;
use umbra_behavior::GraphFactory;
use umbra_core::{Factory, ParallelPolicy, Status};

/// ASM-095(f): an empty composite is not an error: `sequence` and `parallel all` succeed,
/// `selector` and `parallel any` fail (the loader refuses empty composites, `minItems: 1`).
#[test]
fn empty_composites_give_the_vacuous_result() {
    let table = [
        ("sequence", Status::Success),
        ("selector", Status::Failure),
        ("parallel_all", Status::Success),
        ("parallel_any", Status::Failure),
    ];
    for (kind, expected) in table {
        let bench = Bench::new();
        let mut b = B::new();
        let root = match kind {
            "sequence" => b.seq("root", false, &[]),
            "selector" => b.sel("root", false, &[]),
            "parallel_all" => b.par("root", ParallelPolicy::All, &[]),
            _ => b.par("root", ParallelPolicy::Any, &[]),
        };
        let step = bench.run(b.graph("test.graph.empty", root)).step();
        assert_eq!(step.status, expected, "{kind}");
    }
}

/// ASM-095(e): parameters that are neither an object nor null are a `BuildError`.
#[test]
fn build_with_non_object_params_is_a_build_error() {
    let bench = Bench::new();
    let mut b = B::new();
    let root = b.act("a", &["success"]);
    let factory = GraphFactory::new(
        Arc::new(b.graph("test.graph.params", root)),
        &bench.registry,
    )
    .map_err(|e| e.to_string());
    let Ok(factory) = factory else {
        panic!("factory");
    };

    assert!(factory.build(&json!(5)).is_err());
    assert!(factory.build(&json!([1])).is_err());
    assert!(factory.build(&json!(null)).is_ok());
}

/// ASM-095(e): a leaf's `BuildError` is re-reported with the node's name.
#[test]
fn leaf_build_error_names_the_node() {
    let bench = Bench::new();
    let mut b = B::new();
    let root = b.leaf_cond("my_node", "stub.cond.flag", json!({}));
    let factory = GraphFactory::new(
        Arc::new(b.graph("test.graph.leaf_err", root)),
        &bench.registry,
    )
    .map_err(|e| e.to_string());
    let Ok(factory) = factory else {
        panic!("factory");
    };

    let message = factory.build(&json!({})).err().map(|e| e.message);

    assert!(
        message.is_some_and(|m| m.contains("my_node")),
        "message names the node"
    );
}
