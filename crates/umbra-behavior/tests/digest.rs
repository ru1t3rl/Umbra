//! AT-19 (behavior part, M7): `BehaviorInstance::digest` for state hash v3. Requirements: REQ-K-4,
//! REQ-B-16, D-30, ADR 0055, ASM-634.
//!
//! The interpreter implements it for every node kind (statuses, memory indices, timers in ticks,
//! repeat counts, FSM state, subgraph children, blackboard values in key order). `[editor]` data is
//! not part of a `GraphDef` (REQ-B-16), and node names do not change a digest. A plugin behavior
//! instance keeps the trait default: `Opaque`, nothing written. Written in M7 WI-1 against the
//! default `digest`; the rows that need the interpreter's implementation are ignored until M7 WI-15,
//! the opaque-default row passes now.

#![forbid(unsafe_code)]

mod common;

use common::{B, Bench, Runner, call, script_graph};
use serde_json::json;
use umbra_core::{
    DigestSupport, FsmDef, GraphDef, ParallelPolicy, StateDef, StateHash, StateHashVersion,
    StateHasher,
};

/// The digest support and the v3 hash of everything one instance writes.
fn digest_of(run: &Runner) -> (DigestSupport, StateHash) {
    let mut h = StateHasher::with_version(StateHashVersion::V3);
    let support = run.instance.digest(&mut h);
    (support, h.finish())
}

/// One graph per node kind (REQ-B-7), each with a leaf that keeps Running.
fn graphs() -> Vec<(&'static str, GraphDef)> {
    let one = |kind: &'static str, f: &dyn Fn(&mut B) -> umbra_core::NodeId| {
        let mut b = B::new();
        let root = f(&mut b);
        (kind, b.graph("test.graph.kind", root))
    };
    vec![
        one("action", &|b| b.act("a", &["running"])),
        one("condition", &|b| b.cond("c")),
        one("sequence", &|b| {
            let (x, y) = (b.act("x", &["success"]), b.act("y", &["running"]));
            b.seq("root", true, &[x, y])
        }),
        one("selector", &|b| {
            let (x, y) = (b.act("x", &["failure"]), b.act("y", &["running"]));
            b.sel("root", true, &[x, y])
        }),
        one("parallel", &|b| {
            let (x, y) = (b.act("x", &["running"]), b.act("y", &["success"]));
            b.par("root", ParallelPolicy::All, &[x, y])
        }),
        one("inverter", &|b| {
            let c = b.act("c", &["running"]);
            b.inverter("root", c)
        }),
        one("succeed_always", &|b| {
            let c = b.act("c", &["running"]);
            b.succeed_always("root", c)
        }),
        one("repeat", &|b| {
            let c = b.act("c", &["success"]);
            b.repeat("root", c, Some(5))
        }),
        one("cooldown", &|b| {
            let c = b.act("c", &["success"]);
            b.cooldown("root", c, 1.0)
        }),
        one("timeout", &|b| {
            let c = b.act("c", &["running"]);
            b.timeout("root", c, 2.0)
        }),
        one("fsm", &|b| {
            let (s0, s1) = (b.act("s0", &["running"]), b.act("s1", &["running"]));
            b.fsm(
                "root",
                FsmDef::new(0, vec![StateDef::new("a", s0), StateDef::new("b", s1)]),
            )
        }),
    ]
}

/// ADR 0055: an instance that does not override `digest` (the stub behavior of the bench stands for a
/// plugin instance) is opaque and writes nothing. The real plugin-host row comes with WI-2/WI-15.
#[test]
fn given_an_instance_without_a_digest_override_then_it_is_opaque_and_writes_nothing() {
    let mut bench = Bench::new();
    bench.register_plugin_behavior("test.target.plugin");
    let mut run = bench.run_registered("test.target.plugin", &json!({}));
    run.step();
    let mut hasher = StateHasher::new();
    let before = hasher.clone();

    let support = run.instance.digest(&mut hasher);

    assert_eq!(support, DigestSupport::Opaque);
    assert_eq!(hasher, before);
}

