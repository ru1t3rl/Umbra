//! AT-5 (M7, network adapter row, REQ-N-8): the example plugin `example-net-jsonl`
//! (`example.net.jsonl`), the recording network adapter, through the plugin host as a real library
//! (the `dis_plugin.rs` pattern) and through two engines. Requirements: Acceptance 1, REQ-N-8, REQ-N-1,
//! REQ-SDK-3, ADR 0033, ADR 0056, ASM-637, ASM-649, ASM-670.
//!
//! Contract (the crate docs of `examples/plugins/net-jsonl` are the source): params `mode`
//! (`record` | `playback`), `path`, `queue`; key space `(1_000_000, 1_000_000)`. `record` writes one
//! JSON line `{"tick": N, "message": {"type": ..}}` per published message, in publish order, from a
//! writer thread behind a bounded queue (a full queue drops and counts in `tx_dropped`); `stop`
//! flushes and joins. `playback` reads the file at `start` and `poll` at tick `t` delivers, in file
//! order, the messages recorded at ticks `<= t` not yet delivered, at most `max` per call; a line that
//! does not parse counts in `decode_errors`.
//!
//! Step 4 of M7 (WI-2): the loading row passes on the skeleton; the rest is ignored until WI-19.
//! Waits for the writer thread are done by `stop`, which joins it, so nothing here sleeps.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod common;
mod kinds_support;
mod plugin_support;

use core::ops::ControlFlow;
use core::time::Duration;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};

use common::options;
use kinds_support::{example_plugins, lat, lon, scratch_file, write_dir};
use serde_json::{Value, json};
use umbra_core::dead_reckoning::DeadReckoning;
use umbra_core::net::{
    AdapterStart, NetEntityState, NetKey, NetMessage, NetworkAdapter, NetworkAdapterConfig,
    configure_input,
};
use umbra_core::{
    BuildError, ComponentId, ControlAction, Ecef, Force, Ownership, Radians, Registry, SimTime,
    TickRate, Vec3, WorldEuler,
};
use umbra_engine::builtins::standard_registry;
use umbra_engine::{
    HostOptions, PluginHost, RunUntil, instantiate_with_plugins, load_scenario_with_plugins,
};

const CAPABILITY: &str = "example.net.jsonl";

static PACKAGE: LazyLock<PathBuf> = LazyLock::new(|| example_plugins(&["net-jsonl"]));

// ------------------------------------------------------------------------------- the plugin

/// A host with the package loaded and registered.
struct Env {
    host: PluginHost,
    registry: Registry,
}

fn load() -> Env {
    let mut host = PluginHost::new(HostOptions::default());
    for package in PluginHost::discover(std::slice::from_ref(&*PACKAGE)).expect("discover") {
        host.load(&package).expect("load the net-jsonl package");
    }
    let mut registry = Registry::new();
    host.register(&mut registry)
        .expect("register the net-jsonl package");
    Env { host, registry }
}

impl Env {
    fn try_build(&self, params: &Value) -> Result<Arc<dyn NetworkAdapterConfig>, BuildError> {
        let id = ComponentId::new(CAPABILITY).expect("valid id");
        let factory = self
            .registry
            .network_adapter(&id)
            .expect("the adapter is registered");
        let _scope = self.host.build_scope();
        factory.build(&configure_input("jsonl", params, []))
    }

    fn build(&self, params: &Value) -> Arc<dyn NetworkAdapterConfig> {
        self.try_build(params).expect("the adapter builds")
    }
}

fn at(tick: u64) -> SimTime {
    SimTime::at_tick(TickRate::new(30).expect("30 Hz"), tick)
}

fn start(config: &dyn NetworkAdapterConfig) -> Box<dyn NetworkAdapter> {
    config.start(&AdapterStart { now: at(0) }).expect("start")
}

fn record_params(path: &Path) -> Value {
    json!({ "mode": "record", "path": path })
}

fn playback_params(path: &Path) -> Value {
    json!({ "mode": "playback", "path": path })
}

fn tank(key: u64, x: f64) -> NetMessage {
    NetMessage::EntityState(NetEntityState {
        key: NetKey(key),
        entity_type: None,
        force: Some(Force::new("friendly").expect("force")),
        name: Some(Arc::from("Alpha-1")),
        position: Ecef(Vec3::new(3_900_000.0 + x, 400_000.0, 5_000_000.0)),
        velocity: Vec3::new(10.0, -5.0, 2.0),
        acceleration: Vec3::new(0.0, 0.0, 0.0),
        angular_velocity: Vec3::new(0.0, 0.0, 0.0),
        orientation: WorldEuler {
            yaw: Radians(0.5),
            pitch: Radians(-0.25),
            roll: Radians(0.125),
        },
        dead_reckoning: DeadReckoning::Velocity,
    })
}

