//! AT-7 (gate 4, third target `plugin_determinism`): the plugin determinism gate on
//! `examples/scenarios/plugin_wander` with the real `example-wander` library, 30 Hz.
//! Requirements: REQ-K-4, REQ-K-3, A-2 (same OS, ADR 0003), ASM-115, ADR 0020, ADR 0021 (R-M3).
//!
//! Plugin state is not part of the state hash, so every comparison is on the hash after every tick AND
//! on the per-tick intents, contacts and status of every entity (`assert_same_behavior_trace`); the
//! first divergence names the tick, entity and field.
//!
//! Step 4 of M4: written before WI-12/WI-13. Contract with the example scenario (WI-13): it has the
//! entities `wander-direct` (`[behavior] ref = "example.behavior.wander"` with the default
//! `turn_every_s`, so a new heading every `ceil(10 * 30) = 300` ticks), `wander-graph` and
//! `patrol-builtin`. Every test is ignored with the work item that makes it pass: WI-13 for the
//! example, WI-12 for the fixture-plugin test (`second_engine_...`).

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod behavior_support;
mod common;
mod plugin_support;

use core::time::Duration;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use behavior_support::{assert_same_behavior_trace, entity, id_of, standard};
use common::{PAUSE_AT, Trace, options, run_to, run_traced, step_traced};
use umbra_core::Intent;
use umbra_engine::{
    ClockMode, HostOptions, LoadedScenario, RunUntil, TimeScale, instantiate_with_plugins,
    load_plugins, load_scenario_with_plugins,
};

/// 30 s at 30 Hz: three heading periods of the default wander.
const TICKS: u64 = 900;
const DIRECT: &str = "wander-direct";

/// One build of the example plugin per test process.
static WANDER: LazyLock<PathBuf> = LazyLock::new(plugin_support::wander);

fn load(plugins: &Path, scenario: &Path) -> LoadedScenario {
    load_scenario_with_plugins(&standard(), scenario, &[plugins.to_path_buf()])
        .unwrap_or_else(|e| panic!("load {}: {e}", scenario.display()))
}

fn example() -> PathBuf {
    plugin_support::wander_example_dir().join("scenario.toml")
}

/// A fresh load (fresh host) and engine, run for `TICKS`.
fn traced(read_cost: Duration, seed: Option<u64>, clock: Option<ClockMode>, pause: bool) -> Trace {
    let loaded = load(&WANDER, &example());
    let run = instantiate_with_plugins(&loaded, options(read_cost, seed, clock))
        .unwrap_or_else(|e| panic!("instantiate: {e}"));
    run_to(run.engine, TICKS, pause)
}

static BASELINE: LazyLock<Trace> =
    LazyLock::new(|| traced(Duration::from_micros(500), None, None, false));

fn scaled_4x() -> ClockMode {
    ClockMode::Scaled(TimeScale::new(4.0).expect("valid scale"))
}

/// The id of `wander-direct` (ids follow spawn order, so every run of the example agrees).
static DIRECT_ID: LazyLock<umbra_core::EntityId> = LazyLock::new(|| {
    let loaded = load(&WANDER, &example());
    let run = instantiate_with_plugins(&loaded, options(Duration::ZERO, None, None))
        .expect("instantiate for ids");
    id_of(&run, DIRECT)
});

/// The `SetHeading` intents of `wander-direct`, one text per tick, consecutive repeats dropped.
fn heading_changes(trace: &Trace) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for snapshot in &trace.snapshots {
        let text = format!(
            "{:?}",
            entity(snapshot, *DIRECT_ID)
                .intents
                .iter()
                .filter(|i| matches!(i, Intent::SetHeading(_)))
                .collect::<Vec<_>>()
        );
        if out.last() != Some(&text) {
            out.push(text);
        }
    }
    out
}

/// Runs the example in stepped mode in the given chunks, optionally pausing at `PAUSE_AT`.
fn stepped(chunks: &[u64], pause: bool) -> Trace {
    let loaded = load(&WANDER, &example());
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
/// although the wall-time scripts differ; and the plugin really ran (the heading changed).
#[test]
#[ignore = "WI-13"]
fn given_plugin_wander_when_run_twice_with_different_wall_scripts_then_per_tick_hashes_and_intents_identical()
 {
    let first = traced(Duration::from_micros(500), None, None, false);

    let second = traced(Duration::from_micros(1700), None, None, false);

    assert_same_behavior_trace("afap twice, other wall script", &second, &first);
    assert_eq!(first.len(), TICKS);
    assert!(
        heading_changes(&first).len() >= 3,
        "the wander entity changed its heading at least twice in 30 s"
    );
}

/// REQ-K-3, REQ-K-4, D-5: stepped (several chunkings), scaled and real-time runs, each with and
/// without a pause/resume at tick 100, give exactly the as-fast-as-possible hashes and intents.
#[test]
#[ignore = "WI-13"]
fn given_plugin_wander_when_run_in_each_clock_mode_then_matches_afap() {
    let want = &*BASELINE;
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
            assert_same_behavior_trace(&format!("{name}, {tag}"), &stepped(chunks, pause), want);
        }
        assert_same_behavior_trace(
            &format!("scaled 4x, {tag}"),
            &traced(Duration::from_micros(500), None, Some(scaled_4x()), pause),
            want,
        );
        assert_same_behavior_trace(
            &format!("realtime, {tag}"),
            &traced(
                Duration::from_micros(500),
                None,
                Some(ClockMode::RealTime),
                pause,
            ),
            want,
        );
    }
}

