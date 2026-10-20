//! AT-5 (gate 4 from WI-10): the engine handle does not perturb the simulation. A run driven through
//! `EngineHandle` (thread, command queue, snapshot bus, pump) has, tick by tick, the state hash,
//! intents and contacts of the same scenario run by `Engine::run`, whatever the subscribers do.
//! Requirements: REQ-K-4, A-2 (same OS, ADR 0003), ADR 0024.
//!
//! Hashes come from lossless subscriptions (state hash on every tick frame) or, where the test needs
//! a run with nobody subscribed, from the replies of one-tick steps on the stepped clock. A failure
//! names the first divergent tick, the entity and the field (`common::assert_same_trace`).
//!
//! The golden final hashes are read from `scripts/gates.sh` (its `GOLDENS` list is the one source);
//! `plugin_c_circle`, which needs the C example plugin, is left to gate 5 (ASM-178).
//!
//! The plugin scenario loads the real `example-wander` library, so every test that loads it takes
//! the plugin lock (rule m8).

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]
#![expect(clippy::panic, reason = "tests fail by panicking")]

mod common;
mod handle_support;
mod plugin_support;

use core::time::Duration;
use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};

use common::{Trace, assert_same_trace, run_traced};
use handle_support::{config, example, lossless, plugin_lock, start_engine, until_stopped};
use umbra_core::{Geodetic, Position, StateHash, TemplateId};
use umbra_engine::builtins::standard_registry;
use umbra_engine::bus::{FrameCause, SubscribeOptions, Subscription};
use umbra_engine::handle::{EngineHandle, HandleConfig};
use umbra_engine::scenario::{RunOptions, instantiate, load_scenario};
use umbra_engine::{
    ClockMode, EntityId, ManualWallClock, RunUntil, TemplateSpawn, instantiate_with_plugins,
    load_scenario_with_plugins,
};

const SCENARIOS: [&str; 3] = ["walking_skeleton", "patrol_and_engage", "plugin_wander"];

fn wall() -> Box<dyn umbra_engine::WallClock> {
    Box::new(ManualWallClock::with_auto_advance(Duration::from_micros(
        500,
    )))
}

fn plugins_for(name: &str) -> Vec<PathBuf> {
    if name == "plugin_wander" {
        vec![plugin_support::wander()]
    } else {
        Vec::new()
    }
}

/// `Engine::run` for `ticks` ticks (the scenario's duration when `None`), the reference.
fn reference(name: &str, seed: Option<u64>, ticks: Option<u64>) -> Trace {
    let registry = standard_registry().expect("registry");
    let path = example(name);
    let options = RunOptions {
        wall: wall(),
        seed,
        clock: None,
    };
    let plugins = plugins_for(name);
    // `loaded` owns the plugin host and must outlive the run.
    let loaded;
    let mut run = if plugins.is_empty() {
        let scenario = load_scenario(&registry, &path).expect("load");
        instantiate(&registry, &scenario, options).expect("instantiate")
    } else {
        loaded = load_scenario_with_plugins(&registry, &path, &plugins).expect("load plugins");
        instantiate_with_plugins(&loaded, options).expect("instantiate")
    };
    let count = ticks.unwrap_or(run.duration_ticks);
    let mut trace = Trace::default();
    run_traced(&mut run.engine, RunUntil::Ticks(count), &mut trace);
    assert_eq!(
        trace.len(),
        count,
        "{name}: the reference ran all its ticks"
    );
    trace
}

fn config_for(name: &str) -> HandleConfig {
    let mut config = config();
    config.load.extra_plugin_paths = plugins_for(name);
    config
}

/// Runs `name` to its end through a handle and records every tick frame. `extras` are subscribed
/// before the start and returned (so the test can inspect them) after the run.
fn handle_run(
    name: &str,
    seed: Option<u64>,
    extras: &[SubscribeOptions],
) -> (Trace, Vec<Subscription>) {
    let (handle, _thread) = start_engine(config_for(name));
    handle.load(example(name), seed, None).expect("load");
    let recorder = handle.subscribe(lossless());
    let others: Vec<Subscription> = extras
        .iter()
        .map(|o| handle.subscribe(clone_options(o)))
        .collect();

    handle.start().expect("start");
    let frames = until_stopped(&recorder);

    let mut trace = Trace::default();
    for frame in frames.iter().filter(|f| f.cause == FrameCause::Tick) {
        trace.hashes.push(
            frame
                .state_hash
                .expect("a state hash on every recorded tick"),
        );
        trace
            .snapshots
            .push(frame.snapshot.clone().expect("a world on every tick frame"));
    }
    (trace, others)
}