fn lines(path: &Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .expect("the recording exists")
        .lines()
        .map(|l| serde_json::from_str(l).expect("every line is JSON"))
        .collect()
}

fn write_lines(path: &Path, text: &str) {
    std::fs::write(path, text).expect("write the recording");
}

fn poll_all(adapter: &mut dyn NetworkAdapter, tick: u64, max: usize) -> Vec<NetMessage> {
    let mut got = Vec::new();
    adapter.poll(at(tick), max, &mut got).expect("poll");
    got
}

// ---------------------------------------------------------------------------------- loading

#[test]
fn the_package_loads_at_abi_0_1_and_registers_the_recording_adapter() {
    let env = load();

    let id = ComponentId::new(CAPABILITY).expect("valid id");
    let factory = env.registry.network_adapter(&id).expect("registered");
    let schema = factory.param_schema().as_json();
    assert_eq!(schema["required"], json!(["mode", "path"]));
    assert_eq!(schema["additionalProperties"], json!(false));
    assert_eq!(env.host.plugins()[0].abi, (0, 1));
    assert_eq!(env.host.faults(), Vec::new());
}

#[test]
#[ignore = "M7 WI-19"]
fn bad_params_are_build_errors_naming_the_param() {
    let env = load();
    let path = scratch_file("bad-params.jsonl");
    let cases = [
        (json!({ "mode": "sideways", "path": path }), "mode"),
        (json!({ "mode": "record", "path": "" }), "path"),
        (
            json!({ "mode": "record", "path": path, "queue": 0 }),
            "queue",
        ),
    ];

    for (params, param) in cases {
        let error = env.try_build(&params).err().expect("a build error");

        assert!(error.to_string().contains(param), "{param}: {error}");
    }
}

#[test]
#[ignore = "M7 WI-19"]
fn the_key_space_is_one_million_keys_above_one_million() {
    let env = load();
    let config = env.build(&record_params(&scratch_file("keys.jsonl")));

    let space = config.key_space();

    assert_eq!((space.base, space.count), (1_000_000, 1_000_000));
}

// ----------------------------------------------------------------------------------- record

/// REQ-N-8: every published message is a line, in publish order, with the tick it was published at.
#[test]
#[ignore = "M7 WI-19"]
fn record_mode_writes_every_published_message() {
    let env = load();
    let path = scratch_file("record.jsonl");
    let config = env.build(&record_params(&path));
    let mut adapter = start(config.as_ref());
    adapter
        .publish(at(1), &[tank(7, 0.0), tank(8, 1.0)])
        .expect("publish");
    adapter
        .publish(
            at(2),
            &[
                NetMessage::EntityRemoved { key: NetKey(7) },
                NetMessage::SimulationControl(ControlAction::Pause),
            ],
        )
        .expect("publish");

    adapter.stop();

    let written = lines(&path);
    let shape: Vec<(u64, String)> = written
        .iter()
        .map(|l| {
            (
                l["tick"].as_u64().expect("tick"),
                l["message"]["type"].as_str().expect("type").to_owned(),
            )
        })
        .collect();
    assert_eq!(
        shape,
        [
            (1, "entity_state".to_owned()),
            (1, "entity_state".to_owned()),
            (2, "entity_removed".to_owned()),
            (2, "simulation_control".to_owned()),
        ]
    );
    assert_eq!(written[0]["message"]["key"], 7);
    assert_eq!(written[1]["message"]["key"], 8);
    assert_eq!(written[2]["message"]["key"], 7);
    assert_eq!(written[3]["message"]["action"], "pause");
    assert_eq!(
        written[0]["message"]["position"],
        json!([3_900_000.0, 400_000.0, 5_000_000.0])
    );
    assert_eq!(
        adapter.counters(),
        umbra_core::net::AdapterCounters::default()
    );
}

/// ADR 0033: `stop` flushes what is queued and joins the writer, so the file is complete when it
/// returns; afterwards `publish` is an error, as the adapter contract demands.
#[test]
#[ignore = "M7 WI-19"]
fn stop_flushes_everything_queued_and_publish_after_stop_errors() {
    let env = load();
    let path = scratch_file("flush.jsonl");
    let config = env.build(&record_params(&path));
    let mut adapter = start(config.as_ref());
    for tick in 1..=200 {
        adapter
            .publish(
                at(tick),
                &[tank(7, f64::from(u32::try_from(tick).expect("small")))],
            )
            .expect("publish");
    }

    adapter.stop();

    assert_eq!(lines(&path).len(), 200);
    assert!(adapter.publish(at(201), &[tank(7, 0.0)]).is_err());
}

