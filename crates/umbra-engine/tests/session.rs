//! AT-2 (`Session`, M5 WI-5): one scenario load usable without the engine handle (REQ-S-2), with
//! API spawns and removals that behave exactly like scenario placements, and the drop order of ADR
//! 0024 (review D-a). Requirements: REQ-S-2, REQ-X-1, REQ-K-4, REQ-P-10, ADR 0024, ASM-153,
//! ASM-161.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]
#![expect(clippy::panic, reason = "tests fail by panicking")]

mod plugin_support;

use core::time::Duration;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use plugin_support::{Tpl, scratch_copy, write_scenario};
use umbra_core::{
    EntityId, Geodetic, MetersPerSecond, PluginId, Position, Radians, SimEvent, TemplateId,
    TickRate,
};
use umbra_engine::builtins::standard_registry;
use umbra_engine::scenario::TickRateSource;
use umbra_engine::{
    ClockMode, ManualWallClock, PluginLog, Pumped, RunUntil, Session, SessionError, SessionOptions,
    TemplateSpawn,
};

fn walking_skeleton() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/scenarios/walking_skeleton/scenario.toml")
}

fn options() -> SessionOptions {
    SessionOptions::new(Box::new(ManualWallClock::with_auto_advance(
        Duration::from_micros(500),
    )))
}

fn load(path: &Path) -> Session {
    load_with(path, |_| {})
}

fn load_with(path: &Path, tweak: impl FnOnce(&mut SessionOptions)) -> Session {
    let registry = standard_registry().expect("the standard registry builds");
    let mut options = options();
    tweak(&mut options);
    Session::load(&registry, path, options).unwrap_or_else(|e| panic!("load: {e}"))
}

fn template(id: &str) -> TemplateId {
    TemplateId::new(id).expect("valid template id")
}

fn here(lat: f64, lon: f64) -> Position {
    Position::Geodetic(Geodetic::from_degrees(lat, lon, 0.0).expect("valid position"))
}

fn id_of(session: &Session, name: &str) -> EntityId {
    session
        .entities()
        .values()
        .find(|info| info.name.as_deref() == Some(name))
        .map_or_else(|| panic!("no entity named {name}"), |info| info.id)
}

/// Runs `ticks` ticks of an unpaused session through `pump`.
fn pump_ticks(session: &mut Session, ticks: u64) -> Vec<SimEvent> {
    let mut events = Vec::new();
    session.engine_mut().resume();
    for _ in 0..ticks {
        match session.engine_mut().pump(Duration::from_millis(10)) {
            Ok(Pumped::Ticked(report)) => events.extend(report.events),
            other => panic!("expected a tick, got {other:?}"),
        }
    }
    events
}

/// REQ-S-2, ADR 0024: a loaded session is at tick 0 and paused; its metadata is reachable.
#[test]
fn load_is_ready_at_tick_0_and_paused() {
    let mut session = load(&walking_skeleton());
    assert_eq!(session.engine().now().tick(), 0);
    assert!(session.engine().is_paused());
    assert_eq!(
        session
            .engine_mut()
            .pump(Duration::from_millis(10))
            .expect("pump"),
        Pumped::Idle,
        "a paused session does not tick"
    );
    assert_eq!(session.entities().len(), 3);
    assert_eq!(session.duration_ticks(), 240 * 30);
    assert_eq!(session.scenario().name, "walking_skeleton");
}

/// The clock override reaches the engine.
#[test]
fn options_override_clock() {
    let session = load_with(&walking_skeleton(), |o| o.clock = Some(ClockMode::Stepped));
    assert_eq!(session.engine().clock_mode(), ClockMode::Stepped);
    assert_eq!(
        load(&walking_skeleton()).engine().clock_mode(),
        ClockMode::AsFastAsPossible
    );
}

