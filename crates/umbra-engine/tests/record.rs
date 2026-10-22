//! AT-11 (M7): the engine recorder. Requirements: REQ-K-7, REQ-K-4, REQ-K-6, REQ-K-8, ADR 0053,
//! ASM-621..627, ASM-633.
//!
//! The header carries the effective seed, tick rate and source, the file digests, the plugins and
//! the adapter key spaces; commands carry `before_tick` and `seq`; inbound messages are recorded as
//! polled; hashes follow the cadence; recording leaves every per-tick hash unchanged. Written in
//! M7 WI-1 against the recorder stub: ignored until M7 WI-13 (rows that need a plugin or a peer
//! name the same WI). Not covered here, left to WI-13's own rows: the `fault` record (needs a
//! faulting adapter fixture) and a write error marking the recording `Failed`.

#![forbid(unsafe_code)]
#![expect(clippy::panic, reason = "tests fail by panicking")]

mod common;
mod m7_support;
mod net_support;
mod plugin_support;

use core::time::Duration;
use std::path::Path;

use common::{options, run_to};
use m7_support::{
    example, fixture, launcher_registry, plugin_lock, record, scratch, with_launcher,
};
use net_support::{Peer, registry_with};
use umbra_core::hash::fnv1a64;
use umbra_core::net::KeySpace;
use umbra_engine::handle::{EngineHandle, HandleConfig};
use umbra_engine::scenario::{instantiate, load_scenario};
use umbra_engine::{
    ClockMode, Geodetic, Position, RecordOptions, Recorder, TemplateId, TemplateSpawn,
    load_scenario_with_plugins,
};
use umbra_net::LoopbackHub;
use umbra_record::{AdapterRef, Reader, Record, RecordedCommand};

const TICKS: u64 = 120;

fn read_all(file: &Path) -> (umbra_record::Header, Vec<Record>) {
    let (header, reader) = Reader::open(file).unwrap_or_else(|e| panic!("open: {e}"));
    let records = reader
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_else(|e| panic!("read: {e}"));
    (header, records)
}

fn hash_ticks(records: &[Record]) -> Vec<u64> {
    records
        .iter()
        .filter_map(|r| match r {
            Record::Hash { tick, .. } => Some(*tick),
            _ => None,
        })
        .collect()
}

/// REQ-K-7, ADR 0053: attaching a recorder changes no per-tick hash, in the scenarios of the gate.
#[test]
#[ignore = "M7 WI-13"]
fn recording_leaves_hashes_unchanged() {
    for name in ["walking_skeleton", "patrol_and_engage"] {
        let registry = launcher_registry();
        let scenario = load_scenario(&registry, &example(name)).expect("load");
        let plain = instantiate(
            &registry,
            &scenario,
            options(Duration::from_micros(500), None, None),
        )
        .expect("instantiate");

        let recorded = record(&registry, &example(name), &[], Some(TICKS), None);

        let want = run_to(plain.engine, TICKS, false);
        assert_eq!(recorded.hashes, want.hashes, "{name}");
    }
}

/// ADR 0053, ASM-621, ASM-623: the header holds the effective seed (an override, not the file's),
/// the tick rate and its source, the hash version and cadence, and the FNV-1a 64 of every scenario
/// file.
#[test]
#[ignore = "M7 WI-13"]
fn header_holds_effective_seed_rate_and_digests() {
    let scenario = example("walking_skeleton");

    let recorded = record(&launcher_registry(), &scenario, &[], Some(TICKS), Some(99));

    let (header, _) = read_all(&recorded.file);
    assert_eq!(header.seed, 99);
    assert_eq!(header.tick_rate_hz, 30);
    assert_eq!(header.tick_rate_source, "scenario");
    assert_eq!(header.state_hash_version, 2);
    assert_eq!(header.hash_every, 1);
    assert_eq!(header.on_plugin_fault, "disable_capability");
    assert!(header.plugins.is_empty() && header.network_adapters.is_empty());
    assert_eq!(
        header.scenario.path,
        scenario.canonicalize().expect("canonical")
    );
    assert!(header.scenario.files.len() > 3, "scenario, templates");
    for digest in &header.scenario.files {
        let bytes = std::fs::read(&digest.path).expect("a digested file is readable");
        assert_eq!(digest.fnv1a64, fnv1a64(&bytes), "{}", digest.path.display());
    }
    let named: Vec<_> = header
        .scenario
        .files
        .iter()
        .filter_map(|d| d.path.file_name()?.to_str().map(str::to_owned))
        .collect();
    assert!(named.contains(&"scenario.toml".to_owned()), "{named:?}");
    assert!(named.contains(&"truck_cargo.toml".to_owned()), "{named:?}");
}

