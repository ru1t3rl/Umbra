//! AT-3 (halt rules): which nodes are halted, when and how often.
//! Requirements: REQ-B-8, ADR 0013, ASM-070.
//!
//! Step 4 of M3: written before WI-8; un-ignored in WI-8. The stub action
//! counts its `halt()` calls in the log (`halt:<name>`), so a spurious halt of a node that is not
//! Running is visible.

#![forbid(unsafe_code)]

mod common;

use cgf_core::Status;
use common::{B, Bench, lines};

fn count(trace: &[String], line: &str) -> usize {
    trace.iter().filter(|l| l.as_str() == line).count()
}

/// ADR 0013: a child that was Running last tick and is skipped because an earlier child now
/// decides the result is halted exactly once.
#[test]
fn skipped_running_child_is_halted_once() {
    let bench = Bench::new();
    let mut b = B::new();
    let guard = b.cond("guard");
    let work = b.act("work", &["running"]);
    let root = b.seq("root", false, &[guard, work]);
    let mut run = bench.run(b.graph("test.graph.halt_once", root));
    bench.controls.set_flag("guard", true);
    let started = run.step();

    bench.controls.set_flag("guard", false);
    let skipped = run.step();
    let after = run.step();

    assert_eq!(started.status, Status::Running);
    assert_eq!(skipped.status, Status::Failure);
    assert_eq!(count(&skipped.trace, "halt:work"), 1);
    assert_eq!(count(&after.trace, "halt:work"), 0);
    assert_eq!(count(&after.trace, "tick:work"), 0);
}

/// ADR 0013: nodes that finish on their own are not halted.
#[test]
fn finished_children_are_never_halted() {
    let bench = Bench::new();
    let mut b = B::new();
    let a = b.act("a", &["success"]);
    let c = b.act("c", &["failure"]);
    let root = b.sel("root", false, &[c, a]);
    let mut run = bench.run(b.graph("test.graph.no_halt", root));

    let steps = run.steps(3);

    for step in &steps {
        assert_eq!(
            count(&step.trace, "halt:a") + count(&step.trace, "halt:c"),
            0
        );
    }
}

/// ADR 0013: halting the instance halts the Running nodes below it once, and halting again does
/// nothing.
#[test]
fn halting_the_instance_halts_running_nodes_once() {
    let bench = Bench::new();
    let mut b = B::new();
    let a = b.act("a", &["success"]);
    let w = b.act("w", &["running"]);
    let root = b.seq("root", true, &[a, w]);
    let mut run = bench.run(b.graph("test.graph.halt_all", root));
    run.steps(2);

    let first = run.halt();
    let second = run.halt();

    assert_eq!(first, lines(&["halt:w"]));
    assert_eq!(second, Vec::<String>::new());
}

/// ADR 0013: halting before the first tick does nothing.
#[test]
fn halting_a_fresh_instance_does_nothing() {
    let bench = Bench::new();
    let mut run = bench.run(common::script_graph("test.graph.fresh", "a", &["running"]));

    assert_eq!(run.halt(), Vec::<String>::new());
}

/// ADR 0013: after a halt a `memory` sequence starts again at its first child.
#[test]
fn after_halt_a_memory_sequence_restarts_at_its_first_child() {
    let bench = Bench::new();
    let mut b = B::new();
    let a = b.act("a", &["success"]);
    let w = b.act("w", &["running"]);
    let root = b.seq("root", true, &[a, w]);
    let mut run = bench.run(b.graph("test.graph.restart", root));
    run.steps(2);
    run.halt();

    let step = run.step();

    assert_eq!(step.trace, lines(&["tick:a", "tick:w"]));
}

// --- ADR 0013, amended 2026-10-18: a parent halt reaches Running nodes only --------------------

/// `n = 5` ticks at 30 Hz.
const FIVE_TICKS_S: f64 = 5.0 / 30.0;

fn fire_ticks(steps: &[common::Step]) -> Vec<usize> {
    steps
        .iter()
        .enumerate()
        .filter(|(_, s)| s.trace.iter().any(|l| l == "tick:fire"))
        .map(|(i, _)| i)
        .collect()
}

