//! AT-2 (composites, leaves) and AT-3 (reactive and memory semantics, parallel): status tables and
//! evaluation order of `sequence`, `selector`, `parallel`, `condition` and `action` nodes.
//! Requirements: REQ-B-7, REQ-B-8, REQ-B-1, ADR 0013, ASM-070.
//!
//! Step 4 of M3: written before WI-8, un-ignored in WI-8. Stub leaves log `tick:<name>`, `halt:<name>` and
//! `cond:<name>`; assertions on halts count occurrences and do not depend on the order between a
//! halt and the line that caused it.

#![forbid(unsafe_code)]

mod common;

use cgf_core::{ParallelPolicy, Status};
use common::{B, Bench, lines, statuses, traces};

fn count(trace: &[String], line: &str) -> usize {
    trace.iter().filter(|l| l.as_str() == line).count()
}

/// The `tick:`/`cond:` lines only (evaluation order without halts).
fn evaluated(trace: &[String]) -> Vec<String> {
    trace
        .iter()
        .filter(|l| !l.starts_with("halt:"))
        .cloned()
        .collect()
}

/// One tick of `kind(a, b)` with leaf scripts `[a, b]`.
fn one_tick(kind: &str, scripts: [&str; 2]) -> (Status, Vec<String>) {
    let bench = Bench::new();
    let mut b = B::new();
    let a = b.act("a", &[scripts[0]]);
    let c = b.act("b", &[scripts[1]]);
    let root = match kind {
        "sequence" => b.seq("root", false, &[a, c]),
        "selector" => b.sel("root", false, &[a, c]),
        "parallel_all" => b.par("root", ParallelPolicy::All, &[a, c]),
        "parallel_any" => b.par("root", ParallelPolicy::Any, &[a, c]),
        other => panic!("unknown kind {other}"),
    };
    let mut run = bench.run(b.graph("test.graph.table", root));
    let step = run.step();
    (step.status, evaluated(&step.trace))
}

/// REQ-B-7: `sequence` returns the first Failure or Running, else Success; later children are
/// not ticked.
#[test]
fn sequence_status_table() {
    use Status::{Failure, Running, Success};
    let table: [([&str; 2], Status, &[&str]); 5] = [
        (["success", "success"], Success, &["tick:a", "tick:b"]),
        (["success", "failure"], Failure, &["tick:a", "tick:b"]),
        (["failure", "success"], Failure, &["tick:a"]),
        (["running", "success"], Running, &["tick:a"]),
        (["success", "running"], Running, &["tick:a", "tick:b"]),
    ];
    for (scripts, status, trace) in table {
        assert_eq!(
            one_tick("sequence", scripts),
            (status, lines(trace)),
            "{scripts:?}"
        );
    }
}

/// REQ-B-7: `selector` returns the first Success or Running, else Failure.
#[test]
fn selector_status_table() {
    use Status::{Failure, Running, Success};
    let table: [([&str; 2], Status, &[&str]); 5] = [
        (["failure", "failure"], Failure, &["tick:a", "tick:b"]),
        (["failure", "success"], Success, &["tick:a", "tick:b"]),
        (["success", "failure"], Success, &["tick:a"]),
        (["running", "failure"], Running, &["tick:a"]),
        (["failure", "running"], Running, &["tick:a", "tick:b"]),
    ];
    for (scripts, status, trace) in table {
        assert_eq!(
            one_tick("selector", scripts),
            (status, lines(trace)),
            "{scripts:?}"
        );
    }
}

/// REQ-B-7: `parallel` with policy `all`: Failure as soon as one fails, Success when all
/// succeeded, else Running; every unfinished child is ticked in order.
#[test]
fn parallel_all_status_table() {
    use Status::{Failure, Running, Success};
    let table: [([&str; 2], Status); 4] = [
        (["success", "success"], Success),
        (["success", "failure"], Failure),
        (["running", "success"], Running),
        (["running", "failure"], Failure),
    ];
    for (scripts, status) in table {
        let (got, trace) = one_tick("parallel_all", scripts);
        assert_eq!(got, status, "{scripts:?}");
        assert_eq!(trace, lines(&["tick:a", "tick:b"]), "{scripts:?}");
    }
}

/// REQ-B-7: `parallel` with policy `any`: Success as soon as one succeeds, Failure when all
/// failed, else Running.
#[test]
fn parallel_any_status_table() {
    use Status::{Failure, Running, Success};
    let table: [([&str; 2], Status); 3] = [
        (["failure", "failure"], Failure),
        (["running", "success"], Success),
        (["running", "failure"], Running),
    ];
    for (scripts, status) in table {
        let (got, trace) = one_tick("parallel_any", scripts);
        assert_eq!(got, status, "{scripts:?}");
        assert_eq!(trace, lines(&["tick:a", "tick:b"]), "{scripts:?}");
    }
}

