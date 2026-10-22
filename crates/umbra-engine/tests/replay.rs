//! AT-7, AT-8, AT-10 (M7): replay reproduces state hashes, handle-driven runs replay, and replay
//! refuses what it cannot reproduce. Requirements: REQ-K-7, REQ-K-4, REQ-K-5, ADR 0054,
//! ASM-628..631.
//!
//! AT-7: each reference scenario is recorded with `hash_every = 1`, replayed, and every replayed
//! per-tick hash equals the live run's; the final hash equals the golden of `scripts/gates.sh`.
//! AT-8: a run driven through `EngineHandle` (spawn, remove and adapter toggles between ticks, one
//! of them while running as fast as possible, so its tick is timing-dependent) replays to the same
//! hashes. AT-10: changed files, another plugin version, a truncated or newer recording are
//! refused; a changed hash reports the first divergent tick.
//!
//! Written in M7 WI-1 against the replay stub: ignored until M7 WI-14 (the air/sea row also needs
//! the example from WI-6, the plugin-kinds row WI-21).

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]
#![expect(clippy::panic, reason = "tests fail by panicking")]

mod m7_support;
mod net_support;
mod plugin_support;

use core::time::Duration;
use std::path::{Path, PathBuf};

use m7_support::{
    example, first_divergence, fixture, goldens, launcher_registry, plugin_lock, record,
    replay_file, scratch, with_launcher,
};
use net_support::registry_with;
use umbra_engine::handle::{EngineHandle, HandleConfig};
use umbra_engine::{
    ClockMode, Geodetic, Position, ReplayError, ReplayOptions, TemplateId, TemplateSpawn, replay,
};
use umbra_net::LoopbackHub;
use umbra_record::{Reader, Record, RecordError};

/// Records `name` for its whole duration, replays it, and checks tick by tick and against the
/// golden of `scripts/gates.sh` when the scenario has one.
fn replays_to_the_golden(name: &str, plugins: &[PathBuf]) {
    let registry = launcher_registry();
    let live = record(&registry, &example(name), plugins, None, None);

    let (replayed, summary) = replay_file(&registry, &live.file, plugins)
        .unwrap_or_else(|e| panic!("{name}: replay failed: {e}"));

    assert_eq!(
        first_divergence(&replayed, &live.hashes),
        None,
        "{name}: first divergent tick"
    );
    assert_eq!(summary.ticks, live.hashes.len() as u64, "{name}");
    assert_eq!(
        summary.hashes_verified, summary.ticks,
        "{name}: hash_every = 1"
    );
    if let Some(golden) = goldens().get(name) {
        assert_eq!(&summary.final_hash.to_string(), golden, "{name}");
    }
}

/// AT-7: `walking_skeleton`.
#[test]
#[ignore = "M7 WI-14"]
fn replay_reproduces_every_tick_hash_of_walking_skeleton() {
    replays_to_the_golden("walking_skeleton", &[]);
}

/// AT-7: `patrol_and_engage`.
#[test]
#[ignore = "M7 WI-14"]
fn replay_reproduces_every_tick_hash_of_patrol_and_engage() {
    replays_to_the_golden("patrol_and_engage", &[]);
}

/// AT-7: `plugin_wander`, with the real example plugin.
#[test]
#[ignore = "M7 WI-14"]
fn replay_reproduces_every_tick_hash_of_plugin_wander() {
    let _guard = plugin_lock();
    replays_to_the_golden("plugin_wander", &[plugin_support::wander()]);
}

// AT-7 `plugin_c_circle`: not an engine-test row. Compiling the C example needs the `cc` crate (a
// dev-dependency of `umbra-plugin-host` and xtask only), so its replay is checked at the CLI/gate
// level in WI-14 (ASM-653, ASM-178 precedent).

/// AT-7: the M7 air and sea example (not frozen: its golden is recorded in the plan's running log).
#[test]
#[ignore = "M7 WI-14"]
fn replay_reproduces_every_tick_hash_of_air_sea() {
    replays_to_the_golden("air_sea", &[]);
}

/// AT-7: the combined plugin-kinds example (WI-21), with the five example plugins built by the
/// test (`plugin_support`, honours `CARGO_TARGET_DIR`).
#[test]
#[ignore = "M7 WI-21"]
fn replay_reproduces_every_tick_hash_of_plugin_kinds() {
    let _guard = plugin_lock();
    let packages = ["hover", "radar", "bearing", "dispatch", "net-jsonl"];
    let libraries: Vec<(String, PathBuf)> = packages
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
    replays_to_the_golden("plugin_kinds", &[plugin_support::plugins_dir(&refs)]);
}

