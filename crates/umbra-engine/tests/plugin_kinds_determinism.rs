//! AT-22 (M7, gate 4 successor, plugin-kinds row): the combined example `plugin_kinds` with the five
//! example plugins (a motion model `hover`, a sensor `radar`, a condition `bearing`, an action
//! `dispatch`, a network adapter `net-jsonl`) and the built-in air and sea models is deterministic:
//! per-tick hashes AND per-tick intents, contacts and statuses (plugin state is not in the v2 hash)
//! agree across wall-time scripts, stepped chunkings, scaled and real-time modes, with and without a
//! pause, and across seeds that repeat. Requirements: REQ-K-4, REQ-K-3, REQ-P-4, REQ-P-5, A-2 (same
//! OS, ADR 0003), ADR 0020, ADR 0021 (R-M3), ADR 0050.
//!
//! Same shape as `plugin_determinism.rs`. Contract with the scenario (WI-21): `examples/scenarios/
//! plugin_kinds/scenario.toml`, 30 Hz, at least 900 ticks long. The plugin crates are
//! `example-hover`, `example-radar`, `example-bearing`, `example-dispatch` and `example-net-jsonl`
//! (ASM-636; WI-17..19 name them, this file follows). Written in M7 WI-1: ignored until M7 WI-21.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod behavior_support;
mod common;
mod m7_support;
mod plugin_support;

use core::time::Duration;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use behavior_support::{assert_same_behavior_trace, standard};
use common::{PAUSE_AT, Trace, options, run_to, run_traced, step_traced};
use m7_support::{example, plugin_lock};
use umbra_engine::{
    ClockMode, RunUntil, TimeScale, instantiate_with_plugins, load_scenario_with_plugins,
};

const TICKS: u64 = 900;
const PACKAGES: [&str; 5] = ["hover", "radar", "bearing", "dispatch", "net-jsonl"];

/// One build of the five example plugins per test process.
static PLUGINS: LazyLock<PathBuf> = LazyLock::new(|| {
    let libraries: Vec<(String, PathBuf)> = PACKAGES
        .iter()
        .map(|name| {
            (
                (*name).to_owned(),
                plugin_support::library(&format!("example-{name}"), &[]),
            )
        })
        .collect();
    let refs: Vec<(&str, &Path)> = libraries
        .iter()
        .map(|(n, l)| (n.as_str(), l.as_path()))
        .collect();
    plugin_support::plugins_dir(&refs)
});

fn scaled_4x() -> ClockMode {
    ClockMode::Scaled(TimeScale::new(4.0).expect("valid scale"))
}

/// A fresh load (fresh host) and engine, run for `TICKS`.
fn traced(read_cost: Duration, seed: Option<u64>, clock: Option<ClockMode>, pause: bool) -> Trace {
    let loaded = load_scenario_with_plugins(
        &standard(),
        &example("plugin_kinds"),
        std::slice::from_ref(&*PLUGINS),
    )
    .expect("load plugin_kinds");
    let run =
        instantiate_with_plugins(&loaded, options(read_cost, seed, clock)).expect("instantiate");
    run_to(run.engine, TICKS, pause)
}

fn stepped(chunks: &[u64], pause: bool) -> Trace {
    let loaded = load_scenario_with_plugins(
        &standard(),
        &example("plugin_kinds"),
        std::slice::from_ref(&*PLUGINS),
    )
    .expect("load plugin_kinds");
    let mut engine = instantiate_with_plugins(
        &loaded,
        options(Duration::from_micros(500), None, Some(ClockMode::Stepped)),
    )
    .expect("instantiate")
    .engine;
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

/// REQ-K-4: two as-fast-as-possible runs, each with its own load and host, agree on every tick
/// although the wall-time scripts differ; the entity count changes during the run (`dispatch` spawns),
/// so the comparison covers the spawner too.
#[test]
#[ignore = "M7 WI-21"]
fn given_plugin_kinds_when_run_twice_with_different_wall_scripts_then_hashes_and_intents_identical()
{
    let _guard = plugin_lock();
    let first = traced(Duration::from_micros(500), None, None, false);

    let second = traced(Duration::from_micros(1700), None, None, false);

    assert_same_behavior_trace("afap twice, other wall script", &second, &first);
    assert_eq!(first.len(), TICKS);
    let counts: std::collections::BTreeSet<_> =
        first.snapshots.iter().map(|s| s.entities.len()).collect();
    assert!(
        counts.len() > 1,
        "dispatch spawned entities during the run: {counts:?}"
    );
}

/// REQ-K-3, REQ-K-4, D-5: stepped (several chunkings), scaled and real-time runs, each with and
/// without a pause/resume at tick 100, give exactly the as-fast-as-possible hashes and intents.
#[test]
#[ignore = "M7 WI-21"]
fn given_plugin_kinds_when_run_in_each_clock_mode_then_matches_afap() {
    let _guard = plugin_lock();
    let want = traced(Duration::from_micros(500), None, None, false);
    let ones = vec![1_u64; 900];
    let mixed = [1, 7, 64, 28, 800];
    let hundred = [100, 800];

    for pause in [false, true] {
        let tag = if pause { "pause@100" } else { "no pause" };
        let chunkings: [(&str, &[u64]); 3] = [
            ("stepped x1", &ones),
            ("stepped 1,7,64,28,rest", &mixed),
            ("stepped 100,rest", &hundred),
        ];
        for (name, chunks) in chunkings {
            assert_same_behavior_trace(&format!("{name}, {tag}"), &stepped(chunks, pause), &want);
        }
        assert_same_behavior_trace(
            &format!("scaled 4x, {tag}"),
            &traced(Duration::from_micros(500), None, Some(scaled_4x()), pause),
            &want,
        );
        assert_same_behavior_trace(
            &format!("realtime, {tag}"),
            &traced(
                Duration::from_micros(500),
                None,
                Some(ClockMode::RealTime),
                pause,
            ),
            &want,
        );
    }
}

/// REQ-K-4, ADR 0020: the seed is an input that reaches all plugin kinds (they draw from the host
/// streams): different seeds give different final hashes, the same seed repeats exactly.
#[test]
#[ignore = "M7 WI-21"]
fn given_different_seeds_then_final_hashes_differ_and_a_repeat_is_identical() {
    let _guard = plugin_lock();
    let seeds = [1_u64, 2, 3];
    let runs: Vec<Trace> = seeds
        .iter()
        .map(|&s| traced(Duration::from_micros(500), Some(s), None, false))
        .collect();

    let finals: std::collections::BTreeSet<_> =
        runs.iter().map(|t| t.hashes.last().copied()).collect();
    assert_eq!(finals.len(), seeds.len());
    let again = traced(Duration::from_micros(500), Some(2), None, false);
    assert_same_behavior_trace("seed 2 twice", &again, &runs[1]);
}
