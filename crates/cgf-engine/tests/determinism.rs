//! AT-1 (gate 4): the determinism gate. Requirements: M1 acceptance 1, REQ-K-3, REQ-K-4, D-5,
//! A-2 (same OS, ADR 0003).
//!
//! The demo scenario (3 ground entities, seed 42, 30 Hz, 900 ticks) is run repeatedly and the
//! state hash after every tick is compared: twice in as-fast-as-possible mode, in stepped chunks,
//! scaled and real-time modes on a manual wall clock (no sleeping), with a pause/resume inserted at
//! tick 100, and across a mid-run mode switch. On the first divergent tick the failure message
//! names the entity and the field that differ.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

use core::time::Duration;

use std::ops::ControlFlow;
use std::sync::Arc;

use cgf_core::{Meters, StateHash, TickRate};
use cgf_engine::{Engine, EngineConfig, RunUntil, TickView, demo};
use cgf_kernel::{ClockMode, EntitySnapshot, ManualWallClock, Snapshot, TimeScale};

const SEED: u64 = 42;
const TICKS: u64 = 900;
const PAUSE_AT: u64 = 100;

/// The per-tick record of one run: state hash and snapshot after each tick.
#[derive(Default)]
struct Trace {
    hashes: Vec<StateHash>,
    snapshots: Vec<Snapshot>,
}

impl Trace {
    fn record(&mut self, view: &TickView<'_>) -> ControlFlow<()> {
        self.hashes.push(view.state_hash());
        self.snapshots.push(view.snapshot());
        ControlFlow::Continue(())
    }

    fn len(&self) -> u64 {
        u64::try_from(self.hashes.len()).expect("length fits u64")
    }
}

fn config(seed: u64, clock: ClockMode) -> EngineConfig {
    EngineConfig {
        seed,
        tick_rate: TickRate::new(30).expect("non-zero rate"),
        clock,
    }
}

/// A demo engine on a manual wall clock that advances 0.5 ms on every read.
fn demo_engine(seed: u64, clock: ClockMode) -> Engine {
    demo_engine_with_wall(seed, clock, Duration::from_micros(500))
}

/// A demo engine whose manual wall clock advances by `read_cost` on every read, so different
/// costs give different wall-time scripts for the same simulation.
fn demo_engine_with_wall(seed: u64, clock: ClockMode, read_cost: Duration) -> Engine {
    let wall = ManualWallClock::with_auto_advance(read_cost);
    let mut engine = Engine::with_wall_clock(
        config(seed, clock),
        Arc::new(cgf_core::FlatTerrain {
            height: Meters(0.0),
        }),
        Box::new(wall),
    );
    let ids = demo::spawn_walking_skeleton(&mut engine).expect("spawn demo");
    assert_eq!(ids.len(), 3);
    engine
}

fn scaled_4x() -> ClockMode {
    ClockMode::Scaled(TimeScale::new(4.0).expect("valid scale"))
}

/// Runs the demo to `TICKS` with `run`, optionally pausing and resuming at `PAUSE_AT`.
fn run_to_end(mut engine: Engine, pause: bool) -> Trace {
    let mut trace = Trace::default();
    if pause {
        engine
            .run(RunUntil::Ticks(PAUSE_AT), |v| trace.record(v))
            .expect("run to pause");
        engine.pause();
        engine
            .run(RunUntil::Ticks(TICKS), |v| trace.record(v))
            .expect("run while paused");
        assert_eq!(trace.len(), PAUSE_AT, "no tick may run while paused");
        engine.resume();
    }
    engine
        .run(RunUntil::Ticks(TICKS), |v| trace.record(v))
        .expect("run to end");
    trace
}

