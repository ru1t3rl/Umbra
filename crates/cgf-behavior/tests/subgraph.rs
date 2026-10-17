//! AT-2 (`subgraph` status table) and AT-5 (runtime): a `subgraph` node resolves its target
//! through the registry, so file graphs, built-in graphs and plugin behaviors are
//! indistinguishable (REQ-B-9), with defaults and `{ param }` substitution (REQ-B-10).
//! Requirements: REQ-B-7, REQ-B-9, REQ-B-10, REQ-B-11, REQ-B-13, ADR 0011, ADR 0014, ASM-076.
//!
//! Step 4 of M3: written before WI-10; every test is ignored with that work item (they also need
//! the WI-8 and WI-9 interpreter).

#![forbid(unsafe_code)]

mod common;

use cgf_core::{BbType, BbValue, InstantiateError, Provenance, Status};
use common::{
    B, Bench, call, iface, lines, no_interface, output, read_line, script_graph, statuses, traces,
};
use serde_json::json;

fn parent_of(target: &str) -> cgf_core::GraphDef {
    let mut b = B::new();
    let node = b.subgraph("call", call(target, json!({})));
    b.graph("test.graph.parent", node)
}

/// AT-2: the `subgraph` node returns what its child graph returns.
#[test]
#[ignore = "WI-10"]
fn subgraph_status_table() {
    for (word, status) in [
        ("success", Status::Success),
        ("failure", Status::Failure),
        ("running", Status::Running),
    ] {
        let mut bench = Bench::new();
        bench.register(vec![
            script_graph("test.graph.child", "leaf", &[word]),
            parent_of("test.graph.child"),
        ]);
        let mut run = bench.run_registered("test.graph.parent", &json!({}));

        let step = run.step();

        assert_eq!(
            (step.status, step.trace),
            (status, lines(&["tick:leaf"])),
            "{word}"
        );
    }
}

/// REQ-B-9 (AT-5): a code-built file graph, a `Provenance::Builtin` graph and a stub behavior
/// registered with `Provenance::Plugin` behave identically through one `subgraph` node: same
/// statuses, same trace, same intents.
#[test]
#[ignore = "WI-10"]
fn file_builtin_and_plugin_targets_are_indistinguishable() {
    let mut bench = Bench::new();
    let mut builtin = B::new();
    let leaf = builtin.act("leaf", &["running", "success"]);
    bench.register(vec![
        script_graph("test.target.file", "leaf", &["running", "success"]),
        builtin.graph_as(
            "test.target.builtin",
            no_interface(),
            leaf,
            Provenance::Builtin,
        ),
        parent_of("test.target.file"),
    ]);
    bench.register_plugin_behavior("test.target.plugin");
    let mut parents = Vec::new();
    for target in ["test.target.builtin", "test.target.plugin"] {
        let mut b = B::new();
        let node = b.subgraph("call", call(target, json!({})));
        parents.push(b.graph(&format!("test.parent.{}", &target[12..]), node));
    }
    bench.register(parents);

    let mut runs = Vec::new();
    for parent in [
        "test.graph.parent",
        "test.parent.builtin",
        "test.parent.plugin",
    ] {
        runs.push(bench.run_registered(parent, &json!({})).steps(4));
    }

    assert_eq!(
        statuses(&runs[0]),
        [
            Status::Running,
            Status::Success,
            Status::Success,
            Status::Success
        ]
    );
    assert_eq!(runs[0], runs[1], "file and built-in graph");
    assert_eq!(runs[0], runs[2], "file graph and plugin behavior");
}