/// REQ-K-4, ADR 0020: the seed is an input that matters to a plugin: the wander draws its headings from
/// the host-provided entity stream, so different seeds give different headings and final hashes
/// (a private unhashed stream in the plugin would make them equal, AT-14 mutation), and the same
/// seed repeats.
#[test]
#[ignore = "WI-13"]
fn given_different_seeds_then_headings_and_hashes_differ() {
    let seeds = [1_u64, 2, 3, 4];
    let runs: Vec<Trace> = seeds
        .iter()
        .map(|&s| traced(Duration::from_micros(500), Some(s), None, false))
        .collect();

    let finals: std::collections::BTreeSet<_> =
        runs.iter().map(|t| t.hashes.last().copied()).collect();
    assert_eq!(
        finals.len(),
        seeds.len(),
        "every seed has its own final hash"
    );
    let headings: std::collections::BTreeSet<_> = runs.iter().map(heading_changes).collect();
    assert_eq!(
        headings.len(),
        seeds.len(),
        "every seed has its own headings"
    );
    let again = traced(Duration::from_micros(500), Some(3), None, false);
    assert_same_behavior_trace("seed 3 twice", &again, &runs[2]);
}

/// ADR 0022, REQ-K-4: the plugin load order follows the package names, never the order the directory
/// lists them (here: created in opposite orders), so registries, hosts and runs are the same.
#[test]
#[ignore = "WI-13"]
fn plugin_load_order_is_independent_of_directory_listing_order() {
    let wander = plugin_support::library("example-wander", &[]);
    let fixture = plugin_support::library("umbra-test-plugin", &[]);
    let forwards = plugin_support::plugins_dir(&[("a-wander", &wander), ("z-fixture", &fixture)]);
    let backwards = plugin_support::plugins_dir(&[("z-fixture", &fixture), ("a-wander", &wander)]);

    let ids = |dir: &PathBuf| -> Vec<String> {
        let (_, host) = load_plugins(
            &standard(),
            std::slice::from_ref(dir),
            HostOptions::default(),
        )
        .unwrap_or_else(|e| panic!("load_plugins: {e}"));
        host.plugins()
            .iter()
            .map(|p| p.id.as_str().to_owned())
            .collect()
    };

    let (a, b) = (ids(&forwards), ids(&backwards));
    assert_eq!(a, b, "same load order");
    assert_eq!(
        a,
        ["example.plugin.wander", "test.plugin.fixture"],
        "package name order"
    );
    let run = |dir: &PathBuf| {
        let loaded = load(dir, &example());
        let engine =
            instantiate_with_plugins(&loaded, options(Duration::from_micros(500), None, None))
                .expect("instantiate")
                .engine;
        run_to(engine, 300, false)
    };
    assert_same_behavior_trace("listing order", &run(&backwards), &run(&forwards));
}

/// ADR 0021 (R-M3), REQ-K-4: fault state belongs to the engine's host, not to the library image the
/// engines share. The second engine, loaded after the first has faulted and disabled a capability
/// of the same library, has its own fault history and runs exactly like the first.
#[test]
#[ignore = "WI-12"]
fn second_engine_in_process_is_not_affected_by_the_first() {
    let plugins = plugin_support::fixture(&[]);
    let scenario = plugin_support::fault_scenario(
        "second-engine",
        "disable_capability",
        3.0,
        &["healthy", "direct"],
    );
    let run_once = || {
        let loaded = load(&plugins, &scenario);
        let engine =
            instantiate_with_plugins(&loaded, options(Duration::from_micros(500), None, None))
                .expect("instantiate")
                .engine;
        let trace = run_to(engine, 90, false);
        let faults = loaded.plugins.faults();
        // The loaded scenario (and so its host) is returned to stay alive: the second engine runs
        // while the first engine's host still holds its fault history.
        (trace, faults, loaded)
    };

    let (first, first_faults, first_loaded) = run_once();
    let (second, second_faults, _second_loaded) = run_once();

    assert_eq!(first_faults.len(), 1, "exactly one fault: {first_faults:?}");
    assert_eq!(first_faults[0].call, "tick_batch");
    assert_eq!(
        second_faults, first_faults,
        "the second engine faulted by itself, at the same call with the same message"
    );
    assert_same_behavior_trace("second engine", &second, &first);
    assert_eq!(
        first_loaded.plugins.faults(),
        first_faults,
        "the first host's history is unchanged by the second engine"
    );
}
