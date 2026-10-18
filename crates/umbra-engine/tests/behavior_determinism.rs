//! AT-12 (gate 4, second target `behavior_determinism`): the behavior determinism gate on
//! `examples/scenarios/patrol_and_engage` (a tree, an embedded FSM, a file subgraph, a built-in
//! graph, the visual sensor and `chance`), seed 7, 30 Hz.
//! Requirements: REQ-B-16, REQ-K-3, REQ-K-4, D-5, A-2 (same OS, ADR 0003), ASM-061, ASM-081, ASM-087.
//!
//! Behavior node state is not part of the v2 state hash (ASM-081), so every comparison below is on
//! the state hash after every tick AND on the per-tick intents, contacts and status of every
//! entity (`assert_same_behavior_trace`); the first divergence names the tick, entity and field.
//!
//! Written in step 4 of M3; live since WI-16. Gate 4 runs `--test determinism --test
//! behavior_determinism` (ASM-087).

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod behavior_support;
mod common;

use core::time::Duration;
use std::collections::BTreeSet;
use std::sync::LazyLock;

use behavior_support::{
    TICKS, assert_same_behavior_trace, entity, example_dir, example_engine, fire_targets, id_of,
    instantiate_with, load_example, standard, template_and_graph_sources,
};
use common::{PAUSE_AT, Trace, run_to, run_traced, step_traced};
use umbra_engine::scenario::instantiate;
use umbra_engine::{ClockMode, RunUntil, TimeScale};
use umbra_templates::{Loader, SourceFile};

static BASELINE: LazyLock<Trace> =
    LazyLock::new(|| run_to(example_engine(None, None), TICKS, false));

fn baseline() -> &'static Trace {
    &BASELINE
}

fn scaled_4x() -> ClockMode {
    ClockMode::Scaled(TimeScale::new(4.0).expect("valid scale"))
}

/// Runs the example in stepped mode in the given chunks (summing to `TICKS`), optionally pausing
/// and resuming once `PAUSE_AT` ticks are done (the chunks have a boundary there).
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

/// REQ-K-4: two as-fast-as-possible runs agree on every tick, although the wall-time scripts
/// differ (wall time leaking into behavior or perception would make them differ).
#[test]
fn given_patrol_and_engage_when_run_twice_with_different_wall_scripts_then_per_tick_hashes_intents_and_contacts_are_identical()
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

    assert_same_behavior_trace("afap twice, other wall script", &second, &first);
    assert_eq!(first.len(), TICKS);
}

/// REQ-K-3, REQ-K-4, D-5: stepped (several chunkings), scaled and real-time runs, each with and
/// without a pause/resume at tick 100, give exactly the as-fast-as-possible hashes, intents and
/// contacts.
#[test]
fn given_patrol_and_engage_when_run_in_each_clock_mode_then_matches_afap() {
    let want = baseline();
    let ones = vec![1_u64; 1500];
    let mixed = [1, 7, 64, 28, 1400];
    let hundred = [100, 1400];
    let mut sevens = vec![7_u64; 14];
    sevens.extend([2, 1400]);

    for pause in [false, true] {
        let tag = if pause { "pause@100" } else { "no pause" };
        let chunkings: [(&str, &[u64]); 4] = [
            ("stepped x1", &ones),
            ("stepped 1,7,64,rest", &mixed),
            ("stepped 100,1400", &hundred),
            ("stepped 7s", &sevens),
        ];
        for (name, chunks) in chunkings {
            assert_same_behavior_trace(
                &format!("{name}, {tag}"),
                &run_stepped(chunks, pause),
                want,
            );
        }
        assert_same_behavior_trace(
            &format!("scaled 4x, {tag}"),
            &run_to(example_engine(None, Some(scaled_4x())), TICKS, pause),
            want,
        );
        assert_same_behavior_trace(
            &format!("realtime, {tag}"),
            &run_to(
                example_engine(None, Some(ClockMode::RealTime)),
                TICKS,
                pause,
            ),
            want,
        );
    }
}

/// REQ-K-3, REQ-K-4: switching the clock mode mid-run (as fast as possible, real time, stepped)
/// changes nothing.
#[test]
fn given_patrol_and_engage_with_clock_mode_switch_mid_run_then_matches_afap() {
    let mut engine = example_engine(None, None);
    let mut got = Trace::default();

    run_traced(&mut engine, RunUntil::Ticks(500), &mut got);
    engine.set_clock(ClockMode::RealTime);
    run_traced(&mut engine, RunUntil::Ticks(1000), &mut got);
    engine.set_clock(ClockMode::Stepped);
    step_traced(&mut engine, 500, &mut got);

    assert_same_behavior_trace("afap -> realtime -> stepped", &got, baseline());
}

/// The ticks at which blue-1 emitted `Fire` in a run of `ticks` with `seed`.
fn fire_ticks(seed: u64, ticks: u64) -> (Vec<usize>, Option<umbra_engine::StateHash>) {
    let run = instantiate_with(
        &load_example(),
        Duration::from_micros(500),
        Some(seed),
        None,
    );
    let blue = id_of(&run, "blue-1");
    let trace = run_to(run.engine, ticks, false);
    let at = trace
        .snapshots
        .iter()
        .enumerate()
        .filter(|(_, s)| !fire_targets(entity(s, blue)).is_empty())
        .map(|(i, _)| i + 1)
        .collect();
    (at, trace.hashes.last().copied())
}