fn clone_options(from: &SubscribeOptions) -> SubscribeOptions {
    let mut copy = SubscribeOptions::new();
    copy.capacity = from.capacity;
    copy.min_tick_stride = from.min_tick_stride;
    copy.max_carry_events = from.max_carry_events;
    copy.with_state_hash = from.with_state_hash;
    copy
}

/// Full equality of the recorded worlds (state, intents and contacts of every entity), naming the
/// first tick and entity that differ.
fn assert_same_worlds(label: &str, got: &Trace, want: &Trace) {
    assert_same_trace(label, got, want);
    for (i, (g, w)) in got.snapshots.iter().zip(&want.snapshots).enumerate() {
        for (a, b) in g.entities.iter().zip(&w.entities) {
            assert_eq!(a, b, "{label}: tick {} entity {}", i + 1, a.id);
        }
        assert_eq!(g.time, w.time, "{label}: tick {} time", i + 1);
    }
}

fn spawn_spec(name: &str) -> TemplateSpawn {
    let at = Geodetic::from_degrees(52.001, 4.002, 0.0).expect("valid position");
    let template = TemplateId::new("ground_vehicle.base").expect("template id");
    let mut spec = TemplateSpawn::new(template, Position::Geodetic(at));
    spec.name = Some(name.to_owned());
    spec
}

/// The state hash after each of `ticks` one-tick steps on the stepped clock, with nobody
/// subscribed unless `before_tick` subscribes.
fn stepped_hashes(
    handle: &EngineHandle,
    ticks: u64,
    mut before_tick: impl FnMut(&EngineHandle, u64),
) -> Vec<StateHash> {
    (0..ticks)
        .map(|tick| {
            before_tick(handle, tick);
            handle.step(1).expect("step").state_hash
        })
        .collect()
}

fn stepped_engine(seed: Option<u64>) -> (EngineHandle, umbra_engine::handle::EngineThread) {
    let (handle, thread) = start_engine(config());
    handle
        .load(example("walking_skeleton"), seed, Some(ClockMode::Stepped))
        .expect("load");
    handle.start().expect("start");
    (handle, thread)
}

#[test]
#[ignore = "WI-8"]
fn handle_run_equals_engine_run_per_tick_hashes_intents_contacts() {
    let _lock = plugin_lock();
    for name in SCENARIOS {
        let want = reference(name, None, None);
        let (got, _) = handle_run(name, None, &[]);

        assert_same_worlds(name, &got, &want);
    }
    // The comparison can see intents and contacts: the engagement scenario has both.
    let patrol = reference("patrol_and_engage", None, None);
    let busy = |pick: fn(&umbra_engine::EntitySnapshot) -> bool| {
        patrol.snapshots.iter().any(|s| s.entities.iter().any(pick))
    };
    assert!(
        busy(|e| !e.intents.is_empty()),
        "no intents in the reference"
    );
    assert!(
        busy(|e| !e.contacts.is_empty()),
        "no contacts in the reference"
    );
}

#[test]
#[ignore = "WI-8"]
fn final_hashes_are_the_goldens() {
    let _lock = plugin_lock();
    let gates = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/gates.sh");
    let text = std::fs::read_to_string(&gates).expect("scripts/gates.sh");
    let goldens: BTreeMap<&str, &str> = text
        .lines()
        .map(|l| l.trim_start_matches("GOLDENS='"))
        .filter_map(|l| {
            let mut words = l.split_whitespace();
            Some((words.next()?, words.next()?))
        })
        .filter(|(name, hash)| {
            SCENARIOS.contains(name)
                && hash.len() == 16
                && hash.bytes().all(|b| b.is_ascii_hexdigit())
        })
        .collect();
    assert_eq!(
        goldens.len(),
        SCENARIOS.len(),
        "found {goldens:?} in the GOLDENS list"
    );

    for (name, golden) in goldens {
        let (trace, _) = handle_run(name, None, &[]);

        let last = trace.hashes.last().expect("ticks ran");
        assert_eq!(last.to_string(), golden, "{name}");
    }
}