/// AT-7 with spawns: the spawning fixture replays too (the spawner runs again on replay).
#[test]
#[ignore = "M7 WI-14"]
fn replay_reproduces_every_tick_hash_of_a_spawning_scenario() {
    let registry = launcher_registry();
    let live = record(&registry, &fixture("spawn_order"), &[], Some(200), None);

    let (replayed, _) = replay_file(&registry, &live.file, &[]).expect("replay");

    assert_eq!(first_divergence(&replayed, &live.hashes), None);
    assert_eq!(
        live.entity_counts[0], 6,
        "the recording includes the spawns"
    );
}

/// AT-8, ASM-626: a handle-driven run with a spawn, a removal and adapter toggles at stepped ticks
/// and one spawn while running as fast as possible replays to the recorded hashes, and the replayed
/// spawn gets the recorded id at the recorded tick.
#[test]
#[ignore = "M7 WI-14"]
fn replay_reproduces_a_handle_run_with_spawns_removes_and_adapter_toggles() {
    let dir = scratch("handle").with_extension("d");
    std::fs::create_dir_all(&dir).expect("record dir");
    let hub = LoopbackHub::new(64);
    let registry = with_launcher(registry_with(&[("test.net.loopback", &hub, 1000)]));
    let mut config = HandleConfig::new(registry.clone());
    config.wall = std::sync::Arc::new(|| -> Box<dyn umbra_engine::WallClock> {
        Box::new(umbra_engine::ManualWallClock::with_auto_advance(
            Duration::from_micros(500),
        ))
    });
    config.record_dir = Some(dir.clone());
    let (handle, _thread) = EngineHandle::spawn(config).expect("spawn");
    let child = |lat: f64| {
        TemplateSpawn::new(
            TemplateId::new("m7.child").expect("template"),
            Position::Geodetic(Geodetic::from_degrees(lat, 4.002, 0.0).expect("position")),
        )
    };
    handle
        .load(fixture("handle_run"), Some(5), Some(ClockMode::Stepped))
        .expect("load");
    handle.start().expect("start");

    handle.step(37).expect("step");
    let first = handle.spawn_entity(child(52.001)).expect("spawn");
    handle.step(11).expect("step");
    handle
        .set_adapter_enabled("lb".to_owned(), false)
        .expect("disable");
    handle.step(20).expect("step");
    handle.remove_entity(first.snapshot.id).expect("remove");
    handle
        .set_adapter_enabled("lb".to_owned(), true)
        .expect("enable");
    handle
        .set_clock(ClockMode::AsFastAsPossible)
        .expect("clock");
    // `set_clock` to a free-running mode leaves the run Running; no `resume` is needed.
    let second = handle
        .spawn_entity(child(52.002))
        .expect("spawn while running");
    let paused = handle.pause().expect("pause");
    handle.stop().expect("stop");
    handle.unload().expect("unload");

    let file = std::fs::read_dir(&dir)
        .expect("dir")
        .map(|e| e.expect("entry").path())
        .next()
        .expect("one recording");
    let (_, records) = {
        let (h, r) = Reader::open(&file).expect("open");
        (h, r.collect::<Result<Vec<_>, _>>().expect("records"))
    };
    let spawn_ticks: Vec<u64> = records
        .iter()
        .filter_map(|r| match r {
            Record::Command(c)
                if matches!(c.command, umbra_record::RecordedCommand::Spawn { .. }) =>
            {
                Some(c.before_tick)
            }
            _ => None,
        })
        .collect();
    assert_eq!(spawn_ticks[0], 37);
    assert!(
        spawn_ticks[1] >= 68 && Some(spawn_ticks[1]) <= paused.time.map(umbra_core::SimTime::tick)
    );
    assert_ne!(first.snapshot.id, second.snapshot.id);

    let (first_id, second_id, second_tick) =
        (first.snapshot.id, second.snapshot.id, spawn_ticks[1]);
    let mut seen = Vec::new();
    let summary = replay(&registry, &file, ReplayOptions::default(), |view| {
        let ids: Vec<_> = view.snapshot().entities.iter().map(|e| e.id).collect();
        seen.push((ids.contains(&first_id), ids.contains(&second_id)));
    })
    .unwrap_or_else(|e| panic!("replay: {e}"));

    // `seen[n - 1]` is the state after tick n. A command recorded `before_tick = t` was applied
    // after tick t and before tick t + 1: the spawned entity is first seen in `seen[t]` (the state
    // after tick t + 1), the removed one is last seen in `seen[t - 1]`.
    assert!(
        !seen[36].0 && seen[37].0,
        "the first spawn is first seen after tick 38"
    );
    assert!(
        seen[37..68].iter().all(|s| s.0),
        "and stays until its removal"
    );
    assert!(
        seen[68..].iter().all(|s| !s.0),
        "the removal before tick 69 holds"
    );
    let at = usize::try_from(second_tick).expect("small");
    assert!(
        !seen[at - 1].1 && seen[at].1,
        "the second spawn is first seen after tick {}",
        at + 1
    );
    assert_eq!(
        Some(summary.ticks),
        paused.time.map(umbra_core::SimTime::tick)
    );
    assert!(summary.hashes_verified > 0);
    assert_eq!(summary.replayed_adapters, ["lb".to_owned()]);
}

