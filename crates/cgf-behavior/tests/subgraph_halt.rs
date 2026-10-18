//! Halt through a `subgraph` node (WI-10): ADR 0013 amendment 2026-10-18 (case 5), ASM-096.
//! Requirements: REQ-B-8, REQ-B-9, REQ-B-11, ADR 0013, ASM-096.
//!
//! A parent halt of a Running `subgraph` node reaches the child through `halt_running` (Running
//! nodes only, finished nodes keep their state); an instance halt of the caller reaches it through
//! `halt` (everything resets). A child that does not override `halt_running` gets `halt`.

#![forbid(unsafe_code)]

mod common;

use cgf_core::{BbType, NodeId, ParallelPolicy, Status};
use common::{B, Bench, Runner, Step, call, iface, lines, statuses, traces};
use serde_json::json;

/// `n = 5` ticks at 30 Hz.
const FIVE_TICKS_S: f64 = 5.0 / 30.0;

fn fire_ticks(steps: &[Step]) -> Vec<usize> {
    steps
        .iter()
        .enumerate()
        .filter(|(_, s)| s.trace.iter().any(|l| l == "tick:fire"))
        .map(|(i, _)| i)
        .collect()
}

/// The same tree twice: `inner` inline, and `inner` as the graph behind a `subgraph` node.
/// `outer` builds the rest around the node that stands for `inner`.
fn forms(
    inner: impl Fn(&mut B) -> NodeId,
    outer: impl Fn(&mut B, NodeId) -> NodeId,
) -> (Runner, Runner, Bench, Bench) {
    let inlined = Bench::new();
    let mut b = B::new();
    let i = inner(&mut b);
    let root = outer(&mut b, i);
    let inlined_run = inlined.run(b.graph("test.graph.inlined", root));

    let mut split = Bench::new();
    let mut c = B::new();
    let i = inner(&mut c);
    let child = c.graph("test.graph.child", i);
    let mut b = B::new();
    let node = b.subgraph("call", call("test.graph.child", json!({})));
    let root = outer(&mut b, node);
    split.register(vec![child, b.graph("test.graph.outer", root)]);
    let split_run = split.run_registered("test.graph.outer", &json!({}));
    (inlined_run, split_run, inlined, split)
}

fn drive(run: &mut Runner, bench: &Bench, flag: &str, values: &[bool]) -> Vec<Step> {
    values
        .iter()
        .map(|&v| {
            bench.controls.set_flag(flag, v);
            run.step()
        })
        .collect()
}

/// Case 5 / case 1: a cooling cooldown inside the called graph keeps its timer when the caller is
/// pre-empted; the traces and intents equal the inlined form.
#[test]
fn cooling_cooldown_in_a_subgraph_keeps_its_timer_when_the_caller_is_pre_empted() {
    let inner = |b: &mut B| {
        let fire = b.act("fire", &["success"]);
        let cool = b.cooldown("cool", fire, FIVE_TICKS_S);
        let aim = b.act("aim", &["running"]);
        b.sel("pick", false, &[cool, aim])
    };
    let outer = |b: &mut B, pick: NodeId| {
        let near = b.cond("enemy_near");
        b.seq("root", false, &[near, pick])
    };
    let (mut inlined, mut split, bench_i, bench_s) = forms(inner, outer);
    let near = [true, true, false, true, true, true];

    let a = drive(&mut inlined, &bench_i, "enemy_near", &near);
    let b = drive(&mut split, &bench_s, "enemy_near", &near);

    assert_eq!(fire_ticks(&b), [0, 5], "no fire in 1..=4");
    assert_eq!(
        b[2].trace.iter().filter(|l| *l == "halt:aim").count(),
        1,
        "aim is halted once"
    );
    assert_eq!(traces(&a), traces(&b), "same traces");
    assert_eq!(statuses(&a), statuses(&b), "same statuses");
    assert_eq!(
        a.iter().map(|s| &s.intents).collect::<Vec<_>>(),
        b.iter().map(|s| &s.intents).collect::<Vec<_>>(),
        "same intents"
    );
}

/// Case 5 / case 4: a cooldown that finished inside the called graph keeps its timer when the
/// Running `parallel` or `repeat` above the `subgraph` node is halted.
#[test]
fn finished_cooldown_in_a_subgraph_keeps_its_timer_when_the_node_above_is_halted() {
    let inner = |b: &mut B| {
        let fire = b.act("fire", &["success"]);
        b.cooldown("cool", fire, FIVE_TICKS_S)
    };
    let in_parallel = |b: &mut B, cool: NodeId| {
        let guard = b.cond("guard");
        let aim = b.act("aim", &["running"]);
        let par = b.par("par", ParallelPolicy::All, &[cool, aim]);
        b.seq("root", false, &[guard, par])
    };
    let in_repeat = |b: &mut B, cool: NodeId| {
        let guard = b.cond("guard");
        let rep = b.repeat("rep", cool, None);
        b.seq("root", false, &[guard, rep])
    };
    let guard = [true, false, true, true, true, true];
    for (label, outer) in [
        (
            "parallel",
            &in_parallel as &dyn Fn(&mut B, NodeId) -> NodeId,
        ),
        ("repeat", &in_repeat),
    ] {
        let (mut inlined, mut split, bench_i, bench_s) = forms(inner, outer);

        let a = drive(&mut inlined, &bench_i, "guard", &guard);
        let b = drive(&mut split, &bench_s, "guard", &guard);

        assert_eq!(fire_ticks(&b), [0, 5], "{label}");
        assert_eq!(traces(&a), traces(&b), "{label}");
        assert_eq!(statuses(&a), statuses(&b), "{label}");
    }
}