/// `sequence [enemy_near, selector [cooldown(fire), aim]]`.
fn pre_empted_cooldown(bench: &Bench) -> common::Runner {
    let mut b = B::new();
    let near = b.cond("enemy_near");
    let fire = b.act("fire", &["success"]);
    let cool = b.cooldown("cool", fire, FIVE_TICKS_S);
    let aim = b.act("aim", &["running"]);
    let pick = b.sel("pick", false, &[cool, aim]);
    let root = b.seq("root", false, &[near, pick]);
    bench.run(b.graph("test.graph.amend_1", root))
}

/// Required case 1: a cooling cooldown under a pre-empted reactive sequence keeps its timer.
#[test]
fn cooling_cooldown_keeps_its_timer_when_an_ancestor_is_pre_empted() {
    let bench = Bench::new();
    let mut run = pre_empted_cooldown(&bench);
    let mut steps = Vec::new();
    for near in [true, true, false, true, true, true] {
        bench.controls.set_flag("enemy_near", near);
        steps.push(run.step());
    }

    assert_eq!(fire_ticks(&steps), [0, 5], "no fire in 1..=4");
    assert_eq!(count(&steps[2].trace, "halt:aim"), 1, "aim is halted");
    assert_eq!(steps[1].status, Status::Running);
}

/// Required case 2: a cooldown whose child is Running resets when halted; no timer starts.
#[test]
fn cooldown_with_a_running_child_resets_when_halted() {
    let bench = Bench::new();
    let mut b = B::new();
    let guard = b.cond("guard");
    let work = b.act("work", &["running"]);
    let cool = b.cooldown("cool", work, FIVE_TICKS_S);
    let root = b.seq("root", false, &[guard, cool]);
    let mut run = bench.run(b.graph("test.graph.amend_2", root));
    bench.controls.set_flag("guard", true);
    run.step();
    bench.controls.set_flag("guard", false);
    let halted = run.step();
    bench.controls.set_flag("guard", true);
    let again = run.step();

    assert_eq!(count(&halted.trace, "halt:work"), 1);
    assert!(again.trace.contains(&"tick:work".to_string()));
}

/// Required case 3: the instance halt clears every timer.
#[test]
fn instance_halt_clears_cooldown_timers() {
    let bench = Bench::new();
    let mut run = pre_empted_cooldown(&bench);
    bench.controls.set_flag("enemy_near", true);
    let first = run.step();
    run.halt();
    let second = run.step();

    assert_eq!(fire_ticks(&[first, second]), [0, 1]);
}

/// Required case 4: finished children keep their state when the Running node above is halted:
/// a cooldown that finished as a slot of a Running `parallel`, and inside a Running `repeat`.
#[test]
fn finished_children_keep_their_timers_when_the_node_above_is_halted() {
    let bench = Bench::new();
    let mut b = B::new();
    let guard = b.cond("guard");
    let fire = b.act("fire", &["success"]);
    let cool = b.cooldown("cool", fire, FIVE_TICKS_S);
    let aim = b.act("aim", &["running"]);
    let par = b.par("par", cgf_core::ParallelPolicy::All, &[cool, aim]);
    let root = b.seq("root", false, &[guard, par]);
    let mut run = bench.run(b.graph("test.graph.amend_4a", root));
    let mut steps = Vec::new();
    for guard in [true, false, true, true, true, true] {
        bench.controls.set_flag("guard", guard);
        steps.push(run.step());
    }
    assert_eq!(fire_ticks(&steps), [0, 5], "parallel slot");

    let bench = Bench::new();
    let mut b = B::new();
    let guard = b.cond("guard");
    let fire = b.act("fire", &["success"]);
    let cool = b.cooldown("cool", fire, FIVE_TICKS_S);
    let rep = b.repeat("rep", cool, None);
    let root = b.seq("root", false, &[guard, rep]);
    let mut run = bench.run(b.graph("test.graph.amend_4b", root));
    let mut steps = Vec::new();
    for guard in [true, false, true, true, true, true] {
        bench.controls.set_flag("guard", guard);
        steps.push(run.step());
    }
    assert_eq!(fire_ticks(&steps), [0, 5], "repeat child");
}