// --- AT-10: refusals ---------------------------------------------------------------------------

/// A copy of `fixtures/m7` under the target tmp dir, so a test may edit the files a recording
/// digests.
fn scratch_fixtures(tag: &str) -> PathBuf {
    let to = scratch(tag).with_extension("fixtures");
    plugin_support::copy_dir(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/m7"),
        &to,
    );
    to
}

fn recorded_spawn_basic() -> (umbra_engine::Registry, PathBuf, PathBuf) {
    let root = scratch_fixtures("refuse");
    let registry = launcher_registry();
    let live = record(
        &registry,
        &root.join("spawn_basic/scenario.toml"),
        &[],
        Some(60),
        None,
    );
    (registry, live.file, root)
}

/// ASM-629: a changed template file (the digest differs) is refused, naming the file.
#[test]
#[ignore = "M7 WI-14"]
fn changed_template_is_refused() {
    let (registry, file, root) = recorded_spawn_basic();
    let template = root.join("templates/child.toml");
    let text = std::fs::read_to_string(&template).expect("read");
    std::fs::write(&template, format!("{text}# edited\n")).expect("write");

    let result = replay_file(&registry, &file, &[]);

    match result {
        Err(ReplayError::Mismatch { what }) => assert!(what.contains("child.toml"), "{what}"),
        other => panic!("expected a mismatch, got {:?}", other.map(|(_, s)| s)),
    }
}

/// ASM-629: `allow_mismatch` goes on past the digest check (and still verifies hashes).
#[test]
#[ignore = "M7 WI-14"]
fn allow_mismatch_replays_past_a_changed_file() {
    let (registry, file, root) = recorded_spawn_basic();
    let template = root.join("templates/child.toml");
    let text = std::fs::read_to_string(&template).expect("read");
    std::fs::write(&template, format!("{text}# edited\n")).expect("write");
    let mut options = ReplayOptions::default();
    options.allow_mismatch = true;

    let result = replay(&registry, &file, options, |_| {});

    assert!(
        !matches!(result, Err(ReplayError::Mismatch { .. })),
        "the digest mismatch is not an error with allow_mismatch: {result:?}"
    );
}

/// ASM-629, A-2: a plugin version that differs from the recorded one is refused.
#[test]
#[ignore = "M7 WI-14"]
fn other_plugin_version_is_refused() {
    let _guard = plugin_lock();
    let plugins = plugin_support::wander();
    let registry = launcher_registry();
    let live = record(
        &registry,
        &example("plugin_wander"),
        std::slice::from_ref(&plugins),
        Some(30),
        None,
    );
    let (header, _) = Reader::open(&live.file).expect("open");
    let version = header.plugins[0].version.clone();
    let text = std::fs::read_to_string(&live.file).expect("read");
    let edited = text.replacen(
        &format!("\"version\":\"{version}\""),
        "\"version\":\"9.9.9\"",
        1,
    );
    assert_ne!(
        edited, text,
        "the plugin version was not found in the header"
    );
    std::fs::write(&live.file, edited).expect("write");

    let result = replay_file(&registry, &live.file, &[plugins]);

    assert!(
        matches!(result, Err(ReplayError::Mismatch { .. })),
        "{:?}",
        result.map(|(_, s)| s)
    );
}

/// ASM-631: a recording without its `end` record (a crash) is refused, and replays up to its last
/// record with `partial`.
#[test]
#[ignore = "M7 WI-14"]
fn truncated_recording_needs_partial() {
    let (registry, file, _root) = recorded_spawn_basic();
    let text = std::fs::read_to_string(&file).expect("read");
    let lines: Vec<&str> = text.lines().collect();
    assert!(
        lines.last().is_some_and(|l| l.contains("\"end\"")),
        "the file ends with `end`"
    );
    std::fs::write(&file, format!("{}\n", lines[..lines.len() - 2].join("\n"))).expect("write");

    let refused = replay_file(&registry, &file, &[]);
    let mut options = ReplayOptions::default();
    options.partial = true;
    let partial = replay(&registry, &file, options, |_| {});

    assert!(
        matches!(refused, Err(ReplayError::Read(RecordError::Truncated))),
        "{:?}",
        refused.map(|(_, s)| s)
    );
    assert!(partial.expect("partial replay").hashes_verified > 0);
}

