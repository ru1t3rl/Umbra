//! AT-20 (runtime rows): `{ key = .. }` input bindings of `subgraph` nodes, ADR 0014, ASM-088.
//! Requirements: REQ-B-10, REQ-B-11, REQ-B-12 (runtime side), ADR 0014, ASM-071, ASM-088.
//!
//! Step 4 of M3: written before WI-10; every test is ignored with that work item.
//!
//! The parent copies a control slot into its own blackboard key every tick (`stub.action.put`);
//! the subgraph node copies that key into the child's read-only param key before the child ticks;
//! the child logs what it reads (`stub.action.bb`).

#![forbid(unsafe_code)]

mod common;

use cgf_core::{BbType, BbValue, BlackboardError, GraphDef, Provenance, Status};
use common::{B, Bench, call, eid, iface, lines, output, read_line};
use serde_json::json;

fn entity(n: u64) -> BbValue {
    BbValue::Entity(eid(n))
}

/// A child with the interface `fragments` whose only node reads the param `key` as `child`.
fn reading_child(id: &str, fragments: &serde_json::Value, key: &str) -> GraphDef {
    let mut b = B::new();
    let node = b.read("child", key);
    b.graph_with(id, iface(fragments), node)
}

/// A parent with the blackboard key `key` of type `ty`: `sequence [put, subgraph]`, the subgraph
/// call binding `param` to `key`.
fn parent(id: &str, child: &str, ty: BbType, key: &str, param: &str, slot: &str) -> GraphDef {
    let mut b = B::new();
    let put = b.put("put", key, slot);
    let sub = b.subgraph("sub", call(child, json!({})).with_input(param, key));
    let root = b.seq("root", false, &[put, sub]);
    b.graph(id, root).with_blackboard_key(key, ty)
}

/// ADR 0014: the parent's value is copied into the child before each child tick, so a change made
/// by the parent in tick n is seen by the child in tick n.
#[test]
#[ignore = "WI-10"]
fn bound_input_is_copied_in_before_each_child_tick() {
    let mut bench = Bench::new();
    bench.register(vec![
        reading_child(
            "test.graph.child",
            &json!({ "target": { "type": "entity" } }),
            "target",
        ),
        parent(
            "test.graph.parent",
            "test.graph.child",
            BbType::Entity,
            "target",
            "target",
            "t",
        ),
    ]);
    let mut run = bench.run_registered("test.graph.parent", &json!({}));

    bench.controls.set_slot("t", Some(entity(5)));
    let first = run.step();
    bench.controls.set_slot("t", Some(entity(6)));
    let second = run.step();

    assert!(
        first.trace.contains(&read_line("child", Some(&entity(5)))),
        "{:?}",
        first.trace
    );
    assert!(
        second.trace.contains(&read_line("child", Some(&entity(6)))),
        "{:?}",
        second.trace
    );
}

/// ADR 0014: an unset parent key gives an unset child key, also after it was set before (the
/// child key does not keep the stale value).
#[test]
#[ignore = "WI-10"]
fn unset_parent_key_gives_unset_child_key() {
    let mut bench = Bench::new();
    bench.register(vec![
        reading_child(
            "test.graph.child",
            &json!({ "limit": { "type": "integer", "default": 9 } }),
            "limit",
        ),
        parent(
            "test.graph.parent",
            "test.graph.child",
            BbType::Integer,
            "limit",
            "limit",
            "l",
        ),
    ]);
    let mut run = bench.run_registered("test.graph.parent", &json!({}));

    bench.controls.set_slot("l", Some(BbValue::Integer(4)));
    let set = run.step();
    bench.controls.set_slot("l", None);
    let unset = run.step();

    assert!(
        set.trace
            .contains(&read_line("child", Some(&BbValue::Integer(4))))
    );
    assert!(
        unset.trace.contains(&read_line("child", None)),
        "{:?}",
        unset.trace
    );
}

