//! AT-4 (runtime) and AT-2 (`fsm` status table): the finite state machine node.
//! Requirements: REQ-B-7 (fsm), REQ-B-8, REQ-B-12 (runtime side), ADR 0013, ASM-072.
//!
//! Step 4 of M3: written before WI-9, un-ignored in WI-9. Per tick an FSM takes at most one guard (`when`) transition, then ticks
//! the current state's node once, then takes at most one completion (`on`) transition.

#![forbid(unsafe_code)]

mod common;

use cgf_core::{
    ExitDef, FsmDef, NodeId, StateDef, Status, TransitionDef, TransitionTarget, Trigger,
};
use common::{B, Bench, Runner, lines, statuses, traces};

fn state(name: &str, node: NodeId) -> StateDef {
    StateDef::new(name, node)
}

fn when(from: usize, to: usize, guard: NodeId) -> TransitionDef {
    TransitionDef::new(from, TransitionTarget::State(to), Trigger::When(guard))
}

fn on(from: usize, to: TransitionTarget, status: Status) -> TransitionDef {
    TransitionDef::new(from, to, Trigger::On(status))
}

fn run_fsm(bench: &Bench, mut b: B, def: FsmDef) -> Runner {
    let root = b.fsm("machine", def);
    bench.run(b.graph("test.graph.fsm", root))
}

/// ADR 0013: an FSM starts in its `initial` state: only that state's node is ticked.
#[test]
fn starts_in_initial_state() {
    for (initial, expected) in [(0, "tick:s0"), (1, "tick:s1")] {
        let bench = Bench::new();
        let mut b = B::new();
        let s0 = b.act("s0", &["running"]);
        let s1 = b.act("s1", &["running"]);
        let def = FsmDef::new(initial, vec![state("a", s0), state("b", s1)]);
        let mut run = run_fsm(&bench, b, def);

        let step = run.step();

        assert_eq!(step.trace, lines(&[expected]), "initial {initial}");
        assert_eq!(step.status, Status::Running);
    }
}

/// ADR 0013: the first `when` transition of the current state, in file order, whose guard
/// succeeds fires; the new state's node is ticked in the same tick.
#[test]
fn first_true_guard_in_file_order_wins() {
    for (g1, g2, expected) in [
        (true, true, "tick:s1"),
        (false, true, "tick:s2"),
        (true, false, "tick:s1"),
    ] {
        let bench = Bench::new();
        let mut b = B::new();
        let s0 = b.act("s0", &["running"]);
        let s1 = b.act("s1", &["running"]);
        let s2 = b.act("s2", &["running"]);
        let guard1 = b.cond("g1");
        let guard2 = b.cond("g2");
        let def = FsmDef::new(0, vec![state("a", s0), state("b", s1), state("c", s2)])
            .with_transition(when(0, 1, guard1))
            .with_transition(when(0, 2, guard2));
        bench.controls.set_flag("g1", g1);
        bench.controls.set_flag("g2", g2);
        let mut run = run_fsm(&bench, b, def);

        let step = run.step();

        assert!(
            step.trace.contains(&expected.to_string()),
            "{g1} {g2}: {:?}",
            step.trace
        );
        let others = ["tick:s0", "tick:s1", "tick:s2"]
            .into_iter()
            .filter(|l| *l != expected);
        for other in others {
            assert!(
                !step.trace.contains(&other.to_string()),
                "{g1} {g2}: {:?}",
                step.trace
            );
        }
    }
}

/// ADR 0013: a guard that fails (or no guard at all) leaves the machine in its state.
#[test]
fn guard_that_fails_keeps_the_state() {
    let bench = Bench::new();
    let mut b = B::new();
    let s0 = b.act("s0", &["running"]);
    let s1 = b.act("s1", &["running"]);
    let guard = b.cond("g");
    let def =
        FsmDef::new(0, vec![state("a", s0), state("b", s1)]).with_transition(when(0, 1, guard));
    let mut run = run_fsm(&bench, b, def);

    let steps = run.steps(3);

    assert_eq!(
        traces(&steps),
        vec![lines(&["cond:g", "tick:s0"]); 3],
        "the guard is evaluated every tick, the state keeps running"
    );
}