/// Case 3 through a subgraph: an instance halt of the caller resets the cooldown inside the child.
#[test]
fn instance_halt_of_the_caller_resets_the_childs_cooldown() {
    let inner = |b: &mut B| {
        let fire = b.act("fire", &["success"]);
        b.cooldown("cool", fire, FIVE_TICKS_S)
    };
    let (_, mut split, _, _) = forms(inner, |_, node| node);
    let first = split.step();

    let second_without_halt = split.step();
    split.halt();
    let after_halt = split.step();

    assert_eq!(first.trace, lines(&["tick:fire"]));
    assert_eq!(second_without_halt.trace, Vec::<String>::new(), "cooling");
    assert_eq!(after_halt.trace, lines(&["tick:fire"]), "timer cleared");
}

/// ASM-096 d: a built-in or plugin child that does not override `halt_running` gets the full reset,
/// that is its `halt`, when a pre-empting ancestor halts the Running `subgraph` node.
#[test]
fn a_child_without_halt_running_gets_halt_when_the_caller_is_pre_empted() {
    let mut bench = Bench::new();
    bench.register_plugin_behavior("test.target.plugin");
    let mut b = B::new();
    let guard = b.cond("guard");
    let node = b.subgraph("call", call("test.target.plugin", json!({})));
    let root = b.seq("root", false, &[guard, node]);
    bench.register(vec![b.graph("test.graph.parent", root)]);
    let mut run = bench.run_registered("test.graph.parent", &json!({}));

    let steps = drive(&mut run, &bench, "guard", &[true, false, false]);

    assert_eq!(steps[0].status, Status::Running);
    assert_eq!(steps[1].trace, lines(&["cond:guard", "halt:leaf"]));
    assert_eq!(steps[2].trace, lines(&["cond:guard"]), "halted once");
}

/// ASM-096 a: a parent halt of a `subgraph` node that is not Running does not reach the child, so
/// a child that finished is not told to halt.
#[test]
fn parent_halt_of_a_finished_subgraph_node_is_a_no_op() {
    let mut bench = Bench::new();
    bench.register_plugin_behavior("test.target.plugin");
    let mut b = B::new();
    let node = b.subgraph("call", call("test.target.plugin", json!({})));
    let aim = b.act("aim", &["running"]);
    // The plugin returns Running, then Success: the node is finished from the second tick on.
    let par = b.par("par", ParallelPolicy::Any, &[node, aim]);
    bench.register(vec![b.graph("test.graph.parent", par)]);
    let mut run = bench.run_registered("test.graph.parent", &json!({}));

    let steps = run.steps(2);

    assert_eq!(steps[1].status, Status::Success);
    assert!(
        !steps[1].trace.contains(&"halt:leaf".to_string()),
        "a finished child is not halted: {:?}",
        steps[1].trace
    );
}

/// ASM-096 b: an unset required bound input halts a Running child through `halt_running` (a parent
/// halt: the cooling cooldown inside keeps its timer), then returns Failure without ticking it.
#[test]
fn unset_required_input_halts_with_halt_running_and_keeps_finished_state() {
    let mut bench = Bench::new();
    let mut child = B::new();
    let fire = child.act("fire", &["success"]);
    let cool = child.cooldown("cool", fire, FIVE_TICKS_S);
    let aim = child.act("aim", &["running"]);
    let root = child.sel("root", false, &[cool, aim]);
    let mut b = B::new();
    let put = b.put("put", "target", "t");
    let sub = b.subgraph(
        "sub",
        call("test.graph.child", json!({})).with_input("target", "target"),
    );
    let top = b.seq("top", false, &[put, sub]);
    bench.register(vec![
        child.graph_with(
            "test.graph.child",
            iface(&json!({ "target": { "type": "entity" } })),
            root,
        ),
        b.graph("test.graph.parent", top)
            .with_blackboard_key("target", BbType::Entity),
    ]);
    let mut run = bench.run_registered("test.graph.parent", &json!({}));
    let set = Some(cgf_core::BbValue::Entity(common::eid(5)));
    let mut steps = Vec::new();
    for value in [set.clone(), set.clone(), None, set.clone(), set] {
        bench.controls.set_slot("t", value);
        steps.push(run.step());
    }

    assert_eq!(steps[0].trace, lines(&["put:t", "tick:fire"]));
    assert_eq!(steps[1].trace, lines(&["put:t", "tick:aim"]));
    assert_eq!(steps[1].status, Status::Running);
    assert_eq!(
        steps[2].trace,
        lines(&["put:t", "halt:aim"]),
        "halted, not ticked"
    );
    assert_eq!(steps[2].status, Status::Failure);
    assert_eq!(
        steps[3].trace,
        lines(&["put:t", "tick:aim"]),
        "the cooldown kept its timer: no fire"
    );
    assert_eq!(steps[4].trace, lines(&["put:t", "tick:aim"]));
}