/// REQ-B-13: each registered target carries its own provenance, whatever it is called through.
#[test]
#[ignore = "WI-10"]
fn registered_info_carries_each_provenance() {
    let mut bench = Bench::new();
    let mut builtin = B::new();
    let leaf = builtin.act("leaf", &["success"]);
    bench.register(vec![
        script_graph("test.target.file", "leaf", &["success"]),
        builtin.graph_as(
            "test.target.builtin",
            no_interface(),
            leaf,
            Provenance::Builtin,
        ),
    ]);
    bench.register_plugin_behavior("test.target.plugin");

    let source = |id: &str| bench.registry.info(&common::cid(id)).map(|i| i.source);

    assert_eq!(source("test.target.file"), Some(Provenance::File));
    assert_eq!(source("test.target.builtin"), Some(Provenance::Builtin));
    assert_eq!(source("test.target.plugin"), Some(Provenance::Plugin));
}

/// REQ-B-10: an omitted optional param takes its default; a literal overrides it.
#[test]
#[ignore = "WI-10"]
fn defaults_are_applied_and_literals_override() {
    let mut bench = Bench::new();
    let mut child = B::new();
    let leaf = child.leaf_action(
        "leaf",
        "stub.action.script",
        json!({ "name": { "param": "who" }, "script": ["success"] }),
    );
    let interface = iface(&json!({ "who": { "type": "string", "default": "x" } }));
    bench.register(vec![child.graph_with("test.graph.child", interface, leaf)]);
    for (index, (given, expected)) in [(json!({}), "tick:x"), (json!({ "who": "y" }), "tick:y")]
        .into_iter()
        .enumerate()
    {
        let mut b = B::new();
        let node = b.subgraph("call", call("test.graph.child", given.clone()));
        let parent = b.graph(&format!("test.parent.d{index}"), node);
        let id = parent.id.as_str().to_string();
        bench.register(vec![parent]);

        let step = bench.run_registered(&id, &json!({})).step();

        assert_eq!(step.trace, lines(&[expected]), "{given}");
    }
}

/// ASM-076, REQ-B-10: `{ param }` references are substituted through two levels: the top graph's
/// param reaches the middle graph's param, which reaches a leaf param.
#[test]
#[ignore = "WI-10"]
fn param_substitution_works_through_two_levels() {
    let mut bench = Bench::new();
    let mut inner = B::new();
    let leaf = inner.leaf_action(
        "leaf",
        "stub.action.script",
        json!({ "name": { "param": "who" }, "script": ["success"] }),
    );
    let inner_graph = inner.graph_with(
        "test.level.inner",
        iface(&json!({ "who": { "type": "string" } })),
        leaf,
    );
    let mut middle = B::new();
    let call_inner = middle.subgraph(
        "call",
        call("test.level.inner", json!({ "who": { "param": "name" } })),
    );
    let middle_graph = middle.graph_with(
        "test.level.middle",
        iface(&json!({ "name": { "type": "string" } })),
        call_inner,
    );
    let mut top = B::new();
    let call_middle = top.subgraph(
        "call",
        call("test.level.middle", json!({ "name": { "param": "label" } })),
    );
    let top_graph = top.graph_with(
        "test.level.top",
        iface(&json!({ "label": { "type": "string", "default": "t" } })),
        call_middle,
    );
    bench.register(vec![inner_graph, middle_graph, top_graph]);

    let default_step = bench.run_registered("test.level.top", &json!({})).step();
    let given_step = bench
        .run_registered("test.level.top", &json!({ "label": "z" }))
        .step();

    assert_eq!(default_step.trace, lines(&["tick:t"]));
    assert_eq!(given_step.trace, lines(&["tick:z"]));
}