/// The recorder is a debugging aid and never blocks the tick: a full queue drops and counts. Which
/// messages are dropped depends on the writer thread, but none is lost unaccounted for.
#[test]
#[ignore = "M7 WI-19"]
fn a_full_queue_drops_and_counts_and_nothing_is_lost_unaccounted() {
    let env = load();
    let path = scratch_file("overflow.jsonl");
    let config = env.build(&json!({ "mode": "record", "path": path, "queue": 1 }));
    let mut adapter = start(config.as_ref());
    let batch: Vec<NetMessage> = (0..5000).map(|i| tank(7, f64::from(i))).collect();

    adapter.publish(at(1), &batch).expect("publish");
    adapter.stop();

    let written = lines(&path).len() as u64;
    assert!(written >= 1, "the writer wrote something");
    assert_eq!(written + adapter.counters().tx_dropped, 5000);
}

// ---------------------------------------------------------------------------------- playback

fn control_line(tick: u64, action: &str) -> String {
    format!(
        "{{\"tick\":{tick},\"message\":{{\"type\":\"simulation_control\",\"action\":\"{action}\"}}}}\n"
    )
}

/// REQ-N-8: messages arrive at the first poll of their recorded tick, in file order, at most `max`
/// per call, and never early.
#[test]
#[ignore = "M7 WI-19"]
fn playback_mode_delivers_messages_at_their_recorded_ticks() {
    let env = load();
    let path = scratch_file("playback.jsonl");
    write_lines(
        &path,
        &format!(
            "{}{}{}",
            control_line(3, "pause"),
            control_line(3, "resume"),
            control_line(7, "stop")
        ),
    );
    let config = env.build(&playback_params(&path));
    let mut adapter = start(config.as_ref());

    let early = poll_all(adapter.as_mut(), 2, 8);
    let first = poll_all(adapter.as_mut(), 3, 1);
    let rest = poll_all(adapter.as_mut(), 3, 8);
    let between = poll_all(adapter.as_mut(), 6, 8);
    let last = poll_all(adapter.as_mut(), 7, 8);
    let after = poll_all(adapter.as_mut(), 9, 8);

    assert_eq!(early, Vec::new());
    assert_eq!(first, [NetMessage::SimulationControl(ControlAction::Pause)]);
    assert_eq!(rest, [NetMessage::SimulationControl(ControlAction::Resume)]);
    assert_eq!(between, Vec::new());
    assert_eq!(last, [NetMessage::SimulationControl(ControlAction::Stop)]);
    assert_eq!(after, Vec::new(), "each message is delivered once");
    adapter.stop();
}

/// A recording the adapter wrote plays back to the same messages (the encoding round-trips).
#[test]
#[ignore = "M7 WI-19"]
fn playback_returns_what_record_wrote() {
    let env = load();
    let path = scratch_file("round-trip.jsonl");
    let sent = [
        tank(7, 0.0),
        NetMessage::EntityRemoved { key: NetKey(7) },
        NetMessage::SimulationControl(ControlAction::Resume),
    ];
    let mut recorder = start(env.build(&record_params(&path)).as_ref());
    recorder.publish(at(4), &sent).expect("publish");
    recorder.stop();

    let mut player = start(env.build(&playback_params(&path)).as_ref());
    let got = poll_all(player.as_mut(), 4, 8);

    assert_eq!(got, sent);
    player.stop();
}

#[test]
#[ignore = "M7 WI-19"]
fn a_line_that_does_not_parse_is_counted_and_the_rest_still_plays() {
    let env = load();
    let path = scratch_file("garbage.jsonl");
    write_lines(
        &path,
        &format!(
            "{}this is not json\n{}",
            control_line(1, "start"),
            control_line(1, "stop")
        ),
    );
    let mut adapter = start(env.build(&playback_params(&path)).as_ref());

    let got = poll_all(adapter.as_mut(), 1, 8);

    assert_eq!(
        got,
        [
            NetMessage::SimulationControl(ControlAction::Start),
            NetMessage::SimulationControl(ControlAction::Stop)
        ]
    );
    assert_eq!(adapter.counters().decode_errors, 1);
    adapter.stop();
}

// --------------------------------------------------------------------- the adapter contract

/// A recording of three control messages at tick 0, so the fed poll test finds them.
fn contract_playback_config() -> Arc<dyn NetworkAdapterConfig> {
    let path = scratch_file("contract-playback.jsonl");
    write_lines(
        &path,
        &format!(
            "{}{}{}",
            control_line(0, "start"),
            control_line(0, "pause"),
            control_line(0, "resume")
        ),
    );
    load().build(&playback_params(&path))
}

fn contract_record_config() -> Arc<dyn NetworkAdapterConfig> {
    load().build(&record_params(&scratch_file("contract-record.jsonl")))
}