/// ADR 0014, ASM-088: an optional bound param whose parent key is unset stays unset in the child;
/// its default is not substituted for a bound param.
#[test]
#[ignore = "WI-10"]
fn unset_optional_bound_param_stays_unset_not_default() {
    let mut bench = Bench::new();
    bench.register(vec![
        reading_child(
            "test.graph.child",
            &json!({ "limit": { "type": "integer", "default": 9 } }),
            "limit",
        ),
        parent(
            "test.graph.parent",
            "test.graph.child",
            BbType::Integer,
            "limit",
            "limit",
            "l",
        ),
    ]);
    let mut run = bench.run_registered("test.graph.parent", &json!({}));

    let step = run.step();

    assert_eq!(step.status, Status::Success, "the child ran");
    assert!(
        step.trace.contains(&read_line("child", None)),
        "{:?}",
        step.trace
    );
    assert!(
        !step
            .trace
            .contains(&read_line("child", Some(&BbValue::Integer(9))))
    );
}

/// ADR 0014, ASM-088: a required bound param (no default) that is unset makes the node fail
/// without ticking the child, and halts a child that was Running.
#[test]
#[ignore = "WI-10"]
fn unset_required_bound_param_fails_without_ticking_and_halts_child() {
    let mut bench = Bench::new();
    let mut child = B::new();
    let look = child.read("child", "target");
    let work = child.act("work", &["running"]);
    let root = child.seq("root", false, &[look, work]);
    bench.register(vec![
        child.graph_with(
            "test.graph.child",
            iface(&json!({ "target": { "type": "entity" } })),
            root,
        ),
        parent(
            "test.graph.parent",
            "test.graph.child",
            BbType::Entity,
            "target",
            "target",
            "t",
        ),
    ]);
    let mut run = bench.run_registered("test.graph.parent", &json!({}));

    bench.controls.set_slot("t", Some(entity(5)));
    let running = run.step();
    bench.controls.set_slot("t", None);
    let lost = run.step();
    let still_lost = run.step();

    assert_eq!(running.status, Status::Running);
    assert_eq!(lost.status, Status::Failure);
    assert!(
        !lost
            .trace
            .iter()
            .any(|l| l.starts_with("read:child") || l == "tick:work"),
        "{:?}",
        lost.trace
    );
    assert_eq!(lost.trace.iter().filter(|l| *l == "halt:work").count(), 1);
    assert_eq!(still_lost.status, Status::Failure);
    assert_eq!(
        still_lost.trace,
        lines(&["put:t"]),
        "no child tick, no second halt"
    );
}

/// ADR 0014, REQ-B-11: a child's write to an input key is refused (`ReadOnly`), the child keeps
/// seeing the parent's value, and the parent's key is unchanged.
#[test]
#[ignore = "WI-10"]
fn child_write_to_input_key_is_refused_and_parent_unchanged() {
    let mut bench = Bench::new();
    let mut child = B::new();
    let write = child.set("write", "limit", 1);
    let after = child.read("after", "limit");
    let root = child.seq("root", false, &[write, after]);
    bench.register(vec![child.graph_with(
        "test.graph.child",
        iface(&json!({ "limit": { "type": "integer", "default": 9 } })),
        root,
    )]);
    let mut b = B::new();
    let put = b.put("put", "limit", "l");
    let sub = b.subgraph(
        "sub",
        call("test.graph.child", json!({})).with_input("limit", "limit"),
    );
    let own = b.read("parent", "limit");
    let top = b.seq("root", false, &[put, sub, own]);
    bench.register(vec![
        b.graph("test.graph.parent", top)
            .with_blackboard_key("limit", BbType::Integer),
    ]);
    let mut run = bench.run_registered("test.graph.parent", &json!({}));
    bench.controls.set_slot("l", Some(BbValue::Integer(4)));

    let step = run.step();

    let four = Some(&BbValue::Integer(4));
    assert!(
        step.trace
            .iter()
            .any(|l| l.starts_with("set:write=ReadOnly")),
        "{:?}",
        step.trace
    );
    assert!(step.trace.contains(&read_line("after", four)));
    assert!(step.trace.contains(&read_line("parent", four)));
}

