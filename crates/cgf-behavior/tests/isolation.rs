//! AT-7: scoped blackboards and node state are per instance (REQ-B-11, ADR 0014).
//! Requirements: REQ-B-11, REQ-B-5, ADR 0014, ASM-071.
//!
//! Step 4 of M3: written before WI-10; un-ignored in WI-10.

#![forbid(unsafe_code)]

mod common;

use cgf_core::{BbType, BbValue};
use common::{B, Bench, call, iface, lines, no_interface, read_line};
use serde_json::json;

/// REQ-B-11: two `subgraph` nodes of the same child graph in one parent have separate
/// blackboards: one writes `mine`, the other reads it unset.
#[test]
fn two_instances_of_one_subgraph_in_one_graph_have_separate_blackboards() {
    let mut bench = Bench::new();
    let mut child = B::new();
    let node = child.leaf_action(
        "op",
        "stub.action.bb",
        json!({ "op": { "param": "mode" }, "key": "mine", "label": { "param": "label" }, "value": 1 }),
    );
    let interface = iface(&json!({ "mode": { "type": "string" }, "label": { "type": "string" } }));
    bench.register(vec![
        child
            .graph_with("test.graph.child", interface, node)
            .with_blackboard_key("mine", BbType::Integer),
    ]);
    let mut b = B::new();
    let writer = b.subgraph(
        "a",
        call("test.graph.child", json!({ "mode": "set", "label": "A" })),
    );
    let reader = b.subgraph(
        "b",
        call("test.graph.child", json!({ "mode": "read", "label": "B" })),
    );
    let root = b.seq("root", false, &[writer, reader]);
    bench.register(vec![b.graph("test.graph.parent", root)]);
    let mut run = bench.run_registered("test.graph.parent", &json!({}));

    let step = run.step();

    assert_eq!(step.trace, lines(&["set:A=ok", &read_line("B", None)]));
}

/// REQ-B-11, REQ-B-5: two entities share one behavior (one program) but never node state or
/// blackboard: instance 1 sits at a Running child of a `memory` sequence and has a key set; a
/// second instance starts at the first child and reads the key unset.
#[test]
fn two_entities_with_one_graph_share_no_node_state() {
    let bench = Bench::new();
    let mut b = B::new();
    let look = b.read("look", "k");
    let store = b.set("store", "k", 7);
    let work = b.act("work", &["running"]);
    let root = b.seq("root", true, &[look, store, work]);
    let behavior = bench.behavior_of(
        b.graph("test.graph.shared", root)
            .with_blackboard_key("k", BbType::Integer),
        &json!({}),
    );
    let mut one = bench.runner_at(&behavior, common::rate(), 1);
    let mut two = bench.runner_at(&behavior, common::rate(), 2);

    let one_first = one.step();
    let one_second = one.step();
    let two_first = two.step();

    assert_eq!(
        one_first.trace,
        lines(&[&read_line("look", None), "set:store=ok", "tick:work"])
    );
    assert_eq!(
        one_second.trace,
        lines(&["tick:work"]),
        "instance 1 resumed at the Running child"
    );
    // A shared blackboard would show `Some(Integer(7))` here; shared node state would resume at
    // the Running child and tick only `work`.
    assert_eq!(
        two_first.trace,
        lines(&[&read_line("look", None), "set:store=ok", "tick:work"]),
        "instance 2 starts at the first child and reads the key unset"
    );
}

/// REQ-B-11: a child cannot read its parent's blackboard: a key of the parent is not visible by
/// name in the child.
#[test]
fn child_cannot_read_parent_blackboard() {
    let mut bench = Bench::new();
    let mut child = B::new();
    let look = child.read("child", "secret");
    bench.register(vec![child.graph("test.graph.child", look)]);
    let mut b = B::new();
    let set = b.set("set", "secret", 9);
    let sub = b.subgraph("sub", call("test.graph.child", json!({})));
    let own = b.read("parent", "secret");
    let root = b.seq("root", false, &[set, sub, own]);
    bench.register(vec![
        b.graph("test.graph.parent", root)
            .with_blackboard_key("secret", BbType::Integer),
    ]);
    let mut run = bench.run_registered("test.graph.parent", &json!({}));

    let step = run.step();

    assert_eq!(
        step.trace,
        lines(&[
            "set:set=ok",
            &read_line("child", None),
            &read_line("parent", Some(&BbValue::Integer(9))),
        ])
    );
}