/// REQ-B-16, REQ-K-4, ASM-061 revisited: the seed is an input that now matters: the engage state
/// rolls a `chance` before each shot from the entity's hashed RNG stream. Different seeds give
/// different final hashes (the RNG state is hashed), and among several seeds the tick sequence of
/// `Fire` is not the same for all of them.
#[test]
fn given_different_seeds_then_fire_sequence_and_hashes_differ() {
    let seeds = [1_u64, 2, 3, 4, 5, 6, 7, 8];

    let runs: Vec<_> = seeds.iter().map(|&s| fire_ticks(s, 1800)).collect();

    let hashes: BTreeSet<_> = runs.iter().map(|(_, h)| h).collect();
    let sequences: BTreeSet<_> = runs.iter().map(|(f, _)| f.clone()).collect();
    assert_eq!(
        hashes.len(),
        seeds.len(),
        "every seed has its own final hash"
    );
    assert!(
        runs.iter().all(|(f, _)| !f.is_empty()),
        "blue-1 fires in every run"
    );
    assert!(
        sequences.len() > 1,
        "the fire ticks depend on the seed: {sequences:?}"
    );
    assert_eq!(
        fire_ticks(3, 1800),
        runs[2],
        "and the same seed gives the same run"
    );
}

/// The example's template and graph files (in path order, then permuted by `order`, indices
/// modulo the file count) and its scenario file, as in-memory sources.
fn example_sources(order: &[usize]) -> (SourceFile, Vec<SourceFile>) {
    let all = template_and_graph_sources();
    let ordered = order.iter().map(|&i| all[i % all.len()].clone()).collect();
    let scenario = SourceFile::new(
        "scenario.toml",
        std::fs::read_to_string(example_dir().join("scenario.toml")).expect("scenario"),
    );
    (scenario, ordered)
}

fn file_count() -> usize {
    template_and_graph_sources().len()
}

fn run_from_sources(scenario: &SourceFile, files: &[SourceFile]) -> Trace {
    let registry = standard();
    let loaded = Loader::new(&registry)
        .expect("loader")
        .scenario_from_sources(scenario, files)
        .unwrap_or_else(|e| panic!("sources: {e}"));
    let run = instantiate(
        &registry,
        &loaded,
        common::options(Duration::from_micros(500), None, None),
    )
    .expect("instantiate");
    run_to(run.engine, TICKS, false)
}

/// REQ-K-4: the order in which graph and template files are given does not change the run.
#[test]
fn given_graph_files_in_shuffled_order_then_identical_runs() {
    let n = file_count();
    let seven: Vec<usize> = (0..n).collect();
    let (scenario, in_order) = example_sources(&seven);
    let reversed: Vec<usize> = (0..n).rev().collect();
    let (_, backwards) = example_sources(&reversed);
    let rotated: Vec<usize> = (0..n).map(|i| (i + 3) % n).collect();
    let (_, shuffled) = example_sources(&rotated);

    let want = run_from_sources(&scenario, &in_order);

    assert_same_behavior_trace(
        "reversed files",
        &run_from_sources(&scenario, &backwards),
        &want,
    );
    assert_same_behavior_trace(
        "rotated files",
        &run_from_sources(&scenario, &shuffled),
        &want,
    );
}

/// REQ-B-16: the `[editor]` table is not part of the graph: changing it changes no hash, intent or
/// contact on any tick.
#[test]
fn given_editor_tables_changed_then_per_tick_hashes_identical() {
    let n = file_count();
    let seven: Vec<usize> = (0..n).collect();
    let (scenario, files) = example_sources(&seven);
    let edited: Vec<SourceFile> = files
        .iter()
        .map(|f| {
            let text = f.text().replace(
                "positions = { modes = [0, 0]",
                "positions = { modes = [99, 7]",
            );
            SourceFile::new(f.path().to_path_buf(), text)
        })
        .collect();
    assert_ne!(
        files.iter().map(SourceFile::text).collect::<Vec<_>>(),
        edited.iter().map(SourceFile::text).collect::<Vec<_>>(),
        "the edit changed the attack graph's [editor] table"
    );

    let want = run_from_sources(&scenario, &files);
    let got = run_from_sources(&scenario, &edited);

    assert_same_behavior_trace("editor changed", &got, &want);
}

/// REQ-B-9, REQ-B-11, REQ-K-4: the nested subgraph and the FSM inside it (the example's `attack`
/// graph) run identically in two independent loads and engines, and they really ran: blue-1
/// emitted `Fire` at red-1.
#[test]
fn nested_subgraph_and_fsm_run_identically_twice() {
    let first_run = instantiate_with(&load_example(), Duration::from_micros(500), None, None);
    let blue = id_of(&first_run, "blue-1");
    let red = id_of(&first_run, "red-1");
    let first = run_to(first_run.engine, 1800, false);

    let second = run_to(example_engine(None, None), 1800, false);

    assert_same_behavior_trace("two independent loads", &second, &first);
    let fired = first
        .snapshots
        .iter()
        .any(|s| fire_targets(entity(s, blue)).contains(&red));
    assert!(
        fired,
        "the attack subgraph's FSM reached `engage` and fired at red-1"
    );
}
