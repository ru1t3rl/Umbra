//! AT-19 / AT-22 (M7, gate 4 v3 rows, ADR 0055): the opt-in state hash v3 is deterministic. For the
//! reference scenarios `walking_skeleton`, `patrol_and_engage` and the spawning `m7/spawn_order`
//! and `m7/spawn_selfish`, the v3 hash of every tick agrees across as-fast-as-possible runs with
//! different wall scripts, stepped chunkings, scaled and real-time modes, and a pause at tick 100.
//! v3 is also never equal to v2 for the same tick. Requirements: REQ-K-3, REQ-K-4, D-30, ADR 0055.
//!
//! Written in M7 WI-1: ignored until M7 WI-15 (which also adds these files to the gate 4 command in
//! `docs/build.md` and `scripts/gates.sh`).

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod common;
mod m7_support;

use core::ops::ControlFlow;
use core::time::Duration;

use common::options;
use m7_support::{example, fixture, launcher_registry};
use umbra_core::StateHash;
use umbra_engine::scenario::{instantiate, load_scenario};
use umbra_engine::{ClockMode, Engine, StateHashVersion, TimeScale};

const TICKS: u64 = 300;

fn scenario(name: &str) -> std::path::PathBuf {
    if name.starts_with("spawn_") {
        fixture(name)
    } else {
        example(name)
    }
}

fn engine(name: &str, read_cost: Duration, clock: Option<ClockMode>) -> Engine {
    let registry = launcher_registry();
    let loaded = load_scenario(&registry, &scenario(name)).expect("load");
    instantiate(&registry, &loaded, options(read_cost, None, clock))
        .expect("instantiate")
        .engine
}

/// The v3 and v2 hash after every tick of `TICKS` ticks, stepping in `chunks` when the clock is
/// stepped and pausing at tick 100 when `pause` is set.
fn hashes(mut e: Engine, chunks: &[u64], pause: bool) -> (Vec<StateHash>, Vec<StateHash>) {
    let (mut v3, mut v2) = (Vec::new(), Vec::new());
    let mut record = |view: &umbra_engine::TickView<'_>| {
        v3.push(view.state_hash_with(StateHashVersion::V3));
        v2.push(view.state_hash());
        ControlFlow::Continue(())
    };
    if e.clock_mode() == ClockMode::Stepped {
        for &n in chunks {
            e.step(n, &mut record).expect("step");
        }
    } else {
        if pause {
            e.run(umbra_engine::RunUntil::Ticks(100), &mut record)
                .expect("run");
            e.pause();
            e.resume();
        }
        e.run(umbra_engine::RunUntil::Ticks(TICKS), &mut record)
            .expect("run");
    }
    (v3, v2)
}

/// REQ-K-4: v3 agrees across runs and modes, and differs from v2 on every tick.
#[test]
#[ignore = "M7 WI-15"]
fn v3_hashes_match_across_wall_scripts_and_clock_modes() {
    let scaled = ClockMode::Scaled(TimeScale::new(4.0).expect("valid scale"));
    for name in [
        "walking_skeleton",
        "patrol_and_engage",
        "spawn_order",
        "spawn_selfish",
    ] {
        let (want, v2) = hashes(engine(name, Duration::from_micros(500), None), &[], false);
        assert_eq!(want.len() as u64, TICKS, "{name}");
        assert!(
            want.iter().zip(&v2).all(|(a, b)| a != b),
            "{name}: v3 differs from v2"
        );

        let (other_script, _) = hashes(engine(name, Duration::from_micros(1700), None), &[], false);
        assert_eq!(other_script, want, "{name}: other wall script");
        for (label, chunks) in [
            ("x1", vec![1_u64; 300]),
            ("1,7,64,rest", vec![1, 7, 64, 228]),
        ] {
            let e = engine(name, Duration::from_micros(500), Some(ClockMode::Stepped));
            assert_eq!(hashes(e, &chunks, false).0, want, "{name}: stepped {label}");
        }
        for (label, mode) in [("scaled", scaled), ("real time", ClockMode::RealTime)] {
            for pause in [false, true] {
                let e = engine(name, Duration::from_micros(500), Some(mode));
                assert_eq!(
                    hashes(e, &[], pause).0,
                    want,
                    "{name}: {label}, pause {pause}"
                );
            }
        }
    }
}
