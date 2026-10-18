//! AT-8 (gate 4 successor, ported from M1 AT-1): the determinism gate on the example scenario
//! `examples/scenarios/walking_skeleton` (three templates, seed 42, 30 Hz, run for 900 ticks).
//! Requirements: M2 acceptance 2, REQ-K-3, REQ-K-4, D-5, A-2 (same OS, ADR 0003), ASM-061.
//!
//! The scenario is loaded from its files through the built-in registry and instantiated; then the
//! state hash after every tick is compared across runs: twice in as-fast-as-possible mode with
//! different wall-time scripts, in stepped chunks, scaled and real-time modes on a manual wall
//! clock (no sleeping), with a pause/resume at tick 100, and across a mid-run mode switch. On the
//! first divergent tick the failure names the entity and the field.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod common;

use core::time::Duration;

use common::{
    PAUSE_AT, TICKS, Trace, assert_same_trace, example_engine, instantiate_with, load_example,
    run_to, run_traced, step_traced,
};
use umbra_engine::{ClockMode, RunUntil, TimeScale};
use umbra_kernel::StdWallClock;

fn scaled_4x() -> ClockMode {
    ClockMode::Scaled(TimeScale::new(4.0).expect("valid scale"))
}

fn baseline() -> Trace {
    run_to(example_engine(None, None), TICKS, false)
}

/// Runs the example in stepped mode in the given chunks (summing to `TICKS`), optionally pausing
/// and resuming once the first `PAUSE_AT` ticks are done.
fn run_stepped(chunks: &[u64], pause: bool) -> Trace {
    let mut engine = example_engine(None, Some(ClockMode::Stepped));
    let mut trace = Trace::default();
    for &n in chunks {
        step_traced(&mut engine, n, &mut trace);
        if pause && trace.len() == PAUSE_AT {
            engine.pause();
            run_traced(&mut engine, RunUntil::Idle, &mut trace);
            assert_eq!(
                trace.len(),
                PAUSE_AT,
                "no stepped tick may run while paused"
            );
            engine.resume();
        }
    }
    assert_eq!(trace.len(), TICKS);
    trace
}

/// M2 acceptance 2, REQ-K-4: two as-fast-as-possible runs hash alike on every tick, although the
/// wall-time scripts differ (wall time leaking into the simulation would make them differ).
#[test]
fn given_example_scenario_when_run_twice_with_different_wall_scripts_then_per_tick_hashes_are_identical()
 {
    let scenario = load_example();
    let first = run_to(
        instantiate_with(&scenario, Duration::from_micros(500), None, None).engine,
        TICKS,
        false,
    );

    let second = run_to(
        instantiate_with(&scenario, Duration::from_micros(1700), None, None).engine,
        TICKS,
        false,
    );

    assert_same_trace("afap twice, other wall script", &second, &first);
    assert_eq!(first.len(), TICKS);
}

/// REQ-K-4: the production wall clock (standard) gives the same hashes as the manual one.
#[test]
fn given_example_when_run_on_std_wall_clock_then_hashes_match_manual_wall_clock() {
    let scenario = load_example();
    let options = umbra_engine::scenario::RunOptions {
        wall: Box::new(StdWallClock::new()),
        seed: None,
        clock: None,
    };
    let engine = umbra_engine::scenario::instantiate(&common::registry(), &scenario, options)
        .expect("instantiate")
        .engine;

    let got = run_to(engine, TICKS, false);

    assert_same_trace("std wall clock", &got, &baseline());
}

/// REQ-K-3, REQ-K-4, D-5: stepped (several chunkings), scaled and real-time runs, each with and
/// without a pause/resume at tick 100, produce exactly the as-fast-as-possible hashes.
#[test]
fn given_example_when_run_in_each_clock_mode_then_per_tick_hashes_match_afap() {
    let want = baseline();

    for pause in [false, true] {
        let tag = if pause { "pause@100" } else { "no pause" };
        let chunkings: [(&str, &[u64]); 4] = [
            ("stepped x1", &[1; 900]),
            ("stepped 1,7,64,rest", &[1, 7, 64, 28, 800]),
            ("stepped 100,800", &[100, 800]),
            (
                "stepped 7s",
                &[7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 9, 800],
            ),
        ];
        for (name, chunks) in chunkings {
            assert_same_trace(
                &format!("{name}, {tag}"),
                &run_stepped(chunks, pause),
                &want,
            );
        }
        assert_same_trace(
            &format!("scaled 4x, {tag}"),
            &run_to(example_engine(None, Some(scaled_4x())), TICKS, pause),
            &want,
        );
        assert_same_trace(
            &format!("realtime, {tag}"),
            &run_to(
                example_engine(None, Some(ClockMode::RealTime)),
                TICKS,
                pause,
            ),
            &want,
        );
    }
}