/// ASM-633: the header stores the scenario path relative to the recording's directory when it can.
#[test]
#[ignore = "M7 WI-13"]
fn header_stores_a_relative_scenario_path_when_one_exists() {
    let scenario = fixture("spawn_basic");
    let recorded = record(&launcher_registry(), &scenario, &[], Some(5), None);

    let (header, _) = read_all(&recorded.file);

    let relative = header.scenario.relative_path.expect("a relative path");
    let from = recorded.file.parent().expect("directory");
    assert_eq!(
        from.join(&relative).canonicalize().expect("resolves"),
        scenario.canonicalize().expect("canonical")
    );
}

/// ASM-624: hashes follow the cadence (every 10th tick here) and the final tick is always hashed;
/// the last record is `end` with the live run's final hash.
#[test]
#[ignore = "M7 WI-13"]
fn hash_records_follow_the_cadence_and_the_file_ends_with_the_final_hash() {
    let registry = launcher_registry();
    let loaded =
        load_scenario_with_plugins(&registry, &example("walking_skeleton"), &[]).expect("load");
    let run = umbra_engine::instantiate_with_plugins(&loaded, m7_support::run_options(None))
        .expect("instantiate");
    let mut engine = run.engine;
    let file = scratch("cadence");
    let mut opts = RecordOptions::new(&file);
    opts.hash_every = Some(10);
    engine.attach_recorder(
        Recorder::create(opts, &example("walking_skeleton"), &loaded, None).expect("recorder"),
    );
    let mut last = None;

    engine
        .run(umbra_engine::RunUntil::Ticks(95), |v| {
            last = Some(v.state_hash());
            core::ops::ControlFlow::Continue(())
        })
        .expect("run");
    engine.finish_recording("completed").expect("finish");

    let (_, records) = read_all(&file);
    assert_eq!(
        hash_ticks(&records),
        [10, 20, 30, 40, 50, 60, 70, 80, 90, 95]
    );
    match records.last() {
        Some(Record::End(end)) => {
            assert_eq!((end.tick, end.reason.as_str()), (95, "completed"));
            assert_eq!(Some(end.hash), last);
        }
        other => panic!("the last record is {other:?}"),
    }
}