/// ADR 0055: every node kind of the interpreter writes its state.
#[test]
#[ignore = "M7 WI-15"]
fn given_each_node_kind_then_the_interpreter_instance_writes_a_digest() {
    for (kind, def) in graphs() {
        let bench = Bench::new();
        let mut run = bench.run(def);
        run.steps(3);

        let (support, _) = digest_of(&run);

        assert_eq!(support, DigestSupport::Written, "{kind}");
    }
}

/// ADR 0055: a subgraph node digests its child instance, so child state shows in the parent.
#[test]
#[ignore = "M7 WI-15"]
fn given_a_subgraph_call_then_child_state_changes_the_parent_digest() {
    let build = |script: &[&str]| {
        let mut bench = Bench::new();
        let mut p = B::new();
        let node = p.subgraph("call", call("test.graph.child", json!({})));
        bench.register(vec![
            script_graph("test.graph.child", "leaf", script),
            p.graph("test.graph.parent", node),
        ]);
        bench.run_registered("test.graph.parent", &json!({}))
    };
    let mut a = build(&["running", "running", "success"]);
    let mut b = build(&["running", "success", "success"]);

    a.steps(2);
    b.steps(2);

    assert_eq!(digest_of(&a).0, DigestSupport::Written);
    assert_ne!(digest_of(&a).1, digest_of(&b).1);
}

/// ADR 0055, D-30: a `cooldown` timer is hashed in ticks, so the digest changes with every tick the
/// timer runs, although the node's status does not.
#[test]
#[ignore = "M7 WI-15"]
fn given_a_cooling_down_node_then_the_digest_changes_on_every_tick_of_the_timer() {
    let (_, def) = graphs()
        .into_iter()
        .find(|(kind, _)| *kind == "cooldown")
        .expect("cooldown graph");
    let bench = Bench::new();
    let mut run = bench.run(def);
    run.step(); // the child succeeds and the cooldown starts

    let mut seen = Vec::new();
    for _ in 0..5 {
        run.step();
        seen.push(digest_of(&run).1);
    }

    seen.dedup();
    assert_eq!(seen.len(), 5, "five different digests while the timer runs");
}

/// ADR 0055: two instances of one graph with the same history have the same digest.
#[test]
#[ignore = "M7 WI-15"]
fn given_two_instances_with_the_same_history_then_digests_are_equal() {
    for (kind, def) in graphs() {
        let bench = Bench::new();
        let mut a = bench.run(def.clone());
        let mut b = bench.run(def);
        a.steps(4);
        b.steps(4);

        assert_eq!(digest_of(&a), digest_of(&b), "{kind}");
    }
}

/// ADR 0055: different history gives a different digest where the node keeps state (a `repeat`
/// that has counted two successes versus one).
#[test]
#[ignore = "M7 WI-15"]
fn given_different_repeat_counts_then_digests_differ() {
    let (_, def) = graphs()
        .into_iter()
        .find(|(kind, _)| *kind == "repeat")
        .expect("repeat graph");
    let bench = Bench::new();
    let (mut a, mut b) = (bench.run(def.clone()), bench.run(def));
    a.steps(2);
    b.steps(1);

    assert_ne!(digest_of(&a).1, digest_of(&b).1);
}

/// REQ-B-16, ADR 0055: node names are not state; the same structure under other names digests
/// alike.
#[test]
#[ignore = "M7 WI-15"]
fn given_other_node_names_then_the_digest_is_unchanged() {
    let build = |leaf: &str, root: &str| {
        let mut b = B::new();
        let c = b.act(leaf, &["success"]);
        let r = b.cooldown(root, c, 1.0);
        Bench::new().run(b.graph("test.graph.names", r))
    };
    let (mut a, mut b) = (build("one", "alpha"), build("two", "beta"));
    a.steps(3);
    b.steps(3);

    assert_eq!(digest_of(&a), digest_of(&b));
}