/// REQ-K-3, REQ-K-4: switching clock mode mid-run (as fast as possible, then real time, then
/// stepped) does not change a single hash.
#[test]
fn given_mode_switch_mid_run_then_hashes_match_afap() {
    let mut engine = example_engine(None, None);
    let mut got = Trace::default();

    run_traced(&mut engine, RunUntil::Ticks(300), &mut got);
    engine.set_clock(ClockMode::RealTime);
    run_traced(&mut engine, RunUntil::Ticks(600), &mut got);
    assert_eq!(
        got.len(),
        600,
        "the real-time leg is bounded by total ticks"
    );
    engine.set_clock(ClockMode::Stepped);
    step_traced(&mut engine, 300, &mut got);

    assert_same_trace("afap -> realtime -> stepped", &got, &baseline());
}

/// REQ-K-4, ASM-061: the seed is an input. The example has no seed-dependent motion (cruise
/// speeds are fixed in the file), so only the hashed per-entity RNG state differs: the final hashes
/// differ while every entity ends at the same place.
#[test]
fn given_different_seeds_then_hashes_differ() {
    let a = run_to(example_engine(Some(1), None), TICKS, false);
    let b = run_to(example_engine(Some(2), None), TICKS, false);

    assert_ne!(a.hashes.last(), b.hashes.last());
    let final_positions = |t: &Trace| -> Vec<_> {
        t.snapshots.last().map_or_else(Vec::new, |s| {
            s.entities.iter().map(|e| e.state.position).collect()
        })
    };
    assert_eq!(
        final_positions(&a),
        final_positions(&b),
        "seeds do not change motion in the example (ASM-061)"
    );
}

/// REQ-K-4: overriding the seed with the file's own seed changes nothing.
#[test]
fn given_seed_override_equal_to_the_file_seed_then_hashes_are_identical() {
    let overridden = run_to(example_engine(Some(42), None), TICKS, false);

    assert_same_trace("seed 42 explicit", &overridden, &baseline());
}

/// REQ-K-4 (loader determinism): loading the same files twice gives equal scenarios and engines
/// with equal initial hashes.
#[test]
fn given_same_files_loaded_twice_then_scenarios_and_initial_hashes_equal() {
    let (a, b) = (load_example(), load_example());

    let (run_a, run_b) = (
        instantiate_with(&a, Duration::from_micros(500), None, None),
        instantiate_with(&b, Duration::from_micros(500), None, None),
    );

    assert_eq!(a, b);
    assert_eq!(run_a.engine.state_hash(), run_b.engine.state_hash());
}

/// REQ-K-4: one hash per tick, and the per-tick hashes of a moving scenario are all distinct.
#[test]
fn given_same_seed_then_hash_sequence_length_equals_ticks_and_every_tick_changes_the_hash() {
    let trace = baseline();

    assert_eq!(trace.len(), TICKS);
    let distinct: std::collections::BTreeSet<_> = trace.hashes.iter().collect();
    assert_eq!(distinct.len(), trace.hashes.len());
}

/// REQ-K-3: `RunUntil::Ticks` counts total ticks since start, so a second call with the same
/// bound runs nothing.
#[test]
fn given_run_until_reached_when_run_again_then_no_more_ticks_run() {
    let mut engine = example_engine(None, None);
    let mut trace = Trace::default();

    run_traced(&mut engine, RunUntil::Ticks(50), &mut trace);
    run_traced(&mut engine, RunUntil::Ticks(50), &mut trace);

    assert_eq!(trace.len(), 50);
    assert_eq!(engine.now().tick(), 50);
}

/// REQ-K-3: `RunUntil::Idle` stops when a stepped clock has no pending steps.
#[test]
fn given_stepped_clock_without_requests_when_run_until_idle_then_returns_immediately() {
    let mut engine = example_engine(None, Some(ClockMode::Stepped));
    let mut trace = Trace::default();

    run_traced(&mut engine, RunUntil::Idle, &mut trace);

    assert_eq!(trace.len(), 0);
}

/// The scenario's own clock (`afap` in the file) is what runs when no override is given.
#[test]
fn given_no_clock_override_then_the_scenario_clock_runs_the_engine() {
    let mut engine = example_engine(None, None);
    let mut trace = Trace::default();

    run_traced(&mut engine, RunUntil::Ticks(10), &mut trace);

    assert_eq!(trace.len(), 10, "afap runs without stepping requests");
}