/// ADR 0014, REQ-B-11: the child's own keys never reach the parent, even under the same name; only
/// a bound output port does.
#[test]
#[ignore = "WI-10"]
fn child_writes_reach_parent_only_through_bound_outputs() {
    let mut bench = Bench::new();
    let mut child = B::new();
    let w1 = child.set("w1", "result", 1);
    let w2 = child.set("w2", "scratch", 2);
    let root = child.seq("root", false, &[w1, w2]);
    let interface = iface(&json!({})).with_output("found", output(BbType::Integer, "result"));
    bench.register(vec![
        child
            .graph_with("test.graph.child", interface, root)
            .with_blackboard_key("result", BbType::Integer)
            .with_blackboard_key("scratch", BbType::Integer),
    ]);
    let mut b = B::new();
    let sub = b.subgraph(
        "sub",
        call("test.graph.child", json!({})).with_output("found", "outkey"),
    );
    let p_out = b.read("p_out", "outkey");
    let p_result = b.read("p_result", "result");
    let p_scratch = b.read("p_scratch", "scratch");
    let top = b.seq("root", false, &[sub, p_out, p_result, p_scratch]);
    bench.register(vec![
        b.graph("test.graph.parent", top)
            .with_blackboard_key("outkey", BbType::Integer)
            .with_blackboard_key("result", BbType::Integer)
            .with_blackboard_key("scratch", BbType::Integer),
    ]);
    let mut run = bench.run_registered("test.graph.parent", &json!({}));

    let step = run.step();

    assert!(
        step.trace
            .contains(&read_line("p_out", Some(&BbValue::Integer(1)))),
        "{:?}",
        step.trace
    );
    assert!(step.trace.contains(&read_line("p_result", None)));
    assert!(step.trace.contains(&read_line("p_scratch", None)));
}

/// ADR 0014: two subgraph nodes of one child graph, bound to different parent keys, each see their
/// own value.
#[test]
#[ignore = "WI-10"]
fn two_instances_bound_to_different_parent_keys_see_their_own_values() {
    let mut bench = Bench::new();
    let mut child = B::new();
    let look = child.leaf_action(
        "look",
        "stub.action.bb",
        json!({ "op": "read", "key": "n", "label": { "param": "label" } }),
    );
    bench.register(vec![child.graph_with(
        "test.graph.child",
        iface(&json!({ "n": { "type": "integer", "default": 0 }, "label": { "type": "string" } })),
        look,
    )]);
    let mut b = B::new();
    let put1 = b.put("put1", "k1", "one");
    let put2 = b.put("put2", "k2", "two");
    let s1 = b.subgraph(
        "s1",
        call("test.graph.child", json!({ "label": "first" })).with_input("n", "k1"),
    );
    let s2 = b.subgraph(
        "s2",
        call("test.graph.child", json!({ "label": "second" })).with_input("n", "k2"),
    );
    let top = b.seq("root", false, &[put1, put2, s1, s2]);
    bench.register(vec![
        b.graph("test.graph.parent", top)
            .with_blackboard_key("k1", BbType::Integer)
            .with_blackboard_key("k2", BbType::Integer),
    ]);
    let mut run = bench.run_registered("test.graph.parent", &json!({}));
    bench.controls.set_slot("one", Some(BbValue::Integer(1)));
    bench.controls.set_slot("two", Some(BbValue::Integer(2)));

    let step = run.step();

    assert!(
        step.trace
            .contains(&read_line("first", Some(&BbValue::Integer(1)))),
        "{:?}",
        step.trace
    );
    assert!(
        step.trace
            .contains(&read_line("second", Some(&BbValue::Integer(2)))),
        "{:?}",
        step.trace
    );
}