/// REQ-B-7, REQ-B-8: a condition is Success iff its guard is true and is never Running; an action
/// returns what its script says.
#[test]
fn condition_and_action_status_table() {
    let bench = Bench::new();
    let mut b = B::new();
    let root = b.cond("flag");
    let mut cond = bench.run(b.graph("test.graph.cond", root));
    bench.controls.set_flag("flag", true);
    let yes = cond.step();
    bench.controls.set_flag("flag", false);
    let no = cond.step();
    assert_eq!(
        (yes.status, yes.trace),
        (Status::Success, lines(&["cond:flag"]))
    );
    assert_eq!(
        (no.status, no.trace),
        (Status::Failure, lines(&["cond:flag"]))
    );

    for (word, status) in [
        ("success", Status::Success),
        ("failure", Status::Failure),
        ("running", Status::Running),
    ] {
        let mut run = bench.run(common::script_graph("test.graph.act", "act", &[word]));
        let step = run.step();
        assert_eq!(
            (step.status, step.trace),
            (status, lines(&["tick:act"])),
            "{word}"
        );
    }
}

/// ADR 0013: a reactive `selector` re-checks its first child every tick; when it starts to succeed
/// the lower-priority child that was Running is halted (pre-emption, A-001 example).
#[test]
fn reactive_selector_preempts_running_lower_priority_child() {
    let bench = Bench::new();
    let mut b = B::new();
    let hi = b.cond("hi");
    let lo = b.act("lo", &["running"]);
    let root = b.sel("root", false, &[hi, lo]);
    let mut run = bench.run(b.graph("test.graph.preempt", root));

    let first = run.step();
    bench.controls.set_flag("hi", true);
    let second = run.step();
    let third = run.step();

    assert_eq!(first.status, Status::Running);
    assert_eq!(evaluated(&first.trace), lines(&["cond:hi", "tick:lo"]));
    assert_eq!(second.status, Status::Success);
    assert_eq!(evaluated(&second.trace), lines(&["cond:hi"]));
    assert_eq!(count(&second.trace, "halt:lo"), 1);
    assert_eq!(count(&third.trace, "halt:lo"), 0, "halted once, not again");
}

/// ADR 0013: a reactive `sequence` starts at its first child every tick.
#[test]
fn reactive_sequence_rechecks_earlier_children_every_tick() {
    let bench = Bench::new();
    let mut b = B::new();
    let guard = b.cond("guard");
    let work = b.act("work", &["running"]);
    let root = b.seq("root", false, &[guard, work]);
    let mut run = bench.run(b.graph("test.graph.reactive_seq", root));
    bench.controls.set_flag("guard", true);

    let steps = run.steps(3);

    for step in &steps {
        assert_eq!(evaluated(&step.trace), lines(&["cond:guard", "tick:work"]));
    }
    bench.controls.set_flag("guard", false);
    let lost = run.step();
    assert_eq!(lost.status, Status::Failure);
    assert_eq!(count(&lost.trace, "halt:work"), 1);
}

/// ADR 0013: a `sequence` with `memory = true` resumes at its Running child, and restarts at the
/// first child after it finished.
#[test]
fn memory_sequence_resumes_at_running_child() {
    let bench = Bench::new();
    let mut b = B::new();
    let a = b.act("a", &["success"]);
    let w = b.act("b", &["running", "running", "success"]);
    let c = b.act("c", &["success"]);
    let root = b.seq("root", true, &[a, w, c]);
    let mut run = bench.run(b.graph("test.graph.memory_seq", root));

    let steps = run.steps(4);

    assert_eq!(
        statuses(&steps),
        [
            Status::Running,
            Status::Running,
            Status::Success,
            Status::Success
        ]
    );
    assert_eq!(
        traces(&steps),
        [
            lines(&["tick:a", "tick:b"]),
            lines(&["tick:b"]),
            lines(&["tick:b", "tick:c"]),
            lines(&["tick:a", "tick:b", "tick:c"]),
        ]
    );
}

/// ADR 0013: a `selector` with `memory = true` resumes at its Running child too.
#[test]
fn memory_selector_resumes_at_running_child() {
    let bench = Bench::new();
    let mut b = B::new();
    let a = b.act("a", &["failure"]);
    let w = b.act("b", &["running", "running", "success"]);
    let root = b.sel("root", true, &[a, w]);
    let mut run = bench.run(b.graph("test.graph.memory_sel", root));

    let steps = run.steps(3);

    assert_eq!(
        statuses(&steps),
        [Status::Running, Status::Running, Status::Success]
    );
    assert_eq!(
        traces(&steps),
        [
            lines(&["tick:a", "tick:b"]),
            lines(&["tick:b"]),
            lines(&["tick:b"]),
        ]
    );
}