/// The recording is already in the file, so nothing needs feeding.
fn no_feed(_: &mut dyn NetworkAdapter, _: &[NetMessage]) {}

mod contract_playback {
    umbra_core::network_adapter_contract_tests!(
        attrs = [#[ignore = "M7 WI-19"]],
        super::contract_playback_config(),
        feed = super::no_feed
    );
}

mod contract_record {
    umbra_core::network_adapter_contract_tests!(
        attrs = [#[ignore = "M7 WI-19"]],
        super::contract_record_config()
    );
}

// ------------------------------------------------------------------------- two engines

fn scenario(adapter_params: &str, entities: &str) -> PathBuf {
    let text = format!(
        r#"schema_version = 1

[scenario]
name = "net_jsonl"
seed = 7
tick_rate_hz = 30
clock = "afap"
duration_s = 2.0
template_paths = ["templates"]

[terrain]
provider = "builtin.terrain.flat"
params = {{ height_m = 0.0 }}

[[network_adapters]]
adapter = "example.net.jsonl"
name = "rec"
params = {adapter_params}

[network_adapters.ghosts]
default_template = "example.jsonl.vehicle"
{entities}"#
    );
    let vehicle = "schema_version = 1\nid = \"example.jsonl.vehicle\"\n\n[motion]\n\
        model = \"builtin.ground.kinematic\"\n\
        params = { max_speed_mps = 18.0, max_turn_rate_dps = 40.0, max_accel_mps2 = 2.5 }\n";
    write_dir(
        "net-jsonl",
        &[
            ("scenario.toml", text.as_str()),
            ("templates/vehicle.toml", vehicle),
        ],
    )
    .join("scenario.toml")
}

fn toml_path(path: &Path) -> String {
    format!("{:?}", path.to_str().expect("a UTF-8 path"))
}

/// Runs `scenario_path` to its end; returns, per tick, the number of remote entities.
fn ghosts_per_tick(scenario_path: &Path) -> Vec<usize> {
    let loaded = load_scenario_with_plugins(
        &standard_registry().expect("registry"),
        scenario_path,
        std::slice::from_ref(&*PACKAGE),
    )
    .expect("load the scenario");
    let run = instantiate_with_plugins(&loaded, options(Duration::from_micros(500), None, None))
        .expect("instantiate");
    let (mut engine, ticks) = (run.engine, run.duration_ticks);
    let mut ghosts = Vec::new();
    engine
        .run(RunUntil::Ticks(ticks), |view| {
            let remote = view
                .snapshot()
                .entities
                .iter()
                .filter(|e| matches!(e.ownership, Ownership::Remote { .. }))
                .count();
            ghosts.push(remote);
            ControlFlow::Continue(())
        })
        .expect("run");
    ghosts
}

/// REQ-N-8, Acceptance 1: an engine records its owned entity with the adapter in `record` mode; a
/// second engine with no entity of that kind plays the file back and creates the ghost. The ghost
/// appears when the first recorded line is due, within one tick (whether the engine polls before or
/// after the publish of that tick is settled by WI-19), and is the only one.
#[test]
#[ignore = "M7 WI-19"]
fn playback_mode_recreates_ghosts() {
    let file = scratch_file("engines.jsonl");
    let mover = format!(
        "\n[[entities]]\ntemplate = \"example.jsonl.vehicle\"\nname = \"mover\"\n\
         position = {{ lat_deg = {:.7}, lon_deg = {:.7} }}\nheading_deg = 90.0\nspeed_mps = 10.0\n",
        lat(0.0),
        lon(0.0)
    );
    let bystander = format!(
        "\n[[entities]]\ntemplate = \"example.jsonl.vehicle\"\nname = \"bystander\"\n\
         position = {{ lat_deg = {:.7}, lon_deg = {:.7} }}\n",
        lat(5000.0),
        lon(5000.0)
    );
    let recording = format!("{{ mode = \"record\", path = {} }}", toml_path(&file));
    let playback = format!("{{ mode = \"playback\", path = {} }}", toml_path(&file));
    let live = ghosts_per_tick(&scenario(&recording, &mover));
    assert!(live.iter().all(|&n| n == 0), "the recorder has no ghosts");
    let recorded = lines(&file);
    let first_tick = usize::try_from(recorded[0]["tick"].as_u64().expect("tick")).expect("small");
    assert_eq!(recorded[0]["message"]["type"], "entity_state");

    let replayed = ghosts_per_tick(&scenario(&playback, &bystander));

    assert!(
        replayed[..first_tick.saturating_sub(1)]
            .iter()
            .all(|&n| n == 0),
        "no ghost before the first line is due: {replayed:?}"
    );
    assert!(
        replayed[first_tick + 1..].iter().all(|&n| n == 1),
        "exactly one ghost from then on: {replayed:?}"
    );
}
