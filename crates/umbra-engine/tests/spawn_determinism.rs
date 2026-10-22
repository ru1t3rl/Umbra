//! AT-22 (M7, gate 4 successor): spawning and despawning do not perturb determinism. The scenarios
//! `m7/spawn_order` (two carriers, four spawns in tick 1), `m7/spawn_selfish` (a spawn in tick 2,
//! its removal in tick 3) and `m7/spawn_flood` (256 applied, 44 capped) produce the same state hash
//! and entity count on every tick across as-fast-as-possible runs with different wall scripts,
//! stepped chunkings, scaled and real-time modes, a pause and resume, and a mode switch.
//! Requirements: REQ-K-3, REQ-K-4, REQ-B-2, ADR 0052, ASM-602, ASM-603.
//!
//! On the first divergent tick the failure names the entity and the field (`common`). Written in
//! M7 WI-1: ignored until M7 WI-4 applies the intents (before it every run is trivially equal and
//! proves nothing about the spawner).

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod common;
mod m7_support;

use core::time::Duration;

use common::{Trace, assert_same_trace, options, run_to, run_traced, step_traced};
use m7_support::{fixture, launcher_registry};
use umbra_engine::scenario::{instantiate, load_scenario};
use umbra_engine::{ClockMode, Engine, RunUntil, TimeScale};

const TICKS: u64 = 300;
const SCENARIOS: [&str; 3] = ["spawn_order", "spawn_selfish", "spawn_flood"];

fn engine(dir: &str, read_cost: Duration, clock: Option<ClockMode>) -> Engine {
    let registry = launcher_registry();
    let scenario = load_scenario(&registry, &fixture(dir)).expect("load");
    instantiate(&registry, &scenario, options(read_cost, None, clock))
        .expect("instantiate")
        .engine
}

fn baseline(dir: &str) -> Trace {
    run_to(engine(dir, Duration::from_micros(500), None), TICKS, false)
}

/// The spawner changes the world: the entity count grows or shrinks in these scenarios, so the
/// comparisons below compare something.
#[test]
#[ignore = "M7 WI-4"]
fn given_the_scenarios_then_their_runs_spawn_and_despawn_entities() {
    let counts = |dir: &str| -> Vec<usize> {
        baseline(dir)
            .snapshots
            .iter()
            .map(|s| s.entities.len())
            .collect()
    };

    assert_eq!(counts("spawn_order")[..2], [6, 6]);
    assert_eq!(counts("spawn_selfish")[..4], [1, 2, 1, 1]);
    assert_eq!(counts("spawn_flood")[0], 257);
}

/// REQ-K-4: two runs with different wall-time scripts hash alike on every tick.
#[test]
#[ignore = "M7 WI-4"]
fn given_a_spawning_scenario_when_run_twice_with_different_wall_scripts_then_hashes_are_identical()
{
    for dir in SCENARIOS {
        let other = run_to(engine(dir, Duration::from_micros(1700), None), TICKS, false);

        assert_same_trace(
            &format!("{dir}: afap, other wall script"),
            &other,
            &baseline(dir),
        );
    }
}

/// REQ-K-3, REQ-K-4: stepped (several chunkings), scaled and real-time runs, each with and without
/// a pause/resume at tick 100, give the as-fast-as-possible hashes.
#[test]
#[ignore = "M7 WI-4"]
fn given_a_spawning_scenario_when_run_in_each_clock_mode_then_hashes_match_afap() {
    let scaled = ClockMode::Scaled(TimeScale::new(4.0).expect("valid scale"));
    for dir in SCENARIOS {
        let want = baseline(dir);
        for pause in [false, true] {
            for (name, chunks) in [
                ("stepped x1", &[1_u64; 300][..]),
                ("stepped 1,7,64,rest", &[1, 7, 64, 228][..]),
                ("stepped 100,200", &[100, 200][..]),
            ] {
                let mut e = engine(dir, Duration::from_micros(500), Some(ClockMode::Stepped));
                let mut got = Trace::default();
                for &n in chunks {
                    step_traced(&mut e, n, &mut got);
                    if pause && got.len() == 100 {
                        e.pause();
                        run_traced(&mut e, RunUntil::Idle, &mut got);
                        assert_eq!(got.len(), 100, "no tick while paused");
                        e.resume();
                    }
                }
                assert_same_trace(&format!("{dir}: {name}, pause {pause}"), &got, &want);
            }
            for (name, mode) in [("scaled 4x", scaled), ("real time", ClockMode::RealTime)] {
                let got = run_to(
                    engine(dir, Duration::from_micros(500), Some(mode)),
                    TICKS,
                    pause,
                );
                assert_same_trace(&format!("{dir}: {name}, pause {pause}"), &got, &want);
            }
        }
    }
}

/// REQ-K-3: a clock mode switch in the middle of a spawn burst changes no hash.
#[test]
#[ignore = "M7 WI-4"]
fn given_a_mode_switch_around_the_spawn_tick_then_hashes_match_afap() {
    let mut e = engine("spawn_order", Duration::from_micros(500), None);
    let mut got = Trace::default();

    run_traced(&mut e, RunUntil::Ticks(1), &mut got);
    e.set_clock(ClockMode::RealTime);
    run_traced(&mut e, RunUntil::Ticks(3), &mut got);
    e.set_clock(ClockMode::Stepped);
    step_traced(&mut e, TICKS - 3, &mut got);

    assert_same_trace(
        "spawn_order: afap -> realtime -> stepped",
        &got,
        &baseline("spawn_order"),
    );
}