/// ADR 0012: a guard may be a composite of conditions.
#[test]
fn guard_may_be_a_sequence_of_conditions() {
    let bench = Bench::new();
    let mut b = B::new();
    let s0 = b.act("s0", &["running"]);
    let s1 = b.act("s1", &["running"]);
    let g1 = b.cond("g1");
    let g2 = b.cond("g2");
    let both = b.seq("both", false, &[g1, g2]);
    let def =
        FsmDef::new(0, vec![state("a", s0), state("b", s1)]).with_transition(when(0, 1, both));
    bench.controls.set_flag("g1", true);
    let mut run = run_fsm(&bench, b, def);

    let only_one = run.step();
    bench.controls.set_flag("g2", true);
    let both_true = run.step();

    assert!(only_one.trace.contains(&"tick:s0".to_string()));
    assert!(both_true.trace.contains(&"tick:s1".to_string()));
}

/// ADR 0013: after the state node finishes, the first `on success` / `on failure` transition fires;
/// the new state's node is first ticked next tick.
#[test]
fn on_success_and_on_failure_transitions() {
    for (script, target) in [("success", "tick:s1"), ("failure", "tick:s2")] {
        let bench = Bench::new();
        let mut b = B::new();
        let s0 = b.act("s0", &[script]);
        let s1 = b.act("s1", &["running"]);
        let s2 = b.act("s2", &["running"]);
        let def = FsmDef::new(0, vec![state("a", s0), state("b", s1), state("c", s2)])
            .with_transition(on(0, TransitionTarget::State(1), Status::Success))
            .with_transition(on(0, TransitionTarget::State(2), Status::Failure));
        let mut run = run_fsm(&bench, b, def);

        let steps = run.steps(2);

        assert_eq!(steps[0].trace, lines(&["tick:s0"]), "{script}");
        assert_eq!(steps[1].trace, lines(&[target]), "{script}");
        assert_eq!(
            steps[0].status,
            Status::Running,
            "{script}: the FSM is not finished"
        );
    }
}

/// ADR 0013, ASM-072, AT-2: entering an exit returns its mapped status and resets the machine to
/// `initial`; every other tick is Running.
#[test]
fn exit_maps_to_status_and_resets() {
    for (script, exit_status) in [("success", Status::Success), ("failure", Status::Failure)] {
        let bench = Bench::new();
        let mut b = B::new();
        let s0 = b.act("s0", &[script]);
        let s1 = b.act("s1", &["running"]);
        let def = FsmDef::new(0, vec![state("a", s0), state("b", s1)])
            .with_exit(ExitDef::new("done", exit_status))
            .with_transition(on(0, TransitionTarget::Exit(0), exit_status));
        let mut run = run_fsm(&bench, b, def);

        let steps = run.steps(3);

        assert_eq!(
            statuses(&steps),
            vec![exit_status; 3],
            "{script}: reset, then again"
        );
        assert_eq!(traces(&steps), vec![lines(&["tick:s0"]); 3], "{script}");
    }
}

/// ADR 0013: a guard transition to an exit returns the exit's status at once, without ticking the
/// state node, and halts a Running state node.
#[test]
fn guard_to_exit_returns_the_exit_status_and_halts_the_state() {
    let bench = Bench::new();
    let mut b = B::new();
    let s0 = b.act("s0", &["running"]);
    let guard = b.cond("g");
    let def = FsmDef::new(0, vec![state("a", s0)])
        .with_exit(ExitDef::new("abort", Status::Failure))
        .with_transition(TransitionDef::new(
            0,
            TransitionTarget::Exit(0),
            Trigger::When(guard),
        ));
    let mut run = run_fsm(&bench, b, def);
    let started = run.step();
    bench.controls.set_flag("g", true);

    let aborted = run.step();

    assert_eq!(started.status, Status::Running);
    assert_eq!(aborted.status, Status::Failure);
    assert!(!aborted.trace.contains(&"tick:s0".to_string()));
    assert_eq!(aborted.trace.iter().filter(|l| *l == "halt:s0").count(), 1);
}

/// ADR 0013: a guard transition halts the Running node of the state it leaves, once.
#[test]
fn guard_transition_halts_a_running_state_node() {
    let bench = Bench::new();
    let mut b = B::new();
    let s0 = b.act("s0", &["running"]);
    let s1 = b.act("s1", &["running"]);
    let guard = b.cond("g");
    let def =
        FsmDef::new(0, vec![state("a", s0), state("b", s1)]).with_transition(when(0, 1, guard));
    let mut run = run_fsm(&bench, b, def);
    run.step();
    bench.controls.set_flag("g", true);

    let moved = run.step();

    assert_eq!(moved.trace.iter().filter(|l| *l == "halt:s0").count(), 1);
    assert!(moved.trace.contains(&"tick:s1".to_string()));
}