/// REQ-B-11: only bound output ports reach the parent; an unbound port and the child's other keys
/// do not.
#[test]
fn only_bound_outputs_reach_the_parent() {
    let mut bench = Bench::new();
    let mut child = B::new();
    let w1 = child.set("w1", "x", 1);
    let w2 = child.set("w2", "y", 2);
    let root = child.seq("root", false, &[w1, w2]);
    let interface = no_interface()
        .with_output("px", common::output(BbType::Integer, "x"))
        .with_output("py", common::output(BbType::Integer, "y"));
    bench.register(vec![
        child
            .graph_with("test.graph.child", interface, root)
            .with_blackboard_key("x", BbType::Integer)
            .with_blackboard_key("y", BbType::Integer),
    ]);
    let mut b = B::new();
    let sub = b.subgraph(
        "sub",
        call("test.graph.child", json!({})).with_output("px", "got_x"),
    );
    let got_x = b.read("got_x", "got_x");
    let got_y = b.read("got_y", "got_y");
    let root = b.seq("root", false, &[sub, got_x, got_y]);
    bench.register(vec![
        b.graph("test.graph.parent", root)
            .with_blackboard_key("got_x", BbType::Integer)
            .with_blackboard_key("got_y", BbType::Integer),
    ]);
    let mut run = bench.run_registered("test.graph.parent", &json!({}));

    let step = run.step();

    assert_eq!(
        step.trace[2..],
        lines(&[
            &read_line("got_x", Some(&BbValue::Integer(1))),
            &read_line("got_y", None)
        ])[..]
    );
}

/// ADR 0014 / ASM-088: an output whose child key is unset unsets the parent key ("including
/// unset").
#[test]
fn unset_output_unsets_parent_key() {
    let mut bench = Bench::new();
    let mut child = B::new();
    let nothing = child.act("noop", &["success"]);
    let interface = no_interface().with_output("p", common::output(BbType::Integer, "x"));
    bench.register(vec![
        child
            .graph_with("test.graph.child", interface, nothing)
            .with_blackboard_key("x", BbType::Integer),
    ]);
    let mut b = B::new();
    let preset = b.set("preset", "got", 5);
    let sub = b.subgraph(
        "sub",
        call("test.graph.child", json!({})).with_output("p", "got"),
    );
    let after = b.read("after", "got");
    let root = b.seq("root", false, &[preset, sub, after]);
    bench.register(vec![
        b.graph("test.graph.parent", root)
            .with_blackboard_key("got", BbType::Integer),
    ]);
    let mut run = bench.run_registered("test.graph.parent", &json!({}));

    let step = run.step();

    assert_eq!(step.trace.last(), Some(&read_line("after", None)));
}

/// ADR 0014: a blackboard lives as long as its instance; halting does not clear it. Tick 1 stores
/// the key (the guard is true); after a halt, tick 2 (guard false) reads the stored value.
#[test]
fn blackboard_survives_halt() {
    let bench = Bench::new();
    let mut b = B::new();
    let guard = b.cond("g");
    let store = b.set("store", "k", 3);
    let first = b.seq("first", false, &[guard, store]);
    let look = b.read("look", "k");
    let root = b.sel("root", false, &[first, look]);
    let graph = b
        .graph("test.graph.keep", root)
        .with_blackboard_key("k", BbType::Integer);
    let mut run = bench.run(graph);
    bench.controls.set_flag("g", true);
    let stored = run.step();

    run.halt();
    bench.controls.set_flag("g", false);
    let after = run.step();

    assert_eq!(stored.trace, lines(&["cond:g", "set:store=ok"]));
    assert_eq!(
        after.trace,
        lines(&["cond:g", &read_line("look", Some(&BbValue::Integer(3)))])
    );
}
