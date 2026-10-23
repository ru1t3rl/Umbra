//! AT-11 (M7): the engine recorder. Requirements: REQ-K-7, REQ-K-4, REQ-K-6, REQ-K-8, ADR 0053,
//! ASM-621..627, ASM-633.
//!
//! The header carries the effective seed, tick rate and source, the file digests, the plugins and
//! the adapter key spaces; commands carry `before_tick` and `seq`; inbound messages are recorded as
//! polled; hashes follow the cadence; recording leaves every per-tick hash unchanged. Written in
//! M7 WI-1 against the recorder stub: ignored until M7 WI-13 (rows that need a plugin or a peer
//! name the same WI). The rows after the WI-1 ones are WI-13's own: fault records, forced hashes,
//! a write failure, and one file per load with `record_dir`.

#![forbid(unsafe_code)]
#![expect(clippy::panic, reason = "tests fail by panicking")]

mod common;
mod fault_adapter;
mod m7_support;
mod net_support;
mod plugin_support;

use core::time::Duration;
use std::path::Path;

use common::{options, run_to};
use fault_adapter::with_fault_adapter;
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
fn header_holds_effective_seed_rate_and_digests() {
    let scenario = example("walking_skeleton");

    let recorded = record(&launcher_registry(), &scenario, &[], Some(TICKS), Some(99));

    let (header, _) = read_all(&recorded.file);
    assert_eq!(header.seed, 99);
    assert_eq!(header.tick_rate_hz, 30);
    assert_eq!(header.tick_rate_source, "scenario");
    assert_eq!(header.state_hash_version, 2);
    assert_eq!(header.hash_every, 1);
    assert_eq!(
        header.on_plugin_fault, "abort",
        "the default policy (ADR 0021)"
    );
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
    engine
        .attach_recorder(
            Recorder::create(opts, &example("walking_skeleton"), &loaded, None).expect("recorder"),
        )
        .expect("attach");
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

    let files: Vec<_> = std::fs::read_dir(dir.join("serve-1"))
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
    assert!(
        matches!(
            commands.last().map(|c| &c.command),
            Some(RecordedCommand::Stop)
        ),
        "the stop command is the last command"
    );
    match records.last() {
        Some(Record::End(end)) => assert_eq!(end.reason, "stopped"),
        other => panic!("the file ends with {other:?}"),
    }
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
        .attach_recorder(Recorder::create(opts, &scenario, &loaded, None).expect("recorder"))
        .expect("attach");
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
fn the_recorder_reports_its_status_while_active_and_after_finishing() {
    let registry = launcher_registry();
    let scenario = fixture("spawn_basic");
    let loaded = load_scenario_with_plugins(&registry, &scenario, &[]).expect("load");
    let mut engine = umbra_engine::instantiate_with_plugins(&loaded, m7_support::run_options(None))
        .expect("instantiate")
        .engine;
    let file = scratch("status");
    let mut opts = RecordOptions::new(&file);
    opts.hash_every = Some(5);
    engine
        .attach_recorder(Recorder::create(opts, &scenario, &loaded, None).expect("recorder"))
        .expect("attach");

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
    assert_eq!(
        (active.records, active.last_tick),
        (2, 10),
        "the hashes of ticks 5 and 10; the header is not a record (ASM-733)"
    );
    assert_eq!(finished.state, umbra_engine::RecordingState::Finished);
    assert_eq!(finished.last_tick, 10);
}

/// A recorded run of the `net_pair` fixture `dir` on the failing adapter; returns the fault records.
fn faults_of(dir: &str, ticks: u64, policy: &str) -> Vec<umbra_record::FaultRecord> {
    let registry = with_fault_adapter(launcher_registry());
    let scenario = net_support::fixture(dir);
    let loaded = load_scenario_with_plugins(&registry, &scenario, &[])
        .unwrap_or_else(|e| panic!("load: {e}"));
    let mut engine = umbra_engine::instantiate_with_plugins(&loaded, m7_support::run_options(None))
        .unwrap_or_else(|e| panic!("instantiate: {e}"))
        .engine;
    let file = scratch("fault");
    let mut opts = RecordOptions::new(&file);
    opts.hash_every = Some(1);
    engine
        .attach_recorder(
            Recorder::create(opts, &scenario, &loaded, None)
                .unwrap_or_else(|e| panic!("recorder: {e}")),
        )
        .unwrap_or_else(|e| panic!("attach: {e}"));

    // Under `abort` the step ends with the network fault; the recorder must have seen it.
    let _ = engine.step(ticks, |_| core::ops::ControlFlow::Continue(()));
    engine
        .finish_recording("network_fault")
        .unwrap_or_else(|e| panic!("finish: {e}"));

    let (header, records) = read_all(&file);
    assert_eq!(header.on_plugin_fault, policy);
    assert_eq!(header.network_adapters.len(), 1);
    assert_eq!(header.network_adapters[0].adapter, "test.net.fault");
    assert_eq!(
        header.network_adapters[0].key_space,
        KeySpace {
            base: 10,
            count: 10
        }
    );
    records
        .into_iter()
        .filter_map(|r| match r {
            Record::Fault(f) => Some(f),
            _ => None,
        })
        .collect()
}

/// ASM-731: an adapter whose poll fails is recorded once with the call `poll`.
#[test]
fn a_poll_fault_is_recorded_with_the_call_poll() {
    let faults = faults_of("fault_abort_poll", 20, "abort");

    assert_eq!(faults.len(), 1, "{faults:?}");
    assert_eq!(
        (faults[0].adapter.as_str(), faults[0].call.as_str()),
        ("bad", "poll")
    );
    assert!(faults[0].tick <= 1, "the first tick: {}", faults[0].tick);
}

/// ASM-731: a failing publish is recorded once with the call `publish`; the disabled adapter is
/// silent afterwards.
#[test]
fn a_publish_fault_is_recorded_once_with_the_call_publish() {
    let faults = faults_of("fault_disable_publish", 20, "disable_capability");

    assert_eq!(faults.len(), 1, "{faults:?}");
    assert_eq!(
        (faults[0].adapter.as_str(), faults[0].call.as_str()),
        ("bad", "publish")
    );
}

/// ASM-624, ASM-626: a command forces a hash on the next tick whatever the cadence; informational
/// control commands are recorded where the engine publishes them (`before_tick` 0, `seq` 0 first).
#[test]
fn a_command_forces_a_hash_on_the_next_tick() {
    let registry = launcher_registry();
    let loaded =
        load_scenario_with_plugins(&registry, &example("walking_skeleton"), &[]).expect("load");
    let mut engine = umbra_engine::instantiate_with_plugins(&loaded, m7_support::run_options(None))
        .expect("instantiate")
        .engine;
    let file = scratch("forced");
    let mut opts = RecordOptions::new(&file);
    opts.hash_every = Some(1000);
    engine
        .attach_recorder(
            Recorder::create(opts, &example("walking_skeleton"), &loaded, None).expect("recorder"),
        )
        .expect("attach");

    engine.publish_control(umbra_engine::ControlAction::Start);
    engine
        .run(umbra_engine::RunUntil::Ticks(3), |_| {
            core::ops::ControlFlow::Continue(())
        })
        .expect("run");
    engine.finish_recording("completed").expect("finish");

    let (_, records) = read_all(&file);
    assert_eq!(
        hash_ticks(&records),
        [1, 3],
        "forced after the command, then final"
    );
    match &records[0] {
        Record::Command(c) => {
            assert_eq!((c.before_tick, c.seq), (0, 0));
            assert!(matches!(c.command, RecordedCommand::Start));
        }
        other => panic!("the first record is {other:?}"),
    }
}

/// ASM-625: a failing sink never stops the simulation or changes its hashes; the recording is
/// `Failed` and `finish_recording` says so (Linux: `/dev/full` opens and fails every write).
#[cfg(target_os = "linux")]
#[test]
fn a_write_failure_marks_the_recording_failed_and_the_run_goes_on() {
    let registry = launcher_registry();
    let scenario = example("walking_skeleton");
    let plain = record(&registry, &scenario, &[], Some(50), None);
    let loaded = load_scenario_with_plugins(&registry, &scenario, &[]).expect("load");
    let mut engine = umbra_engine::instantiate_with_plugins(&loaded, m7_support::run_options(None))
        .expect("instantiate")
        .engine;
    let mut opts = RecordOptions::new("/dev/full");
    opts.hash_every = Some(1);
    engine
        .attach_recorder(Recorder::create(opts, &scenario, &loaded, None).expect("recorder"))
        .expect("attach");
    let mut hashes = Vec::new();

    engine
        .run(umbra_engine::RunUntil::Ticks(50), |v| {
            hashes.push(v.state_hash());
            core::ops::ControlFlow::Continue(())
        })
        .expect("the run is not stopped by the write failure");
    let finished = engine.finish_recording("completed");

    assert_eq!(hashes, plain.hashes, "the simulation is unchanged");
    assert!(
        matches!(finished, Err(umbra_record::RecordError::Failed(_))),
        "{finished:?}"
    );
    let status = engine.recording().expect("a status");
    assert_eq!(status.state, umbra_engine::RecordingState::Failed);
}

/// ASM-632: `record_dir` writes one file per session load, named `session-<n>.umbrarec.jsonl`; a
/// session replaced by the next load is closed with an `end` record.
#[test]
fn record_dir_writes_one_file_per_load() {
    let dir = scratch("perload").with_extension("d");
    std::fs::create_dir_all(&dir).expect("record dir");
    let mut config = HandleConfig::new(launcher_registry());
    config.wall = std::sync::Arc::new(|| -> Box<dyn umbra_engine::WallClock> {
        Box::new(umbra_engine::ManualWallClock::with_auto_advance(
            Duration::from_micros(500),
        ))
    });
    config.record_dir = Some(dir.clone());
    let (handle, _thread) = EngineHandle::spawn(config).expect("spawn");

    handle
        .load(fixture("spawn_basic"), Some(1), Some(ClockMode::Stepped))
        .expect("first load");
    handle
        .load(fixture("spawn_basic"), Some(2), Some(ClockMode::Stepped))
        .expect("second load");
    handle.unload().expect("unload");

    let dir = dir.join("serve-1");
    let mut names: Vec<_> = std::fs::read_dir(&dir)
        .expect("dir")
        .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(
        names,
        ["session-1.umbrarec.jsonl", "session-2.umbrarec.jsonl"]
    );
    for (name, seed) in [
        ("session-1.umbrarec.jsonl", 1),
        ("session-2.umbrarec.jsonl", 2),
    ] {
        let (header, records) = read_all(&dir.join(name));
        assert_eq!(header.seed, seed, "{name}");
        match records.last() {
            Some(Record::End(end)) => assert_eq!(end.reason, "unloaded", "{name}"),
            other => panic!("{name} ends with {other:?}"),
        }
    }
}

/// ASM-735, ASM-732: a clock change is recorded as the informational `set_clock` with the `--clock`
/// spelling; finishing twice gives the same outcome and writes one `end`.
#[test]
fn a_clock_change_is_recorded_and_finishing_is_idempotent() {
    let registry = launcher_registry();
    let scenario = example("walking_skeleton");
    let loaded = load_scenario_with_plugins(&registry, &scenario, &[]).expect("load");
    let mut engine = umbra_engine::instantiate_with_plugins(&loaded, m7_support::run_options(None))
        .expect("instantiate")
        .engine;
    let file = scratch("clock");
    engine
        .attach_recorder(
            Recorder::create(RecordOptions::new(&file), &scenario, &loaded, None)
                .expect("recorder"),
        )
        .expect("attach");

    engine.set_clock(ClockMode::Stepped);
    let first = engine.finish_recording("stopped").expect("finish");
    let second = engine.finish_recording("completed").expect("finish again");

    assert_eq!(first, second);
    let (_, records) = read_all(&file);
    assert!(
        records.iter().any(|r| matches!(r, Record::Command(c)
            if matches!(&c.command, RecordedCommand::SetClock(mode) if mode == "stepped"))),
        "{records:?}"
    );
    let ends: Vec<_> = records
        .iter()
        .filter_map(|r| match r {
            Record::End(e) => Some(e.reason.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(ends, ["stopped"]);
}

/// A handle over `record_dir` on manual wall clocks.
fn recording_handle(dir: &Path) -> (EngineHandle, umbra_engine::handle::EngineThread) {
    let mut config = HandleConfig::new(launcher_registry());
    config.wall = std::sync::Arc::new(|| -> Box<dyn umbra_engine::WallClock> {
        Box::new(umbra_engine::ManualWallClock::with_auto_advance(
            Duration::from_micros(500),
        ))
    });
    config.record_dir = Some(dir.to_path_buf());
    EngineHandle::spawn(config).unwrap_or_else(|e| panic!("spawn: {e}"))
}

fn names_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<_> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|e| {
            e.unwrap_or_else(|e| panic!("entry: {e}"))
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

/// ASM-736: a second handle on one record directory never touches the first one's files; it writes
/// to the next free `serve-<k>` subdirectory (a restarted `serve` must not overwrite recordings).
#[test]
fn a_second_handle_on_one_record_dir_keeps_the_first_files() {
    let dir = scratch("twohandles").with_extension("d");
    std::fs::create_dir_all(&dir).expect("record dir");
    let (first, _t1) = recording_handle(&dir);
    first
        .load(fixture("spawn_basic"), Some(1), Some(ClockMode::Stepped))
        .expect("load");
    first.unload().expect("unload");
    let kept = std::fs::read(dir.join("serve-1/session-1.umbrarec.jsonl")).expect("first file");

    let (second, _t2) = recording_handle(&dir);
    second
        .load(fixture("spawn_basic"), Some(2), Some(ClockMode::Stepped))
        .expect("load");
    second.unload().expect("unload");

    assert_eq!(names_in(&dir), ["serve-1", "serve-2"]);
    assert_eq!(
        std::fs::read(dir.join("serve-1/session-1.umbrarec.jsonl")).expect("again"),
        kept,
        "the first handle's recording is intact"
    );
    let (header, _) = read_all(&dir.join("serve-2/session-1.umbrarec.jsonl"));
    assert_eq!(header.seed, 2);
}

/// ADR 0057: a recording that cannot start does not fail the load; the session carries a recorder
/// that is `Failed` from the start, naming the intended file.
#[test]
fn a_record_dir_that_vanished_fails_the_recording_not_the_load() {
    let dir = scratch("vanished").with_extension("d");
    std::fs::create_dir_all(&dir).expect("record dir");
    let (handle, _thread) = recording_handle(&dir);
    std::fs::remove_dir_all(&dir).expect("remove the record directory");

    let loaded = handle.load(fixture("spawn_basic"), Some(1), Some(ClockMode::Stepped));

    assert!(loaded.is_ok(), "{loaded:?}");
    handle.unload().expect("unload");
}

/// ADR 0057: `Session::start_recording` that fails leaves a `Failed` recorder behind, so a status
/// shows `failed` rather than nothing; a recorder is attached only once.
#[test]
fn a_recording_that_cannot_start_is_failed_in_the_status() {
    let registry = launcher_registry();
    let scenario = fixture("spawn_basic");
    let mut session = umbra_engine::Session::load(
        &registry,
        &scenario,
        umbra_engine::SessionOptions::new(Box::new(umbra_engine::ManualWallClock::new())),
    )
    .expect("session");
    let missing = scratch("nodir").with_file_name("no-such-dir/rec.umbrarec.jsonl");

    let started = session.start_recording(RecordOptions::new(&missing), None);

    assert!(started.is_err());
    let status = session.engine().recording().expect("a failed recorder");
    assert_eq!(status.state, umbra_engine::RecordingState::Failed);
    assert_eq!(status.file, missing);
    let again = session.start_recording(RecordOptions::new(scratch("again")), None);
    assert!(again.is_err(), "no second recorder replaces the failed one");
}

/// Nit: attaching a second recorder is refused, and the first keeps recording.
#[test]
fn a_second_recorder_is_refused() {
    let registry = launcher_registry();
    let scenario = example("walking_skeleton");
    let loaded = load_scenario_with_plugins(&registry, &scenario, &[]).expect("load");
    let mut engine = umbra_engine::instantiate_with_plugins(&loaded, m7_support::run_options(None))
        .expect("instantiate")
        .engine;
    let (first, second) = (scratch("first"), scratch("second"));
    engine
        .attach_recorder(
            Recorder::create(RecordOptions::new(&first), &scenario, &loaded, None).expect("a"),
        )
        .expect("attach");

    let refused = engine.attach_recorder(
        Recorder::create(RecordOptions::new(&second), &scenario, &loaded, None).expect("b"),
    );

    assert!(refused.is_err());
    assert_eq!(engine.recording().expect("status").file, first);
}

/// Copies the directory `from` to `to`.
fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap_or_else(|e| panic!("mkdir: {e}"));
    for entry in std::fs::read_dir(from).unwrap_or_else(|e| panic!("read_dir: {e}")) {
        let entry = entry.unwrap_or_else(|e| panic!("entry: {e}"));
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap_or_else(|e| panic!("copy: {e}"));
        }
    }
}

/// Minor 3: a recording path that is one of the run's inputs is refused before anything is
/// truncated (on a copy of the fixtures, never the repository's files).
#[test]
fn a_recording_path_that_is_an_input_is_refused_and_the_input_kept() {
    let copy = scratch("inputs").with_extension("d");
    copy_tree(
        fixture("spawn_basic")
            .parent()
            .expect("dir")
            .parent()
            .expect("m7"),
        &copy,
    );
    let scenario = copy.join("spawn_basic/scenario.toml");
    let before = std::fs::read(&scenario).expect("read");
    let loaded = load_scenario_with_plugins(&launcher_registry(), &scenario, &[]).expect("load");

    let created = Recorder::create(RecordOptions::new(&scenario), &scenario, &loaded, None);

    assert!(
        matches!(created, Err(umbra_record::RecordError::Io(_))),
        "{created:?}"
    );
    assert_eq!(std::fs::read(&scenario).expect("read again"), before);
}

/// Minor 3: a header that cannot be built (an unreadable scenario file) leaves an existing
/// recording file alone: the file is opened last.
#[test]
fn a_failed_header_leaves_an_existing_file_alone() {
    let loaded = load_scenario_with_plugins(&launcher_registry(), &fixture("spawn_basic"), &[])
        .expect("load");
    let file = scratch("existing");
    std::fs::write(&file, "keep me").expect("write");

    let created = Recorder::create(
        RecordOptions::new(&file),
        &file.with_file_name("no-such-scenario.toml"),
        &loaded,
        None,
    );

    assert!(created.is_err());
    assert_eq!(std::fs::read_to_string(&file).expect("read"), "keep me");
}

/// ASM-732: a handle run to its duration closes the recording with `completed`.
#[test]
fn a_run_to_its_duration_ends_with_completed() {
    let dir = scratch("duration").with_extension("d");
    std::fs::create_dir_all(&dir).expect("record dir");
    let (handle, _thread) = recording_handle(&dir);
    handle
        .load(fixture("spawn_basic"), Some(1), Some(ClockMode::Stepped))
        .expect("load");
    handle.start().expect("start");

    handle.step(300).expect("step to the duration");

    let (_, records) = read_all(&dir.join("serve-1/session-1.umbrarec.jsonl"));
    match records.last() {
        Some(Record::End(end)) => assert_eq!((end.reason.as_str(), end.tick), ("completed", 300)),
        other => panic!("the file ends with {other:?}"),
    }
}

/// ASM-735: a refused command changed nothing and is not recorded; an accepted one is.
#[test]
fn refused_commands_are_not_recorded() {
    let dir = scratch("refused").with_extension("d");
    std::fs::create_dir_all(&dir).expect("record dir");
    let (handle, _thread) = recording_handle(&dir);
    handle
        .load(fixture("spawn_basic"), Some(1), Some(ClockMode::Stepped))
        .expect("load");
    let at = Position::Geodetic(Geodetic::from_degrees(52.001, 4.001, 0.0).expect("position"));
    handle
        .spawn_entity(TemplateSpawn::new(
            TemplateId::new("m7.child").expect("id"),
            at,
        ))
        .expect("accepted spawn");

    let unknown = umbra_engine::EntityId::new(9999).expect("id");
    assert!(handle.remove_entity(unknown).is_err());
    assert!(handle.set_adapter_enabled("nope".to_owned(), true).is_err());
    assert!(
        handle
            .spawn_entity(TemplateSpawn::new(
                TemplateId::new("no.such").expect("id"),
                at
            ))
            .is_err()
    );
    handle.unload().expect("unload");

    let (_, records) = read_all(&dir.join("serve-1/session-1.umbrarec.jsonl"));
    let kinds: Vec<_> = records
        .iter()
        .filter_map(|r| match r {
            Record::Command(c) => Some(&c.command),
            _ => None,
        })
        .collect();
    assert_eq!(kinds.len(), 1, "{kinds:?}");
    assert!(matches!(kinds[0], RecordedCommand::Spawn { .. }));
}
