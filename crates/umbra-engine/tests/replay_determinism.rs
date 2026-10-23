//! AT-22 (M7, gate 4 successor): recording and replaying are deterministic and do not perturb the
//! run. Two recordings of the same inputs are byte-identical; recording changes no per-tick hash;
//! two replays of one recording give the same per-tick hashes as the live run ("replay of a replay is
//! identical", plan guarantee). Reference scenarios: `walking_skeleton`, `patrol_and_engage` and the
//! spawning `m7/spawn_order`.
//! Requirements: REQ-K-4, REQ-K-7, ADR 0053, ADR 0054, ASM-621, ASM-624.
//!
//! Written in M7 WI-1 against the recorder and replay stubs: ignored until M7 WI-13 (recorder) and
//! M7 WI-14 (replay).

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod common;
mod m7_support;

use core::time::Duration;

use common::{Trace, options, run_to};
use m7_support::{example, fixture, launcher_registry, record, replay_file};
use umbra_engine::scenario::{instantiate, load_scenario};

const TICKS: u64 = 300;

fn scenario(name: &str) -> std::path::PathBuf {
    if name.starts_with("spawn_") {
        fixture(name)
    } else {
        example(name)
    }
}

/// A plain run without a recorder: the reference for "recording changes nothing".
fn unrecorded(name: &str) -> Trace {
    let registry = launcher_registry();
    let loaded = load_scenario(&registry, &scenario(name)).expect("load");
    let run = instantiate(
        &registry,
        &loaded,
        options(Duration::from_micros(500), None, None),
    )
    .expect("instantiate");
    run_to(run.engine, TICKS, false)
}

/// REQ-K-7, ADR 0053: attaching a recorder does not change a single per-tick hash.
#[test]
fn given_a_recorder_when_a_scenario_runs_then_per_tick_hashes_equal_the_unrecorded_run() {
    for name in ["walking_skeleton", "patrol_and_engage", "spawn_order"] {
        let recorded = record(
            &launcher_registry(),
            &scenario(name),
            &[],
            Some(TICKS),
            None,
        );
        let want = unrecorded(name);

        let got: Vec<_> = recorded.hashes.clone();

        assert_eq!(got, want.hashes, "{name}");
    }
}

/// ASM-621: two recordings of the same run are byte-identical (no wall-clock value in the file).
#[test]
fn given_one_run_recorded_twice_then_the_files_are_byte_identical() {
    for name in ["walking_skeleton", "patrol_and_engage", "spawn_order"] {
        let first = record(
            &launcher_registry(),
            &scenario(name),
            &[],
            Some(TICKS),
            None,
        );
        let second = record(
            &launcher_registry(),
            &scenario(name),
            &[],
            Some(TICKS),
            None,
        );

        let (a, b) = (
            std::fs::read(&first.file).expect("read"),
            std::fs::read(&second.file).expect("read"),
        );

        assert_eq!(a, b, "{name}");
    }
}

/// REQ-K-4: replay reproduces the live run's per-tick hashes, and the replay's own tick hashes are
/// the same on a second replay.
#[test]
#[ignore = "M7 WI-14"]
fn given_a_recording_when_replayed_twice_then_both_replays_equal_the_live_hashes() {
    for name in ["walking_skeleton", "patrol_and_engage", "spawn_order"] {
        let registry = launcher_registry();
        let live = record(&registry, &scenario(name), &[], Some(TICKS), None);

        let (first, _) = replay_file(&registry, &live.file, &[]).expect("replay");
        let (second, _) = replay_file(&registry, &live.file, &[]).expect("replay");

        assert_eq!(first, live.hashes, "{name}: replay vs live");
        assert_eq!(second, first, "{name}: replay vs replay");
    }
}