/// ADR 0013: at most one guard transition and one completion transition per tick.
#[test]
fn at_most_one_guard_and_one_completion_transition_per_tick() {
    // Guards: s0 -g-> s1 -g-> s2 with g always true: one hop per tick.
    let bench = Bench::new();
    let mut b = B::new();
    let nodes: Vec<NodeId> = ["s0", "s1", "s2"]
        .iter()
        .map(|n| b.act(n, &["running"]))
        .collect();
    let g_a = b.cond("g1");
    let g_b = b.cond("g2");
    let def = FsmDef::new(
        0,
        nodes
            .iter()
            .enumerate()
            .map(|(i, n)| state(&format!("st{i}"), *n))
            .collect(),
    )
    .with_transition(when(0, 1, g_a))
    .with_transition(when(1, 2, g_b));
    bench.controls.set_flag("g1", true);
    bench.controls.set_flag("g2", true);
    let mut run = run_fsm(&bench, b, def);
    let guards = run.steps(3);

    // Completions: s0 -success-> s1 -success-> s2: the node of the new state waits a tick.
    let bench = Bench::new();
    let mut b = B::new();
    let nodes: Vec<NodeId> = ["s0", "s1", "s2"]
        .iter()
        .map(|n| b.act(n, &["success"]))
        .collect();
    let def = FsmDef::new(
        0,
        nodes
            .iter()
            .enumerate()
            .map(|(i, n)| state(&format!("st{i}"), *n))
            .collect(),
    )
    .with_transition(on(0, TransitionTarget::State(1), Status::Success))
    .with_transition(on(1, TransitionTarget::State(2), Status::Success));
    let mut run = run_fsm(&bench, b, def);
    let completions = run.steps(3);

    let ticked = |steps: &[common::Step], i: usize| -> Vec<String> {
        steps[i]
            .trace
            .iter()
            .filter(|l| l.starts_with("tick:"))
            .cloned()
            .collect()
    };
    assert_eq!(ticked(&guards, 0), lines(&["tick:s1"]));
    assert_eq!(ticked(&guards, 1), lines(&["tick:s2"]));
    assert_eq!(ticked(&completions, 0), lines(&["tick:s0"]));
    assert_eq!(ticked(&completions, 1), lines(&["tick:s1"]));
    assert_eq!(ticked(&completions, 2), lines(&["tick:s2"]));
}

/// ADR 0013: one guard transition and one completion transition may both happen in one tick: the
/// guard moves to s1, s1's node finishes, its `on success` moves to s2; s2's node waits.
#[test]
fn a_guard_and_a_completion_transition_can_share_a_tick() {
    let bench = Bench::new();
    let mut b = B::new();
    let s0 = b.act("s0", &["running"]);
    let s1 = b.act("s1", &["success"]);
    let s2 = b.act("s2", &["running"]);
    let guard = b.cond("g");
    let def = FsmDef::new(0, vec![state("a", s0), state("b", s1), state("c", s2)])
        .with_transition(when(0, 1, guard))
        .with_transition(on(1, TransitionTarget::State(2), Status::Success));
    bench.controls.set_flag("g", true);
    let mut run = run_fsm(&bench, b, def);

    let steps = run.steps(2);

    assert_eq!(steps[0].trace, lines(&["cond:g", "tick:s1"]));
    assert_eq!(steps[1].trace, lines(&["tick:s2"]));
}

/// ADR 0013: a state node that finished without a matching `on` restarts next tick; the machine
/// stays Running.
#[test]
fn finished_state_without_on_restarts() {
    let bench = Bench::new();
    let mut b = B::new();
    let s0 = b.act("s0", &["success"]);
    let def = FsmDef::new(0, vec![state("a", s0)]);
    let mut run = run_fsm(&bench, b, def);

    let steps = run.steps(3);

    assert_eq!(statuses(&steps), vec![Status::Running; 3]);
    assert_eq!(traces(&steps), vec![lines(&["tick:s0"]); 3]);
}

/// ASM-072: an FSM without exits returns Running until it is halted, whatever its states do.
#[test]
fn fsm_without_exits_runs_until_halted() {
    let bench = Bench::new();
    let mut b = B::new();
    let s0 = b.act("s0", &["success", "failure", "running"]);
    let def = FsmDef::new(0, vec![state("a", s0)]);
    let mut run = run_fsm(&bench, b, def);

    let steps = run.steps(6);

    assert_eq!(statuses(&steps), vec![Status::Running; 6]);
}