/// The scratch copy of the walking skeleton with one more placement: template `ground_vehicle.base`
/// named `placed`, no route, no overrides.
fn skeleton_with_placed() -> PathBuf {
    let dir = scratch_copy(
        walking_skeleton().parent().expect("example directory"),
        "session-placed",
    );
    let path = dir.join("scenario.toml");
    let mut text = std::fs::read_to_string(&path).expect("read");
    text.push_str(
        "\n[[entities]]\ntemplate = \"ground_vehicle.base\"\nname = \"placed\"\n\
         group = \"gamma\"\nposition = { lat_deg = 52.001, lon_deg = 4.002 }\n\
         heading_deg = 45.0\nspeed_mps = 7.0\n",
    );
    std::fs::write(&path, text).expect("write");
    path
}

fn spawn_like_placed() -> TemplateSpawn {
    let mut spec = TemplateSpawn::new(template("ground_vehicle.base"), here(52.001, 4.002));
    spec.heading = Radians::from_degrees(45.0);
    spec.speed = MetersPerSecond(7.0);
    spec.name = Some("spawned".to_owned());
    spec.group = Some("gamma".to_owned());
    spec
}

/// ASM-153, REQ-K-4: an API spawn builds exactly what a placement with the same values builds: the
/// same state at once and after 90 ticks, and no route.
#[test]
fn spawn_from_template_equals_scenario_placement() {
    let mut session = load(&skeleton_with_placed());
    let placed = id_of(&session, "placed");
    let (spawned, event) = session.spawn(spawn_like_placed()).expect("spawn");

    assert_eq!(
        event,
        SimEvent::EntityCreated {
            entity: spawned,
            template: Some(template("ground_vehicle.base")),
        }
    );
    assert!(spawned > placed, "ids follow creation order");
    let states = |session: &Session| {
        let snapshot = session.engine().snapshot();
        let find = |id| {
            snapshot
                .entities
                .iter()
                .find(|e| e.id == id)
                .expect("entity in the snapshot")
                .clone()
        };
        (find(placed), find(spawned))
    };
    let (a, b) = states(&session);
    assert_eq!(a.state, b.state, "at tick 0");
    assert_eq!(a.route_next, b.route_next);
    assert_eq!(a.ownership, b.ownership);
    assert_eq!(a.motion_model, b.motion_model);

    let start = a.state.position;
    pump_ticks(&mut session, 90);
    let (a, b) = states(&session);
    assert_ne!(a.state.position, start, "they moved");
    assert_eq!(a.state, b.state, "after 90 ticks");
}

/// An unknown template is refused and nothing is created.
#[test]
fn spawn_unknown_template() {
    let mut session = load(&walking_skeleton());
    let spec = TemplateSpawn::new(template("no.such.template"), here(52.0, 4.0));
    match session.spawn(spec) {
        Err(SessionError::TemplateNotFound(id)) => assert_eq!(id, template("no.such.template")),
        other => panic!("expected TemplateNotFound, got {other:?}"),
    }
    assert_eq!(session.entities().len(), 3);
    assert_eq!(session.engine().snapshot().entities.len(), 3);
}

/// Names are unique among named entities (ASM-059); the refusal changes nothing.
#[test]
fn spawn_duplicate_name() {
    let mut session = load(&walking_skeleton());
    let mut spec = TemplateSpawn::new(template("ground_vehicle.base"), here(52.0, 4.0));
    spec.name = Some("base-1".to_owned());
    match session.spawn(spec) {
        Err(SessionError::DuplicateName(name)) => assert_eq!(name, "base-1"),
        other => panic!("expected DuplicateName, got {other:?}"),
    }
    assert_eq!(session.engine().snapshot().entities.len(), 3);
}

/// Removing an unknown entity is refused and changes nothing.
#[test]
fn remove_unknown_entity() {
    let mut session = load(&walking_skeleton());
    let unknown = EntityId::new(999).expect("non-zero");
    match session.remove(unknown) {
        Err(SessionError::EntityNotFound(id)) => assert_eq!(id, unknown),
        other => panic!("expected EntityNotFound, got {other:?}"),
    }
    assert_eq!(session.entities().len(), 3);
}