/// `BehaviorInstance::halt` contract: an instance halt of the caller reaches the child even when its
/// subgraph node is not Running (ASM-096 c); the child's `halt` must tolerate it.
#[test]
fn instance_halt_calls_the_childs_halt_even_when_it_is_not_running() {
    let mut bench = Bench::new();
    bench.register_plugin_behavior("test.target.plugin");
    let mut b = B::new();
    let node = b.subgraph("call", call("test.target.plugin", json!({})));
    bench.register(vec![b.graph("test.graph.parent", node)]);
    let mut run = bench.run_registered("test.graph.parent", &json!({}));
    let steps = run.steps(2);

    let halted = run.halt();

    assert_eq!(steps[1].status, Status::Success, "the child has finished");
    assert_eq!(halted, lines(&["halt:leaf"]));
}

/// A refused input copy (not expected after validation) halts a Running child as a parent halt
/// and fails the node, so `is_running` stays exact.
#[expect(clippy::expect_used, reason = "tests lock a known-good mutex")]
mod refused {
    use super::*;
    use cgf_core::{
        BbValue, Behavior, BehaviorContext, BehaviorId, BehaviorInstance, BlackboardError,
        EntityView, InstantiateError, IntentBuffer, TickRate,
    };
    use std::sync::Arc;

    struct Refusing(BehaviorId, common::Log);
    struct RefusingInstance(common::Log, u32);

    impl Behavior for Refusing {
        fn id(&self) -> &BehaviorId {
            &self.0
        }
        fn instantiate(&self, _: TickRate) -> Result<Box<dyn BehaviorInstance>, InstantiateError> {
            Ok(Box::new(RefusingInstance(Arc::clone(&self.1), 0)))
        }
    }
    impl BehaviorInstance for RefusingInstance {
        fn tick(
            &mut self,
            _: &mut BehaviorContext<'_>,
            _: &EntityView<'_>,
            _: &mut IntentBuffer<'_>,
        ) -> Status {
            self.0.lock().expect("log").push("tick:child".to_string());
            Status::Running
        }
        fn halt(&mut self) {
            self.0.lock().expect("log").push("halt:full".to_string());
        }
        fn halt_running(&mut self) {
            self.0.lock().expect("log").push("halt:running".to_string());
        }
        fn output(&self, _: &str) -> Option<BbValue> {
            None
        }
        fn set_input(&mut self, param: &str, _: Option<BbValue>) -> Result<(), BlackboardError> {
            self.1 += 1;
            if self.1 > 1 {
                return Err(BlackboardError::Undeclared { key: param.into() });
            }
            Ok(())
        }
    }

    #[test]
    fn refused_input_halts_a_running_child_and_fails() {
        let mut bench = Bench::new();
        let id = common::bid("test.target.refusing");
        bench
            .registry
            .register_behavior(
                Arc::new(common::StubFactory::<dyn Behavior>::new(
                    "test.target.refusing",
                    Box::new({
                        let log = Arc::clone(&bench.log);
                        move |_, _| Ok(Arc::new(Refusing(id.clone(), Arc::clone(&log))))
                    }),
                )),
                common::info(cgf_core::Provenance::Plugin).with_param_type("p", BbType::Integer),
            )
            .expect("register");
        let mut b = B::new();
        let put = b.put("put", "k", "slot");
        let node = b.subgraph(
            "call",
            call("test.target.refusing", json!({})).with_input("p", "k"),
        );
        let root = b.seq("root", false, &[put, node]);
        bench.register(vec![
            b.graph("test.graph.parent", root)
                .with_blackboard_key("k", BbType::Integer),
        ]);
        let mut run = bench.run_registered("test.graph.parent", &json!({}));
        bench
            .controls
            .set_slot("slot", Some(cgf_core::BbValue::Integer(1)));

        let steps = run.steps(3);

        assert_eq!(steps[0].status, Status::Running);
        assert_eq!(
            steps[1].status,
            Status::Failure,
            "second set_input is refused"
        );
        assert!(
            steps[1].trace.contains(&"halt:running".to_string()),
            "{:?}",
            steps[1].trace
        );
        assert!(!steps[1].trace.contains(&"tick:child".to_string()));
        assert!(
            !steps[2].trace.contains(&"halt:running".to_string()),
            "no second halt"
        );
    }
}