/// ADR 0013: halting an FSM resets it to `initial` (and halts the Running state node).
#[test]
fn halt_resets_to_initial() {
    let bench = Bench::new();
    let mut b = B::new();
    let s0 = b.act("s0", &["running"]);
    let s1 = b.act("s1", &["running"]);
    let guard = b.cond("g");
    let def =
        FsmDef::new(0, vec![state("a", s0), state("b", s1)]).with_transition(when(0, 1, guard));
    bench.controls.set_flag("g", true);
    let mut run = run_fsm(&bench, b, def);
    let moved = run.step();
    bench.controls.set_flag("g", false);

    let halted = run.halt();
    let after = run.step();

    assert!(moved.trace.contains(&"tick:s1".to_string()));
    assert_eq!(halted, lines(&["halt:s1"]));
    assert_eq!(after.trace, lines(&["cond:g", "tick:s0"]));
}

/// ADR 0013, ASM-072: an inner FSM is a state node; its exit status is the state node's result for
/// the outer machine's `on` transitions.
#[test]
fn nested_fsm_exit_drives_outer_transition() {
    let bench = Bench::new();
    let mut b = B::new();
    let inner_state = b.act("inner", &["running", "success"]);
    let inner_def = FsmDef::new(0, vec![state("work", inner_state)])
        .with_exit(ExitDef::new("out", Status::Success))
        .with_transition(on(0, TransitionTarget::Exit(0), Status::Success));
    let inner = b.fsm("inner_machine", inner_def);
    let after = b.act("after", &["running"]);
    let outer_def = FsmDef::new(0, vec![state("first", inner), state("second", after)])
        .with_transition(on(0, TransitionTarget::State(1), Status::Success));
    let mut run = run_fsm(&bench, b, outer_def);

    let steps = run.steps(3);

    assert_eq!(
        traces(&steps),
        [
            lines(&["tick:inner"]),
            lines(&["tick:inner"]),
            lines(&["tick:after"])
        ]
    );
    assert_eq!(statuses(&steps), vec![Status::Running; 3]);
}

/// AT-2: the status table of the `fsm` node: Running while in a state, the exit's status when an
/// exit is entered.
#[test]
fn fsm_status_table() {
    let bench = Bench::new();
    let mut b = B::new();
    let s0 = b.act("s0", &["running", "running", "success"]);
    let def = FsmDef::new(0, vec![state("a", s0)])
        .with_exit(ExitDef::new("ok", Status::Success))
        .with_transition(on(0, TransitionTarget::Exit(0), Status::Success));
    let mut run = run_fsm(&bench, b, def);

    let steps = run.steps(3);

    assert_eq!(
        statuses(&steps),
        [Status::Running, Status::Running, Status::Success]
    );
}

// --- halt scope (ADR 0013 amendment 2026-10-18), guard side effects (plan WI-9 Risks) -----------

const FIVE_TICKS_S: f64 = 5.0 / 30.0;

fn ticked(step: &common::Step, line: &str) -> bool {
    step.trace.iter().any(|l| l == line)
}

/// Amendment case 4 for the FSM: state `a` holds `cooldown(fire)`. After the cooldown's child
/// finishes the state node is not Running, so a guard transition away and back is a no-op halt
/// and the timer survives (no `fire` tick until tick 5).
#[test]
fn cooling_cooldown_in_a_state_survives_a_guard_transition_away_and_back() {
    let bench = Bench::new();
    let mut b = B::new();
    let fire = b.act("fire", &["success"]);
    let cool = b.cooldown("cool", fire, FIVE_TICKS_S);
    let other = b.act("other", &["running"]);
    let away = b.cond("away");
    let back = b.cond("back");
    let def = FsmDef::new(0, vec![state("a", cool), state("b", other)])
        .with_transition(when(0, 1, away))
        .with_transition(when(1, 0, back));
    let mut run = run_fsm(&bench, b, def);
    let mut steps = Vec::new();
    for (away_now, back_now) in [
        (false, false),
        (true, false),
        (false, true),
        (false, false),
        (false, false),
        (false, false),
    ] {
        bench.controls.set_flag("away", away_now);
        bench.controls.set_flag("back", back_now);
        steps.push(run.step());
    }

    let fires: Vec<usize> = (0..steps.len())
        .filter(|&i| ticked(&steps[i], "tick:fire"))
        .collect();
    assert_eq!(
        fires,
        [0, 5],
        "tick 0 fires; ticks 1..=4 cool (2 is in b, 3 is back in a)"
    );
    assert!(ticked(&steps[1], "tick:other"));
    assert!(!ticked(&steps[1], "halt:fire"));
}