/// ASM-626: state-changing commands carry `before_tick` (ticks completed when applied), a
/// per-boundary `seq`, and the spawn the full spec and the resulting id; informational commands are
/// recorded too. Driven through the engine handle with `record_dir`.
#[test]
#[ignore = "M7 WI-13"]
fn handle_commands_are_recorded_with_before_tick_seq_and_result() {
    let dir = scratch("dir");
    let dir = dir.with_extension("d");
    std::fs::create_dir_all(&dir).expect("record dir");
    let hub = LoopbackHub::new(64);
    let mut config = HandleConfig::new(with_launcher(registry_with(&[(
        "test.net.loopback",
        &hub,
        1000,
    )])));
    config.wall = std::sync::Arc::new(|| -> Box<dyn umbra_engine::WallClock> {
        Box::new(umbra_engine::ManualWallClock::with_auto_advance(
            Duration::from_micros(500),
        ))
    });
    config.record_dir = Some(dir.clone());
    let (handle, _thread) = EngineHandle::spawn(config).expect("spawn");
    handle
        .load(fixture("handle_run"), Some(7), Some(ClockMode::Stepped))
        .expect("load");
    handle.start().expect("start");
    handle.step(5).expect("step");
    let spec = TemplateSpawn::new(
        TemplateId::new("m7.child").expect("template"),
        Position::Geodetic(Geodetic::from_degrees(52.001, 4.001, 0.0).expect("position")),
    );
    let spawned = handle.spawn_entity(spec).expect("spawn_entity");
    handle.step(3).expect("step");
    handle
        .set_adapter_enabled("lb".to_owned(), false)
        .expect("disable");
    handle.remove_entity(spawned.snapshot.id).expect("remove");
    handle.stop().expect("stop");
    handle.unload().expect("unload");

    let files: Vec<_> = std::fs::read_dir(&dir)
        .expect("dir")
        .map(|e| e.expect("entry").path())
        .collect();
    assert_eq!(files.len(), 1, "one file per session load: {files:?}");
    let (_, records) = read_all(&files[0]);
    let commands: Vec<_> = records
        .iter()
        .filter_map(|r| match r {
            Record::Command(c) => Some(c),
            _ => None,
        })
        .collect();
    let state_changing: Vec<_> = commands
        .iter()
        .filter(|c| {
            matches!(
                c.command,
                RecordedCommand::Spawn { .. }
                    | RecordedCommand::Remove { .. }
                    | RecordedCommand::SetAdapterEnabled { .. }
            )
        })
        .collect();
    assert_eq!(state_changing.len(), 3);
    assert_eq!(state_changing[0].before_tick, 5);
    assert!(
        matches!(&state_changing[0].command, RecordedCommand::Spawn { result, .. } if *result == spawned.snapshot.id)
    );
    assert_eq!(state_changing[1].before_tick, 8);
    assert!(
        matches!(&state_changing[1].command, RecordedCommand::SetAdapterEnabled { name, enabled: false } if name == "lb")
    );
    assert_eq!(state_changing[2].before_tick, 8);
    assert!(
        state_changing[1].seq < state_changing[2].seq,
        "application order within one boundary"
    );
    assert!(
        commands
            .iter()
            .any(|c| matches!(c.command, RecordedCommand::Start)),
        "informational start"
    );
    assert!(
        commands
            .iter()
            .any(|c| matches!(c.command, RecordedCommand::Step(_))),
        "informational step"
    );
}

/// ASM-627, ASM-633: the header lists the adapters with their key spaces, and the recorder stores
/// each adapter's poll result as polled, in tick order.
#[test]
#[ignore = "M7 WI-13"]
fn adapters_and_inbound_messages_are_recorded_as_polled() {
    let hub = LoopbackHub::new(4096);
    let a_registry = with_launcher(registry_with(&[("test.net.loopback", &hub, 1000)]));
    let b_registry = registry_with(&[("test.net.loopback", &hub, 2000)]);
    let scenario = fixture("handle_run");
    let loaded = load_scenario_with_plugins(&a_registry, &scenario, &[]).expect("load");
    let run = umbra_engine::instantiate_with_plugins(&loaded, m7_support::run_options(None))
        .expect("instantiate");
    let (mut a, _) = Peer::new(run);
    let file = scratch("inbound");
    let mut opts = RecordOptions::new(&file);
    opts.hash_every = Some(1);
    a.engine
        .attach_recorder(Recorder::create(opts, &scenario, &loaded, None).expect("recorder"));
    let (mut b, _) = Peer::new(net_support::start("b", &b_registry, None));

    for _ in 0..40 {
        b.tick();
        a.tick();
    }
    a.engine.finish_recording("completed").expect("finish");

    let (header, records) = read_all(&file);
    assert_eq!(
        header.network_adapters,
        [AdapterRef {
            name: "lb".to_owned(),
            adapter: "test.net.loopback".to_owned(),
            key_space: KeySpace {
                base: 1000,
                count: 100
            },
        }]
    );
    let inbound: Vec<_> = records
        .iter()
        .filter_map(|r| match r {
            Record::Inbound(i) => Some(i),
            _ => None,
        })
        .collect();
    assert!(!inbound.is_empty(), "the peer's entities arrived");
    assert!(
        inbound
            .iter()
            .all(|i| i.adapter == "lb" && !i.messages.is_empty())
    );
    assert!(
        inbound.windows(2).all(|w| w[0].tick <= w[1].tick),
        "tick order"
    );
    let seen = records.iter().filter_map(|r| match r {
        Record::Hash { tick, .. } => Some(*tick),
        _ => None,
    });
    assert_eq!(seen.count(), 40, "hash_every = 1");
}

