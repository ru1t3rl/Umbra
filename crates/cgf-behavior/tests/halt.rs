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