/// The instance halt is a full reset: the FSM returns to `initial` and a cooling cooldown inside
/// a state forgets its timer.
#[test]
fn instance_halt_resets_the_fsm_and_the_cooldown_inside() {
    let bench = Bench::new();
    let mut b = B::new();
    let fire = b.act("fire", &["success"]);
    let cool = b.cooldown("cool", fire, FIVE_TICKS_S);
    let other = b.act("other", &["running"]);
    let go = b.cond("go");
    let def = FsmDef::new(0, vec![state("a", cool), state("b", other)])
        .with_transition(when(0, 1, go))
        .with_transition(TransitionDef::new(
            0,
            TransitionTarget::State(1),
            Trigger::On(Status::Success),
        ));
    let mut run = run_fsm(&bench, b, def);
    let first = run.step();
    let second = run.step();

    let halted = run.halt();
    let after = run.step();

    assert!(ticked(&first, "tick:fire"));
    assert!(ticked(&second, "tick:other"), "on success moved to b");
    assert_eq!(halted, lines(&["halt:other"]));
    assert!(ticked(&after, "tick:fire"), "back in `a` with no timer");
    assert!(ticked(&after, "cond:go"));
}

/// A parent halt of a Running FSM resets it to `initial` (and halts its Running state node); a
/// parent halt of a FSM that is not Running changes nothing.
#[test]
fn parent_halt_resets_a_running_fsm_to_initial() {
    let bench = Bench::new();
    let mut b = B::new();
    let s0 = b.act("s0", &["running"]);
    let s1 = b.act("s1", &["running"]);
    let go = b.cond("go");
    let fsm = b.fsm(
        "machine",
        FsmDef::new(0, vec![state("a", s0), state("b", s1)]).with_transition(when(0, 1, go)),
    );
    let gate = b.cond("gate");
    let root = b.seq("root", false, &[gate, fsm]);
    let mut run = bench.run(b.graph("test.graph.fsm_parent_halt", root));
    bench.controls.set_flag("gate", true);
    bench.controls.set_flag("go", true);
    let moved = run.step();
    bench.controls.set_flag("gate", false);
    let preempted = run.step();
    bench.controls.set_flag("gate", true);
    bench.controls.set_flag("go", false);
    let again = run.step();

    assert!(ticked(&moved, "tick:s1"));
    assert_eq!(preempted.trace, lines(&["cond:gate", "halt:s1"]));
    assert_eq!(
        again.trace,
        lines(&["cond:gate", "cond:go", "tick:s0"]),
        "re-entered in `a`"
    );
}

/// Guard side effects (plan WI-9 Risks): a guard condition may write the blackboard, even when it
/// then fails; the write is visible to the state node ticked in the same tick; a guard after the
/// first succeeding one is not evaluated.
#[test]
fn guard_may_write_the_blackboard_and_the_state_sees_it() {
    let bench = Bench::new();
    let mut b = B::new();
    let read = b.read("r", "k");
    let s1 = b.act("s1", &["running"]);
    let guard = b.leaf_cond(
        "guard",
        "stub.cond.store",
        serde_json::json!({ "key": "k", "value": 7, "flag": "g" }),
    );
    let never = b.cond("later");
    let def = FsmDef::new(0, vec![state("a", read), state("b", s1)])
        .with_transition(when(0, 1, guard))
        .with_transition(when(0, 1, never));
    let root = b.fsm("machine", def);
    let mut run = bench.run(
        b.graph("test.graph.fsm_guard_write", root)
            .with_blackboard_key("k", cgf_core::BbType::Integer),
    );

    let failing = run.step();
    bench.controls.set_flag("g", true);
    let firing = run.step();

    assert_eq!(
        failing.trace,
        lines(&[
            "store:k=true",
            "cond:g",
            "cond:later",
            &common::read_line("r", Some(&cgf_core::BbValue::Integer(7))),
        ]),
        "the write survives the failing guard and the state reads it"
    );
    assert_eq!(firing.trace, lines(&["store:k=true", "cond:g", "tick:s1"]));
}