#[test]
#[ignore = "WI-9"]
fn slow_absent_and_strided_subscribers_change_no_hash() {
    const TICKS: u64 = 120;
    let want = reference("walking_skeleton", None, Some(TICKS));
    let mut slow = SubscribeOptions::new();
    slow.capacity = NonZeroUsize::MIN;
    let mut strided = SubscribeOptions::new();
    strided.min_tick_stride = std::num::NonZeroU64::new(7).expect("non-zero");
    strided.with_state_hash = true;
    let (full, extras) = handle_run("walking_skeleton", None, &[slow, strided]);

    let (handle, _thread) = stepped_engine(None);
    let absent = stepped_hashes(&handle, TICKS, |_, _| {});

    assert_same_worlds(
        "a slow and a strided subscriber beside the recorder",
        &full,
        &reference("walking_skeleton", None, None),
    );
    assert_eq!(absent, want.hashes, "nobody subscribed");
    assert!(
        extras[0].stats().dropped_total > 0,
        "the slow subscriber really dropped frames"
    );
}

#[test]
#[ignore = "WI-9"]
fn priming_changes_no_hash() {
    const TICKS: u64 = 60;
    let want = reference("walking_skeleton", None, Some(TICKS)).hashes;
    let (handle, _thread) = stepped_engine(None);
    let mut kept = Vec::new();

    // Subscribing primes the new slot with a frame of the current state: at tick 0, between ticks
    // and while paused.
    let got = stepped_hashes(&handle, TICKS, |h, tick| {
        if tick % 10 == 0 {
            kept.push(h.subscribe(SubscribeOptions::new()));
        }
        if tick == 35 {
            h.pause().expect("pause");
            kept.push(h.subscribe(SubscribeOptions::new()));
            h.resume().expect("resume");
        }
    });

    assert_eq!(got, want);
    assert_eq!(kept.len(), 7);
}

#[test]
#[ignore = "WI-9"]
fn same_command_script_at_same_ticks_gives_same_hashes() {
    const TICKS: u64 = 30;
    let script = |tick: u64, handle: &EngineHandle| match tick {
        10 => {
            handle.spawn_entity(spawn_spec("late")).expect("spawn");
        }
        15 => {
            handle.pause().expect("pause");
            handle.resume().expect("resume");
        }
        20 => {
            handle
                .remove_entity(EntityId::new(2).expect("non-zero"))
                .expect("remove");
        }
        _ => {}
    };
    let run = || {
        let (handle, _thread) = stepped_engine(None);
        stepped_hashes(&handle, TICKS, |h, tick| script(tick, h))
    };
    let plain = reference("walking_skeleton", None, Some(TICKS)).hashes;

    let (first, second) = (run(), run());

    assert_eq!(
        first, second,
        "same commands at the same ticks, same hashes"
    );
    assert_eq!(
        first[..10],
        plain[..10],
        "before the first command nothing differs"
    );
    assert_ne!(first[10], plain[10], "the spawn applied before tick 11");
}

#[test]
#[ignore = "WI-8"]
fn different_seed_differs() {
    let default_seed = handle_run("walking_skeleton", None, &[]).0.hashes;
    let other = handle_run("walking_skeleton", Some(43), &[]).0.hashes;
    let other_again = handle_run("walking_skeleton", Some(43), &[]).0.hashes;

    assert_eq!(other, other_again, "a seed is repeatable");
    assert_eq!(other.len(), default_seed.len());
    assert_ne!(
        other, default_seed,
        "the hashed RNG state differs with the seed"
    );
}

#[test]
fn the_reference_itself_is_repeatable() {
    // extra: runs now. Two reference runs agree tick by tick, so a handle divergence is the
    // handle's, not the reference's.
    let first = reference("walking_skeleton", None, Some(200));
    let second = reference("walking_skeleton", None, Some(200));

    assert_same_worlds("reference twice", &second, &first);
    assert_eq!(first.len(), 200);
}
