//! AT-13 (M7, gate 4 row): the `air_sea` example (air, sea and ground models in one world) hashes
//! alike on every tick across wall scripts, stepped chunkings, scaled and real-time modes and a
//! pause, so the air and sea models add no hidden input to a run. Requirements: REQ-K-3, REQ-K-4,
//! REQ-M-2, ADR 0051, ASM-607..611.
//!
//! The run is 600 ticks of the 9000-tick example: the aircraft is climbing and turning, the ship
//! and the truck are under way, so the comparison covers motion, not a parked world.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod common;
mod m7_support;

use core::time::Duration;

use common::{Trace, assert_same_trace, options, run_to, run_traced, step_traced};
use m7_support::example;
use umbra_engine::builtins::standard_registry;
use umbra_engine::scenario::{instantiate, load_scenario};
use umbra_engine::{ClockMode, Engine, RunUntil, TimeScale};

const TICKS: u64 = 600;

fn engine(read_cost: Duration, clock: Option<ClockMode>) -> Engine {
    let registry = standard_registry().expect("registry");
    let scenario = load_scenario(&registry, &example("air_sea")).expect("load air_sea");
    instantiate(&registry, &scenario, options(read_cost, None, clock))
        .expect("instantiate")
        .engine
}

fn baseline() -> Trace {
    run_to(engine(Duration::from_micros(500), None), TICKS, false)
}

/// The comparison is not vacuous: the world changes from tick to tick.
#[test]
fn given_the_air_sea_example_then_its_hashes_change_every_tick() {
    let hashes = baseline().hashes;

    assert!(hashes.windows(2).all(|w| w[0] != w[1]));
}

/// REQ-K-4: two runs with different wall-time scripts hash alike on every tick.
#[test]
fn given_air_sea_when_run_with_a_different_wall_script_then_hashes_are_identical() {
    let other = run_to(engine(Duration::from_micros(1700), None), TICKS, false);

    assert_same_trace("air_sea: afap, other wall script", &other, &baseline());
}

/// REQ-K-3, REQ-K-4: stepped (several chunkings), scaled and real-time runs, with and without a
/// pause at tick 100, give the as-fast-as-possible hashes.
#[test]
fn given_air_sea_when_run_in_each_clock_mode_then_hashes_match_afap() {
    let scaled = ClockMode::Scaled(TimeScale::new(4.0).expect("valid scale"));
    let want = baseline();
    for pause in [false, true] {
        for (name, chunks) in [
            ("stepped x1", &[1_u64; 600][..]),
            ("stepped 1,7,64,rest", &[1, 7, 64, 528][..]),
            ("stepped 100,500", &[100, 500][..]),
        ] {
            let mut e = engine(Duration::from_micros(500), Some(ClockMode::Stepped));
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
            assert_same_trace(&format!("air_sea: {name}, pause {pause}"), &got, &want);
        }
        for (name, mode) in [("scaled 4x", scaled), ("real time", ClockMode::RealTime)] {
            let got = run_to(engine(Duration::from_micros(500), Some(mode)), TICKS, pause);
            assert_same_trace(&format!("air_sea: {name}, pause {pause}"), &got, &want);
        }
    }
}