/// The directory follows spawns and removals, and a removed name can be used again.
#[test]
fn directory_tracks_spawn_and_remove() {
    let mut session = load(&walking_skeleton());
    let (id, _) = session.spawn(spawn_like_placed()).expect("spawn");
    let info = session.entities().get(&id).expect("listed");
    assert_eq!(info.template, template("ground_vehicle.base"));
    assert_eq!(info.name.as_deref(), Some("spawned"));
    assert_eq!(info.group.as_deref(), Some("gamma"));
    assert_eq!(session.entities().len(), 4);

    let event = session.remove(id).expect("remove");
    assert_eq!(event, SimEvent::EntityRemoved { entity: id });
    assert!(!session.entities().contains_key(&id));
    assert_eq!(session.entities().len(), 3);
    assert_eq!(session.engine().snapshot().entities.len(), 3);

    let (again, _) = session
        .spawn(spawn_like_placed())
        .expect("the name is free");
    assert!(again > id, "ids are not reused (REQ-C-2)");
    let ids: Vec<EntityId> = session.entities().keys().copied().collect();
    let mut sorted = ids.clone();
    sorted.sort();
    assert_eq!(ids, sorted, "the directory is in id order");
}

/// ADR 0010 note: the session reports the tick rate and where it came from.
#[test]
fn session_reports_tick_rate_and_source() {
    let explicit = load(&walking_skeleton());
    assert_eq!(explicit.scenario().tick_rate.hz(), 30);
    assert_eq!(
        explicit.scenario().tick_rate_source,
        TickRateSource::Scenario
    );

    let dir = scratch_copy(
        walking_skeleton().parent().expect("example directory"),
        "session-default-rate",
    );
    let path = dir.join("scenario.toml");
    let text = std::fs::read_to_string(&path).expect("read");
    assert!(text.contains("tick_rate_hz = 30\n"));
    std::fs::write(&path, text.replace("tick_rate_hz = 30\n", "")).expect("write");

    let defaulted = load(&path);
    assert_eq!(defaulted.scenario().tick_rate.hz(), 60);
    assert_eq!(
        defaulted.scenario().tick_rate_source,
        TickRateSource::Default
    );
    assert_eq!(defaulted.duration_ticks(), 240 * 60);

    let configured = load_with(&path, |o| {
        o.load.default_tick_rate = TickRate::new(20).expect("rate");
    });
    assert_eq!(configured.scenario().tick_rate.hz(), 20);
    assert_eq!(
        configured.scenario().tick_rate_source,
        TickRateSource::Default
    );
    assert_eq!(configured.duration_ticks(), 240 * 20);
}

/// ADR 0028: the initial load emits no `EntityCreated`; only API spawns do, and they return it
/// from `spawn` rather than from a tick.
#[test]
fn initial_load_emits_no_entity_created() {
    let mut session = load(&walking_skeleton());
    let events = pump_ticks(&mut session, 5);
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, SimEvent::EntityCreated { .. }))
    );
    let (_, event) = session.spawn(spawn_like_placed()).expect("spawn");
    assert!(matches!(event, SimEvent::EntityCreated { .. }));
    let events = pump_ticks(&mut session, 5);
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, SimEvent::EntityCreated { .. })),
        "the spawn event is not repeated by a tick"
    );
}

/// A scenario with errors is a `SessionError::Scenario` with its diagnostics.
#[test]
fn invalid_scenario_is_a_scenario_error() {
    let registry = standard_registry().expect("registry");
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bad_scenario/scenario.toml");
    match Session::load(&registry, &path, options()) {
        Err(SessionError::Scenario(_)) => {}
        Err(other) => panic!("expected a scenario error, got {other}"),
        Ok(_) => panic!("the bad scenario loaded"),
    }
}

/// REQ-P-10, ASM-138: the plugin build scope is open around `spawn`: a template whose behavior is a
/// run-mode plugin behavior can be spawned between ticks, and it ticks.
#[test]
fn spawn_plugin_behavior_inside_build_scope() {
    let fixture = plugin_support::fixture(&[]);
    let scenario =
        plugin_support::fault_scenario("spawn-scope", "disable_capability", 2.0, &["healthy"]);
    let mut session = load_with(&scenario, |o| o.load.extra_plugin_paths = vec![fixture]);
    let mut spec = TemplateSpawn::new(template("test.entity.healthy"), here(52.01, 4.01));
    spec.name = Some("late".to_owned());
    let (id, _) = session
        .spawn(spec)
        .expect("a plugin behavior builds in the scope");
    assert_eq!(session.entities().len(), 2);
    pump_ticks(&mut session, 3);
    assert!(
        session
            .engine()
            .snapshot()
            .entities
            .iter()
            .any(|e| e.id == id)
    );
}