/// REQ-B-11, ADR 0014: every interface param is a read-only key of the same name in the child:
/// the literal or default value is set at instantiation, and the child's writes are refused.
#[test]
#[ignore = "WI-10"]
fn every_interface_param_is_a_read_only_key_in_the_child() {
    let mut bench = Bench::new();
    let mut child = B::new();
    let before = child.read("before", "count");
    let write = child.set("write", "count", 7);
    let after = child.read("after", "count");
    let root = child.seq("root", false, &[before, write, after]);
    let interface = iface(&json!({ "count": { "type": "integer", "default": 5 } }));
    bench.register(vec![child.graph_with("test.graph.child", interface, root)]);
    for (given, value) in [(json!({}), 5), (json!({ "count": 9 }), 9)] {
        let mut b = B::new();
        let node = b.subgraph("call", call("test.graph.child", given));
        let parent = b.graph(&format!("test.parent.ro{value}"), node);
        let id = parent.id.as_str().to_string();
        bench.register(vec![parent]);

        let step = bench.run_registered(&id, &json!({})).step();

        let seen = Some(BbValue::Integer(value));
        assert_eq!(step.trace[0], read_line("before", seen.as_ref()));
        assert!(
            step.trace[1].starts_with("set:write=ReadOnly"),
            "{:?}",
            step.trace
        );
        assert_eq!(step.trace[2], read_line("after", seen.as_ref()));
    }
}

/// ASM-078, ADR 0013: a child whose `instantiate` fails makes the parent's `instantiate` fail with
/// `InstantiateError::Subgraph` naming the child.
#[test]
#[ignore = "WI-10"]
fn child_instantiate_failure_makes_the_parents_fail_with_subgraph() {
    let mut bench = Bench::new();
    let mut child = B::new();
    let work = child.act("c", &["success"]);
    let slow = child.cooldown("slow", work, 1e30);
    bench.register(vec![
        child.graph("test.graph.child", slow),
        parent_of("test.graph.child"),
    ]);
    let behavior = bench.registered("test.graph.parent", &json!({}));

    let err = behavior.instantiate(common::rate()).err();

    match err {
        Some(InstantiateError::Subgraph { id, source }) => {
            assert_eq!(id.as_str(), "test.graph.child");
            assert!(matches!(*source, InstantiateError::Duration { hz: 30, .. }));
        }
        other => panic!("expected Subgraph, got {other:?}"),
    }
}

/// ADR 0013: halting the parent halts the child's Running nodes (once).
#[test]
#[ignore = "WI-10"]
fn halting_the_parent_halts_the_child() {
    let mut bench = Bench::new();
    bench.register(vec![
        script_graph("test.graph.child", "leaf", &["running"]),
        parent_of("test.graph.child"),
    ]);
    let mut run = bench.run_registered("test.graph.parent", &json!({}));
    run.step();

    let halted = run.halt();
    let again = run.halt();

    assert_eq!(halted, lines(&["halt:leaf"]));
    assert_eq!(again, Vec::<String>::new());
}

/// REQ-B-10: `output(port)` returns an owned value for a declared output port after a tick, and
/// `None` before the first tick, for an unset key and for an unknown port.
#[test]
#[ignore = "WI-10"]
fn output_returns_owned_values_and_none_for_unset_or_unknown_ports() {
    let bench = Bench::new();
    let mut b = B::new();
    let write = b.set("w", "k", 3);
    let interface = no_interface()
        .with_output("out", output(BbType::Integer, "k"))
        .with_output("never", output(BbType::Integer, "unset_key"));
    let graph = b
        .graph_with("test.graph.out", interface, write)
        .with_blackboard_key("k", BbType::Integer)
        .with_blackboard_key("unset_key", BbType::Integer);
    let mut run = bench.run(graph);
    let before = run.instance.output("out");

    run.step();

    assert_eq!(before, None);
    assert_eq!(run.instance.output("out"), Some(BbValue::Integer(3)));
    assert_eq!(run.instance.output("never"), None);
    assert_eq!(run.instance.output("no_such_port"), None);
}

/// The trace of one `subgraph` node over several ticks is the child's trace (no extra lines).
#[test]
#[ignore = "WI-10"]
fn subgraph_node_adds_no_trace_of_its_own() {
    let mut bench = Bench::new();
    bench.register(vec![
        script_graph("test.graph.child", "leaf", &["running", "success"]),
        parent_of("test.graph.child"),
    ]);
    let mut run = bench.run_registered("test.graph.parent", &json!({}));

    let steps = run.steps(2);

    assert_eq!(traces(&steps), vec![lines(&["tick:leaf"]); 2]);
}