/// ADR 0014, ASM-088: an `integer` parent key bound to a `number` param arrives as `Number(i as
/// f64)`; 2^53 + 1 arrives as 2^53 (the documented loss); an `integer` param keeps `Integer`.
/// Checked for a graph target (types from `Interface::types`) and for a registered behavior (types
/// from `ComponentInfo::param_types`).
#[test]
#[ignore = "WI-10"]
fn integer_key_bound_to_number_param_arrives_as_number() {
    let big: i64 = (1 << 53) + 1;
    let mut bench = Bench::new();
    bench.register(vec![
        reading_child(
            "test.graph.number",
            &json!({ "p": { "type": "number", "default": 0.5 } }),
            "p",
        ),
        reading_child(
            "test.graph.integer",
            &json!({ "p": { "type": "integer", "default": 0 } }),
            "p",
        ),
        parent(
            "test.parent.number",
            "test.graph.number",
            BbType::Integer,
            "p",
            "p",
            "p",
        ),
        parent(
            "test.parent.integer",
            "test.graph.integer",
            BbType::Integer,
            "p",
            "p",
            "p",
        ),
    ]);
    let rows = [
        ("test.parent.number", 3, BbValue::Number(3.0)),
        (
            "test.parent.number",
            big,
            BbValue::Number(9_007_199_254_740_992.0),
        ),
        ("test.parent.integer", 3, BbValue::Integer(3)),
    ];
    for (parent_id, value, expected) in rows {
        let mut run = bench.run_registered(parent_id, &json!({}));
        bench.controls.set_slot("p", Some(BbValue::Integer(value)));

        let step = run.step();

        assert!(
            step.trace.contains(&read_line("child", Some(&expected))),
            "{parent_id} {value}: {:?}",
            step.trace
        );
    }

    // A registered behavior declares the type through ComponentInfo::param_types.
    let mut plugin_bench = Bench::new();
    plugin_bench.register_plugin_behavior_with(
        "test.target.plugin",
        common::info(Provenance::Plugin).with_param_type("p", BbType::Number),
    );
    plugin_bench.register(vec![parent(
        "test.parent.plugin",
        "test.target.plugin",
        BbType::Integer,
        "p",
        "p",
        "p",
    )]);
    let mut run = plugin_bench.run_registered("test.parent.plugin", &json!({}));
    plugin_bench
        .controls
        .set_slot("p", Some(BbValue::Integer(3)));
    let step = run.step();
    assert!(
        step.trace
            .contains(&format!("input:p={:?}", Some(BbValue::Number(3.0)))),
        "{:?}",
        step.trace
    );
}

/// ADR 0014: `set_input` returns the blackboard's error for an undeclared param, a key that is not
/// an input, a wrong type and a non-finite number; a valid call is visible to the next tick, and
/// `None` unsets.
#[test]
#[ignore = "WI-10"]
fn set_input_returns_the_blackboards_errors_and_feeds_the_next_tick() {
    let bench = Bench::new();
    let mut b = B::new();
    let node = b.read("graph", "limit");
    let graph = b
        .graph_with(
            "test.graph.top",
            iface(&json!({ "limit": { "type": "integer", "default": 9 }, "speed": { "type": "number", "default": 1.0 } })),
            node,
        )
        .with_blackboard_key("scratch", BbType::Integer);
    let mut run = bench.run(graph);

    let undeclared = run.instance.set_input("nope", Some(BbValue::Integer(1)));
    let not_input = run.instance.set_input("scratch", Some(BbValue::Integer(1)));
    let wrong_type = run
        .instance
        .set_input("limit", Some(BbValue::String("x".into())));
    let non_finite = run
        .instance
        .set_input("speed", Some(BbValue::Number(f64::NAN)));
    let ok = run.instance.set_input("limit", Some(BbValue::Integer(4)));
    let seen = run.step();
    let cleared = run.instance.set_input("limit", None);
    let unseen = run.step();

    assert!(
        matches!(undeclared, Err(BlackboardError::Undeclared { .. })),
        "{undeclared:?}"
    );
    assert!(
        matches!(not_input, Err(BlackboardError::NotInput { .. })),
        "{not_input:?}"
    );
    assert!(
        matches!(
            wrong_type,
            Err(BlackboardError::WrongType {
                expected: BbType::Integer,
                found: BbType::String,
                ..
            })
        ),
        "{wrong_type:?}"
    );
    assert!(
        matches!(non_finite, Err(BlackboardError::NonFinite { .. })),
        "{non_finite:?}"
    );
    assert_eq!(ok, Ok(()));
    assert_eq!(cleared, Ok(()));
    assert_eq!(
        seen.trace,
        lines(&[&read_line("graph", Some(&BbValue::Integer(4)))])
    );
    assert_eq!(unseen.trace, lines(&[&read_line("graph", None)]));
}
