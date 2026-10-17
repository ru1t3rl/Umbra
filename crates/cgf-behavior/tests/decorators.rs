//! AT-2 (decorators) and AT-3 (`repeat`, `cooldown`, `timeout` timing): status tables of
//! `inverter`, `succeed_always`, `repeat`, `cooldown` and `timeout`.
//! Requirements: REQ-B-7, REQ-B-8, ADR 0013, ASM-070, ASM-078.
//!
//! Step 4 of M3: written before WI-8; every test is ignored with that work item.
//!
//! Choices the plan leaves open (recorded in the plan's running log): a `cooldown` of `n` ticks
//! that finished at tick `t` ticks its child again at tick `t + n`; a `timeout` of `n` ticks whose
//! child started at tick `s` ticks the child at `s .. s + n - 1` and halts it with Failure at
//! tick `s + n`. The plan's AT-3 table lists the `repeat`/`cooldown`/`timeout` tests in
//! `composites.rs`; they live here with the other decorators.

#![forbid(unsafe_code)]

mod common;

use cgf_core::{InstantiateError, Status};
use common::{B, Bench, Step, lines, rate_hz, statuses, traces};
use serde_json::json;

fn one_child(kind: &str, child_script: &str) -> Step {
    let bench = Bench::new();
    let mut b = B::new();
    let c = b.act("c", &[child_script]);
    let root = match kind {
        "inverter" => b.inverter("root", c),
        "succeed_always" => b.succeed_always("root", c),
        other => panic!("unknown kind {other}"),
    };
    bench.run(b.graph("test.graph.decorator", root)).step()
}

/// REQ-B-7: `inverter` swaps Success and Failure and passes Running.
#[test]
#[ignore = "WI-8"]
fn inverter_status_table() {
    let table = [
        ("success", Status::Failure),
        ("failure", Status::Success),
        ("running", Status::Running),
    ];
    for (child, status) in table {
        assert_eq!(one_child("inverter", child).status, status, "{child}");
    }
}

/// REQ-B-7: `succeed_always` maps Failure to Success and passes Success and Running.
#[test]
#[ignore = "WI-8"]
fn succeed_always_status_table() {
    let table = [
        ("success", Status::Success),
        ("failure", Status::Success),
        ("running", Status::Running),
    ];
    for (child, status) in table {
        assert_eq!(one_child("succeed_always", child).status, status, "{child}");
    }
}

fn repeat_run(count: Option<u32>, child_script: &[&str], ticks: usize) -> Vec<Step> {
    let bench = Bench::new();
    let mut b = B::new();
    let c = b.act("c", child_script);
    let root = b.repeat("root", c, count);
    bench.run(b.graph("test.graph.repeat", root)).steps(ticks)
}

/// REQ-B-7, ADR 0013: with `count = 3` and a child that always succeeds, `repeat` is Running after
/// the first and second success, Success after the third, and then starts over.
#[test]
#[ignore = "WI-8"]
fn repeat_with_count_succeeds_after_that_many_child_successes() {
    let steps = repeat_run(Some(3), &["success"], 6);

    assert_eq!(
        statuses(&steps),
        [
            Status::Running,
            Status::Running,
            Status::Success,
            Status::Running,
            Status::Running,
            Status::Success,
        ]
    );
}

/// REQ-B-7: `count = 1` is Success on the first child success.
#[test]
#[ignore = "WI-8"]
fn repeat_with_count_one_succeeds_on_the_first_child_success() {
    let steps = repeat_run(Some(1), &["success"], 2);

    assert_eq!(statuses(&steps), [Status::Success, Status::Success]);
}

/// REQ-B-7: without `count` the node repeats forever: Running after every child success.
#[test]
#[ignore = "WI-8"]
fn repeat_without_count_never_finishes_while_the_child_succeeds() {
    let steps = repeat_run(None, &["success"], 8);

    assert_eq!(statuses(&steps), vec![Status::Running; 8]);
}

/// ADR 0013: a failing child fails the `repeat` at once; a Running child keeps it Running and does
/// not count.
#[test]
#[ignore = "WI-8"]
fn repeat_fails_with_the_child_and_stays_running_with_it() {
    let failing = repeat_run(Some(3), &["success", "failure"], 3);
    let running = repeat_run(Some(2), &["running", "running", "success", "success"], 4);

    assert_eq!(
        statuses(&failing),
        [Status::Running, Status::Failure, Status::Failure]
    );
    assert_eq!(
        statuses(&running),
        [
            Status::Running,
            Status::Running,
            Status::Running,
            Status::Success
        ]
    );
}