/// ADR 0013: `all` fails fast. The failing child ends the parallel with Failure; the child that
/// was still Running is halted; children after the failing one are not ticked.
#[test]
fn parallel_all_fails_fast_and_halts_others() {
    let bench = Bench::new();
    let mut b = B::new();
    let x = b.act("x", &["running"]);
    let y = b.act("y", &["running", "failure"]);
    let z = b.act("z", &["running"]);
    let root = b.par("root", ParallelPolicy::All, &[x, y, z]);
    let mut run = bench.run(b.graph("test.graph.par_all", root));

    let first = run.step();
    let second = run.step();

    assert_eq!(first.status, Status::Running);
    assert_eq!(
        evaluated(&first.trace),
        lines(&["tick:x", "tick:y", "tick:z"])
    );
    assert_eq!(second.status, Status::Failure);
    assert_eq!(count(&second.trace, "halt:x"), 1);
    assert_eq!(count(&second.trace, "halt:z"), 1);
    assert_eq!(
        count(&second.trace, "tick:z"),
        0,
        "z is after the failing child"
    );
}

/// ADR 0013: `any` succeeds fast and halts the children that are still Running.
#[test]
fn parallel_any_succeeds_fast() {
    let bench = Bench::new();
    let mut b = B::new();
    let x = b.act("x", &["running"]);
    let y = b.act("y", &["running", "success"]);
    let root = b.par("root", ParallelPolicy::Any, &[x, y]);
    let mut run = bench.run(b.graph("test.graph.par_any", root));

    let first = run.step();
    let second = run.step();

    assert_eq!(first.status, Status::Running);
    assert_eq!(second.status, Status::Success);
    assert_eq!(count(&second.trace, "halt:x"), 1);
}

/// ADR 0013: finished results are kept until the parallel finishes: a child that succeeded is not
/// ticked again while a sibling is still Running.
#[test]
fn parallel_keeps_finished_children_until_it_finishes() {
    let bench = Bench::new();
    let mut b = B::new();
    let p = b.act("p", &["success"]);
    let q = b.act("q", &["running", "success"]);
    let root = b.par("root", ParallelPolicy::All, &[p, q]);
    let mut run = bench.run(b.graph("test.graph.par_keep", root));

    let steps = run.steps(2);

    assert_eq!(statuses(&steps), [Status::Running, Status::Success]);
    assert_eq!(
        traces(&steps),
        [lines(&["tick:p", "tick:q"]), lines(&["tick:q"])]
    );
}

/// REQ-B-8: leaves are visited depth first, children in array order. With every leaf succeeding,
/// `sequence [ sequence [a, b], parallel [c, d], e ]` ticks a, b, c, d, e.
#[test]
fn evaluation_trace_is_depth_first_in_array_order() {
    let bench = Bench::new();
    let mut b = B::new();
    let na = b.act("a", &["success"]);
    let nb = b.act("b", &["success"]);
    let inner = b.seq("inner", false, &[na, nb]);
    let nc = b.act("c", &["success"]);
    let nd = b.act("d", &["success"]);
    let par = b.par("par", ParallelPolicy::All, &[nc, nd]);
    let e = b.act("e", &["success"]);
    let root = b.seq("root", false, &[inner, par, e]);
    let mut run = bench.run(b.graph("test.graph.order", root));

    let step = run.step();

    assert_eq!(step.status, Status::Success);
    assert_eq!(
        step.trace,
        lines(&["tick:a", "tick:b", "tick:c", "tick:d", "tick:e"])
    );
}

/// REQ-B-1: leaves emit intents in evaluation order into the buffer they are given.
#[test]
fn intents_are_appended_in_evaluation_order() {
    let bench = Bench::new();
    let mut b = B::new();
    let na = b.act("a", &["success"]);
    let nc = b.act("c", &["success"]);
    let nd = b.act("d", &["success"]);
    let root = b.seq("root", false, &[na, nc, nd]);
    let mut run = bench.run(b.graph("test.graph.intents", root));

    let step = run.step();

    let names: Vec<String> = step
        .intents
        .iter()
        .map(|i| match i {
            cgf_core::Intent::Custom { payload, .. } => {
                payload.as_str().unwrap_or_default().to_string()
            }
            other => panic!("unexpected intent {other:?}"),
        })
        .collect();
    assert_eq!(names, lines(&["a", "c", "d"]));
}