/// ASM-622, REQ-X: a recording cannot be created in a directory that does not exist; the error is
/// reported, nothing is attached.
#[test]
#[ignore = "M7 WI-13"]
fn given_an_unwritable_path_when_a_recorder_is_created_then_it_fails_with_io() {
    let registry = launcher_registry();
    let scenario = example("walking_skeleton");
    let loaded = load_scenario_with_plugins(&registry, &scenario, &[]).expect("load");
    let bad = scratch("missing").with_file_name("no-such-dir/rec.umbrarec.jsonl");

    let result = Recorder::create(RecordOptions::new(bad), &scenario, &loaded, None);

    assert!(matches!(result, Err(umbra_record::RecordError::Io(_))));
}

/// ADR 0053, A-2: the header lists a loaded plugin with its id, version, ABI and the digest of the
/// library bytes.
#[test]
#[ignore = "M7 WI-13"]
fn header_lists_plugins_with_library_digests() {
    let _guard = plugin_lock();
    let plugins = plugin_support::wander();
    let scenario = example("plugin_wander");

    let recorded = record(
        &launcher_registry(),
        &scenario,
        std::slice::from_ref(&plugins),
        Some(30),
        None,
    );

    let (header, _) = read_all(&recorded.file);
    assert_eq!(header.plugins.len(), 1);
    let plugin = &header.plugins[0];
    assert_eq!(plugin.id, "example.plugin.wander");
    let library = std::fs::read_dir(plugins.join("wander"))
        .expect("package")
        .map(|e| e.expect("entry").path())
        .find(|p| {
            p.extension()
                .is_some_and(|e| e == std::env::consts::DLL_EXTENSION)
        })
        .expect("the library");
    assert_eq!(
        plugin.library_fnv1a64,
        fnv1a64(&std::fs::read(library).expect("read"))
    );
}

/// ADR 0053 status: a live recorder reports `Active` with a growing record count and the last tick,
/// and `Finished` after `finish_recording`.
#[test]
#[ignore = "M7 WI-13"]
fn the_recorder_reports_its_status_while_active_and_after_finishing() {
    let registry = launcher_registry();
    let scenario = fixture("spawn_basic");
    let loaded = load_scenario_with_plugins(&registry, &scenario, &[]).expect("load");
    let mut engine = umbra_engine::instantiate_with_plugins(&loaded, m7_support::run_options(None))
        .expect("instantiate")
        .engine;
    let file = scratch("status");
    engine.attach_recorder(
        Recorder::create(RecordOptions::new(&file), &scenario, &loaded, None).expect("recorder"),
    );

    engine
        .step(10, |_| core::ops::ControlFlow::Continue(()))
        .expect("step");
    let active = engine.recording().expect("a recorder is attached");
    let finished = engine
        .finish_recording("completed")
        .expect("finish")
        .expect("a status");

    assert_eq!(active.state, umbra_engine::RecordingState::Active);
    assert_eq!(active.file, file);
    assert!(active.records > 0 && active.last_tick <= 10);
    assert_eq!(finished.state, umbra_engine::RecordingState::Finished);
    assert_eq!(finished.last_tick, 10);
}