/// ADR 0013, AT-3: `repeat` ticks its child at most once per tick, however often the child
/// succeeds.
#[test]
#[ignore = "WI-8"]
fn repeat_ticks_child_at_most_once_per_tick() {
    let steps = repeat_run(None, &["success"], 10);

    for (i, step) in steps.iter().enumerate() {
        assert_eq!(step.trace, lines(&["tick:c"]), "tick {i}");
    }
}

/// The ticks (0-based) at which the child `c` was ticked.
fn child_ticks(steps: &[Step]) -> Vec<usize> {
    steps
        .iter()
        .enumerate()
        .filter(|(_, s)| s.trace.iter().any(|l| l == "tick:c"))
        .map(|(i, _)| i)
        .collect()
}

fn cooldown_run(hz: u32, seconds: f64, ticks: usize) -> Vec<Step> {
    let bench = Bench::new();
    let mut b = B::new();
    let c = b.act("c", &["success"]);
    let root = b.cooldown("root", c, seconds);
    let behavior = bench.behavior_of(b.graph("test.graph.cooldown", root), &json!({}));
    bench.runner_at(&behavior, rate_hz(hz), 1).steps(ticks)
}

fn timeout_run(hz: u32, seconds: f64, ticks: usize) -> Vec<Step> {
    let bench = Bench::new();
    let mut b = B::new();
    let c = b.act("c", &["running"]);
    let root = b.timeout("root", c, seconds);
    let behavior = bench.behavior_of(b.graph("test.graph.timeout", root), &json!({}));
    bench.runner_at(&behavior, rate_hz(hz), 1).steps(ticks)
}

/// ASM-078, ADR 0013: durations are whole ticks fixed at instantiate. A 0.5 s `cooldown` is 15
/// ticks at 30 Hz and 30 ticks at 60 Hz: the child runs at tick 0, is not ticked and the node
/// fails in between, and runs again exactly `n` ticks after it finished.
#[test]
#[ignore = "WI-8"]
fn cooldown_and_timeout_count_whole_ticks_at_30_and_60_hz() {
    let cool30 = cooldown_run(30, 0.5, 31);
    let cool60 = cooldown_run(60, 0.5, 61);
    let time30 = timeout_run(30, 0.5, 17);
    let time60 = timeout_run(60, 0.5, 32);

    assert_eq!(child_ticks(&cool30), [0, 15, 30]);
    assert_eq!(child_ticks(&cool60), [0, 30, 60]);
    assert_eq!(cool30[0].status, Status::Success);
    assert_eq!(cool30[1].status, Status::Failure, "cooling down");
    // The child ran for ticks 0..=14 (15 ticks), then the timeout halts it with Failure.
    assert_eq!(statuses(&time30[..15]), vec![Status::Running; 15]);
    assert_eq!(time30[15].status, Status::Failure);
    assert_eq!(
        time30[15].trace.iter().filter(|l| *l == "halt:c").count(),
        1
    );
    assert_eq!(
        time30[16].status,
        Status::Running,
        "a fresh start after the timeout"
    );
    assert_eq!(statuses(&time60[..30]), vec![Status::Running; 30]);
    assert_eq!(time60[30].status, Status::Failure);
}

/// REQ-B-7: while cooling down the child is not ticked, so it cannot emit intents.
#[test]
#[ignore = "WI-8"]
fn cooldown_does_not_tick_its_child_while_cooling_down() {
    let steps = cooldown_run(30, 0.5, 15);

    assert_eq!(traces(&steps[1..]), vec![Vec::<String>::new(); 14]);
    assert_eq!(statuses(&steps[1..]), vec![Status::Failure; 14]);
}

/// ASM-078 / ADR 0013 amendment: a duration that does not fit in ticks at the rate makes
/// `instantiate` fail with `InstantiateError::Duration` (no panic, no saturation).
#[test]
#[ignore = "WI-8"]
fn cooldown_and_timeout_durations_that_do_not_fit_fail_instantiate() {
    let bench = Bench::new();
    for kind in ["cooldown", "timeout"] {
        let mut b = B::new();
        let c = b.act("c", &["success"]);
        let root = match kind {
            "cooldown" => b.cooldown("root", c, 1e30),
            _ => b.timeout("root", c, 1e30),
        };
        let behavior = bench.behavior_of(b.graph("test.graph.too_long", root), &json!({}));

        let err = behavior.instantiate(rate_hz(30)).err();

        assert!(
            matches!(err, Some(InstantiateError::Duration { hz: 30, .. })),
            "{kind}: {err:?}"
        );
    }
}