/// ADR 0053: a recording of a newer format version is refused.
#[test]
#[ignore = "M7 WI-14"]
fn newer_format_is_refused() {
    let (registry, file, _root) = recorded_spawn_basic();
    let text = std::fs::read_to_string(&file).expect("read");
    let edited = text.replacen("\"version\":1", "\"version\":2", 1);
    assert_ne!(
        edited, text,
        "the format version was not found in the header"
    );
    std::fs::write(&file, edited).expect("write");

    let result = replay_file(&registry, &file, &[]);

    assert!(
        matches!(
            result,
            Err(ReplayError::Read(RecordError::Version { found: 2 }))
        ),
        "{:?}",
        result.map(|(_, s)| s)
    );
}

/// Changes the value of the `hash` record of `tick` in the recording at `file` (a one-bit flip,
/// whatever the encoding of the hash is).
fn mutate_hash_at(file: &Path, tick: u64) {
    let text = std::fs::read_to_string(file).expect("read");
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    let target = lines
        .iter()
        .position(|l| l.contains("\"type\":\"hash\"") && l.contains(&format!("\"tick\":{tick}")))
        .unwrap_or_else(|| panic!("no hash record for tick {tick}"));
    let mut value: serde_json::Value = serde_json::from_str(&lines[target]).expect("json");
    value["hash"] = match &value["hash"] {
        serde_json::Value::String(s) => {
            let flipped = if s.ends_with('0') { "1" } else { "0" };
            serde_json::Value::String(format!("{}{flipped}", &s[..s.len() - 1]))
        }
        serde_json::Value::Number(n) => serde_json::Value::from(n.as_u64().expect("u64") ^ 1),
        other => panic!("unexpected hash encoding {other}"),
    };
    lines[target] = serde_json::to_string(&value).expect("json");
    std::fs::write(file, format!("{}\n", lines.join("\n"))).expect("write");
}

/// ADR 0054: a changed hash record reports the first divergent tick, with both hashes.
#[test]
#[ignore = "M7 WI-14"]
fn mutated_hash_reports_divergence_tick() {
    let (registry, file, _root) = recorded_spawn_basic();
    mutate_hash_at(&file, 40);

    let result = replay_file(&registry, &file, &[]);

    match result {
        Err(ReplayError::Diverged {
            tick,
            expected,
            got,
        }) => {
            assert_eq!(tick, 40);
            assert_ne!(expected, got);
        }
        other => panic!("expected a divergence, got {:?}", other.map(|(_, s)| s)),
    }
}

/// ADR 0054: with `verify = false` a mutated hash is not compared and no divergence is reported.
#[test]
#[ignore = "M7 WI-14"]
fn no_verify_replays_a_mutated_recording_without_comparing() {
    let (registry, file, _root) = recorded_spawn_basic();
    mutate_hash_at(&file, 40);
    let mut options = ReplayOptions::default();
    options.verify = false;

    let summary = replay(&registry, &file, options, |_| {}).expect("replay");

    assert_eq!(summary.hashes_verified, 0);
}

// --- state hash v3 (ADR 0055) ---------------------------------------------------------------------

/// ADR 0055: a recording made with `RecordOptions.state_hash = V3` names version 3 in its header, and
/// replay verifies with the header's version: it succeeds, checks every hash, and its final hash is
/// the v3 hash, not the v2 hash the live run's `state_hash()` reports.
#[test]
#[ignore = "M7 WI-15"]
fn a_v3_recording_has_version_3_in_the_header_and_replays_verified_with_v3() {
    let registry = launcher_registry();
    let wall = Box::new(umbra_engine::ManualWallClock::with_auto_advance(
        Duration::from_micros(500),
    ));
    let live = m7_support::record_with(
        &registry,
        &fixture("spawn_selfish"),
        &[],
        Some(60),
        None,
        wall,
        umbra_engine::StateHashVersion::V3,
    );

    let (header, _) = Reader::open(&live.file).expect("open");
    let (_, summary) =
        replay_file(&registry, &live.file, &[]).unwrap_or_else(|e| panic!("replay: {e}"));

    assert_eq!(header.state_hash_version, 3);
    assert_eq!(summary.hashes_verified, 60);
    assert_ne!(
        Some(&summary.final_hash),
        live.hashes.last(),
        "v3 differs from the v2 hash of the live run"
    );
}