#[derive(Default)]
struct Recorder(Mutex<Vec<String>>);

impl PluginLog for Recorder {
    fn log(&self, _: &PluginId, _: umbra_plugin_host::LogLevel, message: &str) {
        self.0.lock().expect("log lock").push(message.to_owned());
    }
}

/// ADR 0024 (D-a): dropping a session destroys the entity instances first, then the behaviors,
/// and the plugin shuts down last.
#[test]
fn session_drop_order_destroys_instances_then_behaviors_then_shuts_down() {
    let fixture = plugin_support::fixture(&[]);
    let scenario = write_scenario(
        "drop-order",
        2.0,
        "",
        &[Tpl {
            id: "test.entity.life",
            behavior: "test.behavior.lifecycle",
            params: "",
        }],
        &[],
        &[("test.entity.life", "a"), ("test.entity.life", "b")],
    );
    let recorder = Arc::new(Recorder::default());
    let log: Arc<dyn PluginLog> = recorder.clone();
    let session = load_with(&scenario, |o| {
        o.load.extra_plugin_paths = vec![fixture];
        o.plugin_log = Some(log);
    });
    let before = recorder.0.lock().expect("log lock").len();
    assert!(before > 0, "the load logged create calls");
    drop(session);

    let all = recorder.0.lock().expect("log lock").clone();
    let after = &all[before..];
    let count = |what: &str, upto: usize| after[..upto].iter().filter(|m| *m == what).count();
    assert_eq!(count("destroy_instance", after.len()), 2, "{after:?}");
    assert_eq!(count("destroy_behavior", after.len()), 2, "{after:?}");
    assert_eq!(count("shutdown", after.len()), 1, "{after:?}");
    // Each entity has its own behavior: a behavior is never destroyed before the instance made
    // from it, and the plugin shuts down after every destroy call.
    for upto in 1..=after.len() {
        assert!(
            count("destroy_behavior", upto) <= count("destroy_instance", upto),
            "a behavior died before its instance: {after:?}"
        );
    }
    assert_eq!(
        after.last().map(String::as_str),
        Some("shutdown"),
        "{after:?}"
    );
}

#[test]
fn run_until_is_reexported() {
    // `Session::engine_mut().run` stays available for embedders (REQ-S-2).
    let mut session = load(&walking_skeleton());
    session.engine_mut().resume();
    let summary = session
        .engine_mut()
        .run(RunUntil::Ticks(3), |_| std::ops::ControlFlow::Continue(()))
        .expect("run");
    assert_eq!(summary.ticks_run, 3);
}

/// `LoadSettings::extra_plugin_paths` are searched by the plugin-aware loaders, after `extra`.
#[test]
fn plugin_aware_loaders_search_the_settings_plugin_paths() {
    let fixture = plugin_support::fixture(&[]);
    let scenario =
        plugin_support::fault_scenario("settings-paths", "disable_capability", 1.0, &["healthy"]);
    let registry = standard_registry().expect("registry");
    let mut settings = umbra_engine::LoadSettings::default();
    settings.extra_plugin_paths = vec![fixture];

    let loaded =
        umbra_engine::load_scenario_with_plugins_and_settings(&registry, &scenario, &[], &settings);
    assert!(loaded.is_ok(), "{:?}", loaded.err().map(|e| e.to_string()));
    let report =
        umbra_engine::validate_path_with_plugins_and_settings(&registry, &scenario, &[], &settings)
            .expect("validate");
    assert_eq!(report.diagnostics, Vec::new());
    // Without the setting the plugin behavior is unknown.
    let bare = umbra_engine::load_scenario_with_plugins_and_settings(
        &registry,
        &scenario,
        &[],
        &umbra_engine::LoadSettings::default(),
    );
    assert!(bare.is_err());
}