/// Runs the demo in stepped mode in the given chunks (summing to `TICKS`), optionally pausing
/// and resuming once the first `PAUSE_AT` ticks are done.
fn run_stepped(chunks: &[u64], pause: bool) -> Trace {
    let mut engine = demo_engine(SEED, ClockMode::Stepped);
    let mut trace = Trace::default();
    for &n in chunks {
        engine.step(n, |v| trace.record(v)).expect("step");
        if pause && trace.len() == PAUSE_AT {
            engine.pause();
            engine
                .run(RunUntil::Idle, |v| trace.record(v))
                .expect("run while paused");
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

fn baseline() -> Trace {
    run_to_end(demo_engine(SEED, ClockMode::AsFastAsPossible), false)
}

/// The first difference between two entity snapshots, as `(field, left, right)`.
fn entity_diff(a: &EntitySnapshot, b: &EntitySnapshot) -> Option<(String, String, String)> {
    if a.id != b.id {
        return Some((
            "id (entity order differs)".into(),
            a.id.to_string(),
            b.id.to_string(),
        ));
    }
    let bits = |x: f64| format!("{x:?} (bits {:#018x})", x.to_bits());
    let fields = [
        ("position.x", a.state.position.0.x, b.state.position.0.x),
        ("position.y", a.state.position.0.y, b.state.position.0.y),
        ("position.z", a.state.position.0.z, b.state.position.0.z),
        ("velocity.x", a.state.velocity.x, b.state.velocity.x),
        ("velocity.y", a.state.velocity.y, b.state.velocity.y),
        ("velocity.z", a.state.velocity.z, b.state.velocity.z),
        (
            "orientation.w",
            a.state.orientation.w,
            b.state.orientation.w,
        ),
        (
            "orientation.x",
            a.state.orientation.x,
            b.state.orientation.x,
        ),
        (
            "orientation.y",
            a.state.orientation.y,
            b.state.orientation.y,
        ),
        (
            "orientation.z",
            a.state.orientation.z,
            b.state.orientation.z,
        ),
    ];
    for (name, x, y) in fields {
        if x.to_bits() != y.to_bits() {
            return Some((name.to_string(), bits(x), bits(y)));
        }
    }
    if a.route_next != b.route_next {
        return Some((
            "route_next".into(),
            format!("{:?}", a.route_next),
            format!("{:?}", b.route_next),
        ));
    }
    if a.ownership != b.ownership {
        return Some((
            "ownership".into(),
            format!("{:?}", a.ownership),
            format!("{:?}", b.ownership),
        ));
    }
    if a.motion_model != b.motion_model {
        return Some((
            "motion_model".into(),
            a.motion_model.to_string(),
            b.motion_model.to_string(),
        ));
    }
    None
}

/// Panics at the first divergent tick, naming the entity and field (REQ-K-4).
fn assert_same_trace(label: &str, got: &Trace, want: &Trace) {
    assert_eq!(
        got.hashes.len(),
        want.hashes.len(),
        "{label}: number of ticks"
    );
    for (i, (g, w)) in got.hashes.iter().zip(&want.hashes).enumerate() {
        if g == w {
            continue;
        }
        let (gs, ws) = (&got.snapshots[i], &want.snapshots[i]);
        let detail = if gs.entities.len() == ws.entities.len() {
            gs.entities
                .iter()
                .zip(&ws.entities)
                .find_map(|(a, b)| entity_diff(a, b).map(|d| (a.id, d)))
                .map_or_else(
                    || {
                        "snapshots equal: the difference is in RNG state or hash order (not in the snapshot)"
                            .to_string()
                    },
                    |(id, (field, a, b))| format!("entity {id}, field {field}: {a} vs {b}"),
                )
        } else {
            format!(
                "entity count {} vs {}",
                gs.entities.len(),
                ws.entities.len()
            )
        };
        panic!(
            "{label}: first divergent tick is {} (hash {g} vs {w}): {detail}",
            i + 1
        );
    }
}

/// M1 acceptance 1, REQ-K-4: two as-fast-as-possible runs of the same seed hash alike on every tick.
#[test]
fn given_demo_when_run_twice_afap_then_per_tick_hashes_are_identical() {
    let first = baseline();
    // A different wall-time script: wall time leaking into the simulation (for example into dt)
    // would make the runs differ.
    let second = run_to_end(
        demo_engine_with_wall(
            SEED,
            ClockMode::AsFastAsPossible,
            Duration::from_micros(1700),
        ),
        false,
    );

    assert_same_trace("afap twice, other wall script", &second, &first);
    assert_eq!(first.len(), TICKS);
}

/// REQ-K-4: the production constructor (standard wall clock) gives the same hashes as the manual
/// wall clock; wall time never reaches the state.
#[test]
fn given_demo_when_run_on_std_wall_clock_then_hashes_match_manual_wall_clock() {
    let mut engine = Engine::new(
        config(SEED, ClockMode::AsFastAsPossible),
        Arc::new(cgf_core::FlatTerrain {
            height: Meters(0.0),
        }),
    );
    assert!(demo::spawn_walking_skeleton(&mut engine).is_ok());
    let got = run_to_end(engine, false);

    assert_same_trace("std wall clock", &got, &baseline());
}

/// REQ-K-3, REQ-K-4, D-5: stepped (several chunkings), scaled and real-time runs, each with and
/// without a pause/resume at tick 100, produce exactly the as-fast-as-possible hashes.
#[test]
fn given_demo_when_run_in_each_clock_mode_then_per_tick_hashes_match_afap() {
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
            &run_to_end(demo_engine(SEED, scaled_4x()), pause),
            &want,
        );
        assert_same_trace(
            &format!("realtime, {tag}"),
            &run_to_end(demo_engine(SEED, ClockMode::RealTime), pause),
            &want,
        );
    }
}

/// REQ-K-3, REQ-K-4: switching clock mode mid-run (as fast as possible, then real time, then
/// stepped) does not change a single hash.
#[test]
fn given_mode_switch_mid_run_then_hashes_match_afap() {
    let mut engine = demo_engine(SEED, ClockMode::AsFastAsPossible);
    let mut got = Trace::default();

    engine
        .run(RunUntil::Ticks(300), |v| got.record(v))
        .expect("afap leg");
    engine.set_clock(ClockMode::RealTime);
    engine
        .run(RunUntil::Ticks(600), |v| got.record(v))
        .expect("realtime leg");
    assert_eq!(
        got.len(),
        600,
        "the real-time leg is bounded by total ticks"
    );
    engine.set_clock(ClockMode::Stepped);
    engine.step(300, |v| got.record(v)).expect("stepped leg");

    assert_same_trace("afap -> realtime -> stepped", &got, &baseline());
}

/// REQ-K-4: the seed is an input: different seeds give different final hashes.
#[test]
fn given_different_seeds_when_run_then_final_hashes_differ() {
    let a = run_to_end(demo_engine(1, ClockMode::AsFastAsPossible), false);
    let b = run_to_end(demo_engine(2, ClockMode::AsFastAsPossible), false);

    assert_ne!(a.hashes.last(), b.hashes.last());
    let final_position = |t: &Trace| t.snapshots.last().map(|s| s.entities[0].state.position);
    assert_ne!(
        final_position(&a),
        final_position(&b),
        "the seed changes where entity 1 ends up (its cruise speed)"
    );
}

/// REQ-K-4: one hash per tick, and the per-tick hashes of a moving scenario are all distinct.
#[test]
fn given_same_seed_then_hash_sequence_length_equals_ticks() {
    let trace = baseline();

    assert_eq!(trace.len(), TICKS);
    let distinct: std::collections::BTreeSet<_> = trace.hashes.iter().collect();
    assert_eq!(
        distinct.len(),
        trace.hashes.len(),
        "every tick changes the state hash"
    );
}

/// REQ-K-3: `RunUntil::Ticks` counts total ticks since start, so a second call with the same
/// bound runs nothing.
#[test]
fn given_run_until_reached_when_run_again_then_no_more_ticks_run() {
    let mut engine = demo_engine(SEED, ClockMode::AsFastAsPossible);
    let mut trace = Trace::default();

    engine
        .run(RunUntil::Ticks(50), |v| trace.record(v))
        .expect("first run");
    engine
        .run(RunUntil::Ticks(50), |v| trace.record(v))
        .expect("second run");

    assert_eq!(trace.len(), 50);
    assert_eq!(engine.now().tick(), 50);
}

/// REQ-K-3: `RunUntil::Idle` stops when a stepped clock has no pending steps.
#[test]
fn given_stepped_clock_without_requests_when_run_until_idle_then_returns_immediately() {
    let mut engine = demo_engine(SEED, ClockMode::Stepped);
    let mut trace = Trace::default();

    engine
        .run(RunUntil::Idle, |v| trace.record(v))
        .expect("run until idle");

    assert_eq!(trace.len(), 0);
}
