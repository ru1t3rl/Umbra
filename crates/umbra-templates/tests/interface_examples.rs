//! WI-15 review follow-up (ASM-110): the `examples` entry of an assembled interface schema is
//! validated against the schema and left out when the guess would violate it. Requirements:
//! REQ-B-17, ADR 0009, ADR 0014.

#![forbid(unsafe_code)]

mod common;
mod graph_support;

use graph_support::{one_graph, with_interface};

fn examples_of(params: &str) -> Option<serde_json::Value> {
    let graph = one_graph(&with_interface("test.graph.ex", params));
    graph.interface.params.as_json().get("examples").cloned()
}

/// Defaults and the simplest legal values are documented.
#[test]
fn example_uses_defaults_and_simplest_values() {
    let found = examples_of(
        "params = { a = { type = \"number\", exclusiveMinimum = 0, default = 2.5 }, b = { type = \"integer\", minimum = 3 }, c = { type = \"boolean\" } }",
    );

    assert_eq!(
        found,
        Some(serde_json::json!([{ "a": 2.5, "b": 3, "c": false }]))
    );
}

/// A string with `minLength` has no legal empty example, so none is written.
#[test]
fn example_is_left_out_when_the_guess_violates_the_schema() {
    let string = examples_of("params = { s = { type = \"string\", minLength = 3 } }");
    let integer = examples_of("params = { n = { type = \"integer\", minimum = 0.5 } }");

    assert_eq!(string, None);
    assert_eq!(integer, None);
}

/// A graph with only binding-only params documents the empty example.
#[test]
fn binding_only_graph_has_the_empty_example() {
    let found = examples_of("params = { who = { type = \"entity\" } }");

    assert_eq!(found, Some(serde_json::json!([{}])));
}
