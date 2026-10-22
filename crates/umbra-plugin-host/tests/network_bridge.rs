//! AT-16 and AT-21 (M6 WI-11): the host network bridge over ABI 0.1 (REQ-N-1, REQ-P-6..8, REQ-P-11,
//! D-8, ADR 0021, ADR 0030, ADR 0033, ASM-252, ASM-288). The plugin is the hand-written raw-ABI network
//! adapter of `net_support` (loaded with `PluginHost::load_static`), so every fault row is reachable,
//! including those the safe SDK cannot produce.
//!
//! Covered: registration as `ComponentKind::NetworkAdapter`; `configure` with the ADR 0030 wrapper and
//! the cached, checked key space; build, start and lifecycle errors; message marshalling both ways
//! (every kind, the plugin's minor in the host-written headers, unused fields ignored); `max` and the
//! ABI poll cap; every fault (a panic or a non-OK code in `publish`, `poll`, `counters`, an invalid
//! kind, a non-UTF-8 string, more than `max` pushes, a sink token used after its call) faulting the
//! capability under the host's policy, with only `stop` and `destroy` afterwards; counters read only
//! on the tick path; `stop` and `destroy` exactly once, before the image is released; the plugin's
//! I/O thread refused by the host API and joined before `destroy`; a host-refused `start` (validation
//! host, faulted capability) dropping with `destroy` only; a started adapter used and dropped on
//! another thread; several adapters at once (ASM-410: two images interleaved on one thread, a foreign
//! sink token, one plugin's two capabilities and one capability under two names, a fault confined to
//! its capability, each image released after its own adapter); the core adapter contract suite
//! through the bridge, fed through the plugin's echo path.
//!
//! The rows that need a real SDK-built library (the network capabilities of `umbra-test-plugin`) are
//! the `plugin_failures.rs` network rows and the engine-level `net_plugin_faults.rs` (WI-12, ASM-407).
#![expect(unsafe_code, reason = "forged! publishes raw ABI entry functions")]
#![expect(clippy::expect_used, reason = "tests build known-valid fixtures")]

#[macro_use]
mod support;
#[macro_use]
mod net_support;

use std::sync::Arc;

use net_support::{CAPABILITY, NAME};
use umbra_core::dead_reckoning::DeadReckoning;
use umbra_core::net::{
    AdapterCounters, AdapterStart, KeySpace, NetEntityState, NetEntityType, NetError, NetKey,
    NetMessage, NetworkAdapter, NetworkAdapterConfig, configure_input,
};
use umbra_core::{
    ComponentId, ComponentKind, ControlAction, Ecef, Force, PluginFaultPolicy, Radians, SimTime,
    TickRate, Vec3, WeaponId, WorldEuler,
};
use umbra_plugin_api::{UMBRA_ABI_VERSION, UMBRA_MAX_NET_POLL, UmbraNetBatch, UmbraNetMessage};
use umbra_plugin_host::{HostMode, PluginHost};

fn rate() -> TickRate {
    TickRate::new(30).expect("30 Hz")
}

fn at(tick: u64) -> SimTime {
    SimTime::at_tick(rate(), tick)
}

fn start(config: &dyn NetworkAdapterConfig) -> Box<dyn NetworkAdapter> {
    config
        .start(&AdapterStart { now: at(0) })
        .expect("start the adapter")
}

fn poll(
    adapter: &mut dyn NetworkAdapter,
    tick: u64,
    max: usize,
) -> Result<Vec<NetMessage>, NetError> {
    let mut into = Vec::new();
    adapter.poll(at(tick), max, &mut into).map(|()| into)
}

fn v(x: f64, y: f64, z: f64) -> Vec3 {
    Vec3 { x, y, z }
}

fn every_kind() -> Vec<NetMessage> {
    vec![
        NetMessage::EntityState(NetEntityState {
            key: NetKey(1001),
            entity_type: Some(NetEntityType::new("1:2:225:1:1:0:0")),
            force: Some(Force::new("blue").expect("force")),
            name: Some(Arc::from("Alpha 1")),
            position: Ecef(v(3_900_000.0, 300_000.0, 5_000_000.0)),
            velocity: v(1.0, -2.0, 0.5),
            acceleration: v(0.0, 0.25, 0.0),
            angular_velocity: v(0.0, 0.0, 0.1),
            orientation: WorldEuler {
                yaw: Radians(1.0),
                pitch: Radians(-0.5),
                roll: Radians(0.25),
            },
            dead_reckoning: DeadReckoning::Velocity,
        }),
        NetMessage::EntityState(NetEntityState {
            key: NetKey(1002),
            entity_type: None,
            force: None,
            name: None,
            position: Ecef(v(1.0, 2.0, 3.0)),
            velocity: v(0.0, 0.0, 0.0),
            acceleration: v(0.0, 0.0, 0.0),
            angular_velocity: v(0.0, 0.0, 0.0),
            orientation: WorldEuler {
                yaw: Radians(0.0),
                pitch: Radians(0.0),
                roll: Radians(0.0),
            },
            dead_reckoning: DeadReckoning::BodyAcceleration,
        }),
        NetMessage::EntityRemoved { key: NetKey(1003) },
        NetMessage::Fire {
            shooter: NetKey(1001),
            target: Some(NetKey(7)),
            weapon: WeaponId::new("example.weapon.cannon").expect("weapon"),
            position: Ecef(v(4.0, 5.0, 6.0)),
        },
        NetMessage::Detonation {
            shooter: None,
            target: None,
            position: Ecef(v(7.0, 8.0, 9.0)),
        },
        NetMessage::SimulationControl(ControlAction::Pause),
    ]
}

/// The fault the host recorded for the raw capability, with its call.
fn only_fault(env: &net_support::NetEnv) -> (&'static str, String) {
    let faults = env.host.faults();
    assert_eq!(faults.len(), 1, "exactly one fault: {faults:?}");
    assert_eq!(faults[0].capability.as_str(), CAPABILITY);
    (faults[0].call, faults[0].message.clone())
}

/// The events after the first one starting with `marker`.
fn events_after(env: &net_support::NetEnv, marker: &str) -> Vec<&'static str> {
    let events = env.events();
    let at = events
        .iter()
        .position(|e| e.starts_with(marker))
        .unwrap_or_else(|| panic!("no event {marker} in {events:?}"));
    events[at + 1..].to_vec()
}

// ------------------------------------------------------------------ registration and build

#[test]
fn a_network_capability_registers_as_a_network_adapter() {
    let env = net_env!();
    let id = ComponentId::new(CAPABILITY).expect("id");

    assert_eq!(
        env.registry.kind_of(&id),
        Some(ComponentKind::NetworkAdapter)
    );
    assert!(env.registry.network_adapter(&id).is_some());
    assert!(env.registry.behavior(&id).is_none());
    let info = &env.host.plugins()[0];
    assert_eq!(info.abi, (0, 1));
    assert_eq!(info.capabilities[0].kind, ComponentKind::NetworkAdapter);
    assert!(!info.capabilities[0].faulted);
}

#[test]
fn build_passes_the_adr_0030_wrapper_and_caches_the_checked_key_space() {
    let env = net_env!();

    let config = env
        .try_build("echo", r#","base":5000,"count":250"#)
        .expect("build");

    let input = configure_input(NAME, &env.params("echo", r#","base":5000,"count":250"#), []);
    let configure = format!("configure {}", serde_json::to_string(&input).expect("json"));
    assert_eq!(
        env.events(),
        ["init", configure.as_str(), "key_space"],
        "configure gets the wrapper as JSON text, then the key space is read once"
    );
    for _ in 0..3 {
        assert_eq!(
            config.key_space(),
            KeySpace {
                base: 5000,
                count: 250
            }
        );
    }
    assert_eq!(env.count("key_space"), 1, "cached: no further plugin call");
}

#[test]
fn configure_failures_are_build_errors_naming_the_capability_and_not_faults() {
    let env = net_env!();
    for (mode, needle) in [
        (
            "configure_error",
            "UMBRA_ERR_PARAMS (3): params.port: 70000 is out of range",
        ),
        (
            "configure_panic",
            "UMBRA_ERR_PANIC (1): panicked: configure exploded",
        ),
        (
            "configure_null",
            "configure returned UMBRA_OK with a null handle",
        ),
    ] {
        let err = env.try_build(mode, "").err().expect(mode);
        let msg = err.to_string();
        assert!(
            msg.contains(&format!("plugin network adapter {CAPABILITY}")) && msg.contains(needle),
            "{mode}: {msg}"
        );
    }
    assert!(
        env.host.faults().is_empty(),
        "build errors are not faults (R-M9)"
    );
    assert_eq!(
        env.count("destroy"),
        0,
        "no handle was made, so nothing is destroyed"
    );
}

#[test]
fn a_bad_key_space_is_a_build_error_and_the_handle_is_destroyed() {
    let env = net_env!();
    for (mode, extra, needle) in [
        ("echo", r#","count":0"#, "count is 0"),
        (
            "echo",
            r#","base":18446744073709551615,"count":1"#,
            "overflows u64",
        ),
        ("key_error", "", "key_space returned UMBRA_ERR_INTERNAL"),
    ] {
        let before = env.count("destroy");
        let err = env.try_build(mode, extra).err().expect(needle);

        assert!(err.to_string().contains(needle), "{err}");
        assert!(err.to_string().contains(CAPABILITY), "{err}");
        assert_eq!(
            env.count("destroy"),
            before + 1,
            "{needle}: configured handle destroyed"
        );
    }
    assert_eq!(env.host.faults().len(), 0);
}

#[test]
fn the_largest_key_space_that_fits_is_accepted() {
    let env = net_env!();

    let config = env
        .try_build("echo", r#","base":18446744073709551614,"count":1"#)
        .expect("base + count == u64::MAX fits");

    assert_eq!(config.key_space().count, 1);
}

#[test]
fn a_run_mode_host_builds_only_inside_its_build_scope() {
    let env = net_env!();
    let id = ComponentId::new(CAPABILITY).expect("id");
    let factory = env.registry.network_adapter(&id).expect("registered");
    let input = configure_input(NAME, &env.params("echo", ""), []);

    let err = factory.build(&input).err().expect("outside the scope");

    assert!(
        err.to_string()
            .contains("plugin host not attached to this engine"),
        "{err}"
    );
    assert_eq!(env.count("configure"), 0, "refused before any plugin call");
}

// ------------------------------------------------------------------ start

#[test]
fn start_twice_is_already_started_without_a_second_plugin_call() {
    let env = net_env!();
    let config = env.config("echo");
    let mut first = start(config.as_ref());

    let second = config.start(&AdapterStart { now: at(3) });

    assert!(matches!(second, Err(NetError::AlreadyStarted)));
    assert_eq!(env.count("start"), 1);
    assert!(env.events().contains(&"start tick=0 rate=30"));
    first.stop();
}

#[test]
fn a_failed_start_is_a_start_error_and_the_handle_is_still_stopped_and_destroyed() {
    let env = net_env!();
    let config = env.config("start_error");

    let err = config
        .start(&AdapterStart { now: at(0) })
        .err()
        .expect("start fails");

    assert!(
        matches!(&err, NetError::Start(m) if m.contains(CAPABILITY) && m.contains("bind failed: address in use")),
        "{err:?}"
    );
    assert!(env.host.faults().is_empty(), "a start error is not a fault");
    drop(config);
    assert_eq!(events_after(&env, "start"), ["stop", "destroy"]);
}

#[test]
fn a_validation_host_never_starts_adapters() {
    let env = net_env!(
        mode = HostMode::Validation,
        policy = PluginFaultPolicy::Abort
    );
    let id = ComponentId::new(CAPABILITY).expect("id");
    let factory = env.registry.network_adapter(&id).expect("registered");

    let config = factory
        .build(&configure_input(NAME, &env.params("echo", ""), []))
        .expect("a validation host builds anywhere (dry run)");
    let err = config
        .start(&AdapterStart { now: at(0) })
        .err()
        .expect("refused");

    assert!(
        matches!(&err, NetError::Start(m) if m.contains("validation host")),
        "{err:?}"
    );
    assert_eq!(env.count("start"), 0);
    drop(config);
    assert_eq!(
        events_after(&env, "key_space"),
        ["destroy"],
        "a start the host refused claims nothing: no stop on drop"
    );
}

#[test]
fn a_start_refused_because_the_capability_is_faulted_drops_with_destroy_only() {
    let env = net_env!(
        mode = HostMode::Run,
        policy = PluginFaultPolicy::DisableCapability
    );
    let faulty = env.config("poll_error");
    let later = env
        .try_build("echo", r#","tag":"later""#)
        .expect("built before the fault");
    let mut a = start(faulty.as_ref());
    assert!(poll(a.as_mut(), 1, 8).is_err(), "faults the capability");

    let err = later
        .start(&AdapterStart { now: at(2) })
        .err()
        .expect("refused");

    assert!(
        matches!(&err, NetError::Start(m) if m.contains("faulted for this host")),
        "{err:?}"
    );
    drop(later);
    let events = net_support::events_tagged(env.f.order, "later");
    assert!(events[0].starts_with("configure "), "{events:?}");
    assert_eq!(
        events[1..],
        ["key_space", "destroy"],
        "after configure only key_space and destroy: no start and no stop"
    );
}

// ------------------------------------------------------------------ marshalling

#[test]
fn every_message_kind_round_trips_through_publish_and_poll() {
    let env = net_env!();
    let config = env.config("echo");
    let mut adapter = start(config.as_ref());
    let sent = every_kind();

    adapter.publish(at(4), &sent).expect("publish");
    let got = poll(adapter.as_mut(), 5, 64).expect("poll");

    assert_eq!(got, sent);
    let msg = std::mem::size_of::<UmbraNetMessage>();
    let batch = std::mem::size_of::<UmbraNetBatch>();
    assert!(
        env.events().contains(
            &format!(
                "publish v={UMBRA_ABI_VERSION:#x} size={batch} tick=4 rate=30 count=6 null=false"
            )
            .as_str()
        ),
        "{:?}",
        env.events()
    );
    assert_eq!(
        env.count(&format!("message v={UMBRA_ABI_VERSION:#x} size={msg}")),
        6,
        "every host-written message carries the plugin's minor (1) and size"
    );
    assert_eq!(
        env.count(&format!(
            "poll tick=5 max=64 sink v={UMBRA_ABI_VERSION:#x} size=24"
        )),
        1
    );
    adapter.stop();
}

#[test]
fn a_step_control_action_is_not_sent() {
    let env = net_env!();
    let config = env.config("echo");
    let mut adapter = start(config.as_ref());

    adapter
        .publish(
            at(1),
            &[
                NetMessage::SimulationControl(ControlAction::Step(5)),
                NetMessage::SimulationControl(ControlAction::Stop),
            ],
        )
        .expect("publish");

    assert_eq!(env.count("message "), 1);
    assert_eq!(
        poll(adapter.as_mut(), 1, 8).expect("poll"),
        [NetMessage::SimulationControl(ControlAction::Stop)]
    );
}

#[test]
fn fields_unused_by_the_kind_are_ignored_inbound() {
    let env = net_env!();
    let config = env.config("unused_fields");
    let mut adapter = start(config.as_ref());

    let got = poll(adapter.as_mut(), 1, 8).expect("garbage in unused fields is no fault");

    assert_eq!(got, [NetMessage::EntityRemoved { key: NetKey(5) }]);
    assert_eq!(env.host.faults().len(), 0);
}

#[test]
fn poll_passes_max_capped_by_the_abi_and_returns_at_most_that() {
    let env = net_env!();
    let config = env.config("echo");
    let mut adapter = start(config.as_ref());
    let many: Vec<NetMessage> = (1..=UMBRA_MAX_NET_POLL as u64 + 10)
        .map(|k| NetMessage::EntityRemoved { key: NetKey(k) })
        .collect();
    adapter.publish(at(1), &many).expect("publish");

    let first = poll(adapter.as_mut(), 2, 3).expect("poll 3");
    let second = poll(adapter.as_mut(), 3, usize::MAX).expect("poll capped");

    assert_eq!(first, many[..3]);
    assert_eq!(second.len(), UMBRA_MAX_NET_POLL);
    assert_eq!(second, many[3..3 + UMBRA_MAX_NET_POLL]);
    assert_eq!(
        env.count(&format!("poll tick=3 max={UMBRA_MAX_NET_POLL} ")),
        1
    );
}

// ------------------------------------------------------------------ faults

/// Starts an adapter in `mode`, runs `call` on it once, and checks the fault: the call returned
/// `NetError::Faulted`, the host recorded one fault of `fault_call` containing `needle`, nothing was
/// appended, and afterwards `publish` and `poll` fail without a plugin call while `stop` and
/// `destroy` still run.
fn assert_faults(
    env: &net_support::NetEnv,
    mode: &str,
    fault_call: &str,
    needle: &str,
    call: impl FnOnce(&mut dyn NetworkAdapter) -> Result<Vec<NetMessage>, NetError>,
) {
    let config = env.config(mode);
    let mut adapter = start(config.as_ref());

    let result = call(adapter.as_mut());

    assert!(
        matches!(&result, Err(NetError::Faulted(m)) if m.contains(CAPABILITY)),
        "{mode}: {result:?}"
    );
    let (call_name, message) = only_fault(env);
    assert_eq!(call_name, fault_call, "{mode}");
    assert!(message.contains(needle), "{mode}: {message}");
    assert!(env.host.plugins()[0].capabilities[0].faulted);
    let calls_before = env.events().len();
    assert!(adapter.publish(at(9), &every_kind()).is_err());
    assert!(poll(adapter.as_mut(), 9, 8).is_err());
    assert_eq!(
        env.events().len(),
        calls_before,
        "{mode}: no plugin call after the fault"
    );
    drop(adapter);
    drop(config);
    let after = &env.events()[calls_before..];
    assert_eq!(
        after,
        ["stop", "destroy"],
        "{mode}: only stop and destroy after a fault"
    );
}

#[test]
fn a_panic_in_poll_faults_and_discards_the_pushed_messages_whole() {
    let env = net_env!();
    assert_faults(
        &env,
        "poll_panic",
        "poll",
        "UMBRA_ERR_PANIC (1): panicked: poll exploded",
        |a| {
            let mut into = vec![NetMessage::SimulationControl(ControlAction::Start)];
            let r = a.poll(at(1), 8, &mut into);
            assert_eq!(
                into,
                [NetMessage::SimulationControl(ControlAction::Start)],
                "nothing appended"
            );
            r.map(|()| into)
        },
    );
    assert!(
        env.events().contains(&"pushed=0"),
        "the push itself was accepted, then discarded"
    );
}

#[test]
fn a_panic_in_publish_faults() {
    let env = net_env!();
    assert_faults(
        &env,
        "publish_panic",
        "publish",
        "panicked: publish exploded",
        |a| a.publish(at(1), &every_kind()).map(|()| Vec::new()),
    );
}

#[test]
fn a_non_ok_poll_faults_with_the_plugins_message() {
    let env = net_env!();
    assert_faults(
        &env,
        "poll_error",
        "poll",
        "UMBRA_ERR_INTERNAL (5): socket closed",
        |a| poll(a, 1, 8),
    );
}

#[test]
fn an_unknown_message_kind_faults() {
    let env = net_env!();
    assert_faults(
        &env,
        "bad_kind",
        "poll",
        "message kind 99 is unknown",
        |a| poll(a, 1, 8),
    );
    assert!(
        env.events().contains(&"pushed=2"),
        "the push was refused (INVALID_ARGUMENT)"
    );
}

#[test]
fn a_non_utf8_string_faults() {
    let env = net_env!();
    assert_faults(
        &env,
        "bad_utf8",
        "poll",
        "ENTITY_STATE name is not valid UTF-8",
        |a| poll(a, 1, 8),
    );
}

#[test]
fn more_than_max_pushes_fault() {
    let env = net_env!();
    assert_faults(
        &env,
        "flood",
        "poll",
        "more than max = 3 messages in one poll",
        |a| poll(a, 1, 3),
    );
    assert!(
        env.events().contains(&"push 3 refused=2"),
        "{:?}",
        env.events()
    );
}

#[test]
fn a_sink_token_used_after_its_call_faults() {
    let env = net_env!();
    assert_faults(&env, "stale", "poll", "stale or foreign token", |a| {
        let first = poll(a, 1, 8).expect("the first poll only keeps its sink");
        assert_eq!(first.len(), 0);
        poll(a, 2, 8)
    });
    assert!(
        env.events().contains(&"stale push=6"),
        "refused with UMBRA_ERR_STATE"
    );
}

#[test]
fn a_non_ok_counters_faults_and_discards_the_poll() {
    let env = net_env!();
    assert_faults(
        &env,
        "counters_error",
        "counters",
        "UMBRA_ERR_INTERNAL",
        |a| poll(a, 1, 8),
    );
}

#[test]
fn under_abort_the_fault_is_handed_out_once() {
    let env = net_env!();
    let config = env.config("poll_error");
    let mut adapter = start(config.as_ref());

    assert!(poll(adapter.as_mut(), 1, 8).is_err());

    let abort = env.host.take_abort().expect("abort pending");
    assert_eq!(
        (abort.call, abort.capability.as_str()),
        ("poll", CAPABILITY)
    );
    assert!(env.host.take_abort().is_none());
}

#[test]
fn under_disable_capability_there_is_no_abort_and_every_adapter_of_the_capability_stops_working() {
    let env = net_env!(
        mode = HostMode::Run,
        policy = PluginFaultPolicy::DisableCapability
    );
    let faulty = env.config("poll_error");
    let healthy = env.config("echo");
    let mut a = start(faulty.as_ref());
    let mut b = start(healthy.as_ref());
    b.publish(at(1), &every_kind())
        .expect("b works before the fault");

    assert!(poll(a.as_mut(), 1, 8).is_err());

    assert!(env.host.take_abort().is_none());
    assert_eq!(env.host.faults().len(), 1);
    let polls = env.count("poll");
    assert!(
        matches!(poll(b.as_mut(), 2, 8), Err(NetError::Faulted(m)) if m.contains("faulted for this host")),
        "the capability, not one adapter, is faulted (ADR 0021)"
    );
    assert_eq!(env.count("poll"), polls, "no plugin call");
    assert!(
        env.try_build("echo", "").is_err(),
        "a faulted capability builds nothing"
    );
}

// ------------------------------------------------------------------ counters

#[test]
fn counters_are_read_after_each_poll_and_before_stop_and_cached_otherwise() {
    let env = net_env!();
    let config = env.config("echo");
    let mut adapter = start(config.as_ref());
    assert_eq!(
        adapter.counters(),
        AdapterCounters::default(),
        "nothing read before the first poll"
    );

    for tick in 1..=3 {
        poll(adapter.as_mut(), tick, 8).expect("poll");
        for _ in 0..5 {
            let c = adapter.counters();
            assert_eq!(
                (c.tx_dropped, c.rx_dropped, c.decode_errors, c.io_errors),
                (tick * 10 + 1, tick * 10 + 2, tick * 10 + 3, tick * 10 + 4),
                "the values of the last poll"
            );
        }
    }
    adapter.publish(at(4), &every_kind()).expect("publish");

    assert_eq!(
        env.count("counters"),
        3,
        "one read per poll, none for counters() or publish"
    );
    assert_eq!(
        env.count("counters n=4 zeroed=true"),
        3,
        "four counters, zero-filled"
    );
    adapter.stop();
    assert_eq!(env.count("counters"), 4, "and one before stop");
    let tail = events_after(&env, "publish");
    assert_eq!(
        &tail[tail.len() - 2..],
        ["counters n=4 zeroed=true", "stop"]
    );
}

#[test]
fn an_adapter_without_counters_reports_zero() {
    let f = forged!(net_support::parts_with(|vt| vt.counters = None));
    let env = net_support::NetEnv::load(f, HostMode::Run, PluginFaultPolicy::Abort);
    let config = env.config("echo");
    let mut adapter = start(config.as_ref());

    poll(adapter.as_mut(), 1, 8).expect("poll");

    assert_eq!(adapter.counters(), AdapterCounters::default());
    assert_eq!(env.count("counters"), 0);
}

// ------------------------------------------------------------------ lifecycle and unload order

#[test]
fn stop_and_destroy_run_exactly_once_on_drop_before_the_image_is_released() {
    let f = forged!(net_support::parts());
    let env = net_support::NetEnv::load(f, HostMode::Run, PluginFaultPolicy::Abort);
    let config = env.config("echo");
    let mut adapter = start(config.as_ref());
    adapter.stop();
    adapter.stop();
    let net_support::NetEnv {
        host,
        registry,
        f,
        log: _,
    } = env;
    let events = || f.order.lock().expect("order").clone();

    drop(host);
    drop(registry);
    assert!(
        !events().contains(&"shutdown"),
        "the adapter and config hold the image"
    );
    drop(adapter);
    assert!(
        !events().contains(&"destroy"),
        "the configuration still holds the handle"
    );
    drop(config);

    let all = events();
    let tail = &all[all.iter().position(|e| *e == "stop").expect("stop")..];
    assert_eq!(tail, ["stop", "destroy", "shutdown"]);
}

#[test]
fn dropping_a_started_adapter_without_stop_stops_it() {
    let env = net_env!();
    let config = env.config("echo");
    let adapter = start(config.as_ref());

    drop(adapter);
    drop(config);

    assert_eq!(
        events_after(&env, "start"),
        ["counters n=4 zeroed=true", "stop", "destroy"]
    );
}

#[test]
fn a_dry_run_configuration_is_destroyed_without_start_or_stop() {
    let env = net_env!();

    drop(env.config("echo"));

    assert_eq!(events_after(&env, "key_space"), ["destroy"]);
}

#[test]
fn the_io_thread_is_refused_by_the_host_api_and_joined_before_destroy() {
    let f = forged!(net_support::parts());
    let env = net_support::NetEnv::load(f, HostMode::Run, PluginFaultPolicy::Abort);
    let config = env.config("thread");
    let adapter = start(config.as_ref());
    let net_support::NetEnv {
        host,
        registry,
        f,
        log,
    } = env;

    drop(host);
    drop(registry);
    drop(adapter);
    drop(config);

    let all = f.order.lock().expect("order").clone();
    let from = all
        .iter()
        .position(|e| e.starts_with("thread log="))
        .expect("logged");
    assert_eq!(
        &all[from..],
        [
            "thread log=STATE",
            "counters n=4 zeroed=true",
            "stop",
            "thread done",
            "joined ok=true",
            "destroy",
            "shutdown"
        ],
        "UMBRA_ERR_STATE off a call record (ADR 0021); stop joins; destroy and shutdown after"
    );
    assert!(
        log.all()
            .iter()
            .all(|(_, _, m)| !m.contains("from the I/O thread")),
        "the refused message never reached the host log"
    );
}

// ------------------------------------------------------------------ the core contract (AT-21)

/// Delivers `messages` through the plugin's echo path: published now, polled next.
fn feed(adapter: &mut dyn NetworkAdapter, messages: &[NetMessage]) {
    adapter
        .publish(at(0), messages)
        .expect("feed through publish");
}

mod contract {
    use super::feed;

    umbra_core::network_adapter_contract_tests!(net_config!("echo"), feed = feed);
}

#[test]
fn two_hosts_over_one_image_fault_independently() {
    let f = forged!(net_support::parts());
    let entry = f.entry;
    let env = net_support::NetEnv::load(f, HostMode::Run, PluginFaultPolicy::Abort);
    let mut other = PluginHost::new(umbra_plugin_host::HostOptions::default());
    other
        .load_static("net-origin-2", entry)
        .expect("second host reuses the image");
    let config = env.config("poll_error");
    let mut adapter = start(config.as_ref());

    assert!(poll(adapter.as_mut(), 1, 8).is_err());

    assert_eq!(env.host.faults().len(), 1);
    assert!(
        other.faults().is_empty(),
        "fault state is per host (ADR 0021)"
    );
}

// ------------------------------------------------------------------ threads

/// Backs the `Send`/`Sync` SAFETY claims of the bridge's `AdapterHandle` and `MessageBuf`: a started
/// adapter and its configuration move to another thread, which publishes, polls and drops them.
#[test]
fn a_started_adapter_moves_to_another_thread_and_is_used_and_dropped_there() {
    let env = net_env!();
    let config = env.config("echo");
    let mut adapter = start(config.as_ref());
    adapter
        .publish(at(1), &every_kind()[..1])
        .expect("publish on the starting thread");
    let sent = every_kind();

    let got = std::thread::spawn(move || {
        let mut got = poll(adapter.as_mut(), 1, 64).expect("poll there");
        adapter.publish(at(2), &sent[1..]).expect("publish there");
        got.extend(poll(adapter.as_mut(), 2, 64).expect("poll there again"));
        drop(adapter);
        drop(config);
        got
    })
    .join()
    .expect("the other thread finished");

    assert_eq!(got, every_kind());
    let events = env.events();
    assert_eq!(
        events[events.len() - 3..],
        ["counters n=4 zeroed=true", "stop", "destroy"],
        "stopped and destroyed on the other thread"
    );
}

// ------------------------------------------------------------------ several adapters (ASM-410)

/// Two network plugin images in one host: image 0 `test.plugin.net_a` with `test.net.a`, image 1
/// `test.plugin.net_b` with `test.net.b` (one `forged!` per expansion, so per test).
macro_rules! two_images {
    ($policy:expr) => {
        net_support::NetImages::load(
            vec![
                forged!(net_support::parts_for("test.plugin.net_a", &["test.net.a"])),
                forged!(net_support::parts_for("test.plugin.net_b", &["test.net.b"])),
            ],
            $policy,
        )
    };
}

/// The sink tokens a tagged adapter's polls were given.
fn sink_tokens(events: &[&'static str]) -> Vec<&'static str> {
    events
        .iter()
        .filter_map(|e| e.strip_prefix("sink ctx="))
        .collect()
}

#[test]
fn two_plugin_images_interleaved_on_one_thread_keep_messages_counters_and_sinks_apart() {
    let env = two_images!(PluginFaultPolicy::Abort);
    let ca = env.build(0, "test.net.a", "net_a", "echo", "a", 1000);
    let cb = env.build(1, "test.net.b", "net_b", "echo", "b", 5000);
    assert_eq!(
        ca.key_space(),
        KeySpace {
            base: 1000,
            count: 100
        }
    );
    assert_eq!(
        cb.key_space(),
        KeySpace {
            base: 5000,
            count: 100
        }
    );
    let mut a = start(ca.as_ref());
    let mut b = start(cb.as_ref());
    let all = every_kind();
    let (to_a, to_b) = all.split_at(3);

    // One tick, interleaved: publish a, publish b, poll b, poll a; then a polls once more.
    a.publish(at(1), to_a).expect("publish a");
    b.publish(at(1), to_b).expect("publish b");
    let got_b = poll(b.as_mut(), 1, 64).expect("poll b");
    let got_a = poll(a.as_mut(), 1, 64).expect("poll a");
    let again_a = poll(a.as_mut(), 2, 64).expect("poll a again");

    assert_eq!(got_a, to_a, "a gets only what a published");
    assert_eq!(got_b, to_b, "b gets only what b published");
    assert_eq!(again_a, [], "a's queue is drained");
    assert_eq!((a.counters().tx_dropped, b.counters().tx_dropped), (21, 11));
    let (ea, eb) = (env.events(0, "a"), env.events(1, "b"));
    for events in [&ea, &eb] {
        let publishes: Vec<_> = events.iter().filter(|e| e.starts_with("publish")).collect();
        assert!(
            publishes.len() == 1 && publishes[0].contains("tick=1 rate=30 count=3"),
            "one batch of its own three messages each: {events:?}"
        );
    }
    let (ta, tb) = (sink_tokens(&ea), sink_tokens(&eb));
    assert_eq!((ta.len(), tb.len()), (2, 1));
    assert!(
        ta.iter().all(|t| !tb.contains(t)),
        "per-adapter sinks: {ta:?} {tb:?}"
    );
    assert_eq!(env.host.faults(), [], "no fault");
}

#[test]
fn a_sink_of_one_adapter_pushed_during_another_adapters_poll_faults_only_the_culprit() {
    let env = two_images!(PluginFaultPolicy::Abort);
    let ca = env.build(0, "test.net.a", "net_a", "lend", "a", 1000);
    let cb = env.build(1, "test.net.b", "net_b", "borrow", "b", 5000);
    let mut a = start(ca.as_ref());
    let mut b = start(cb.as_ref());
    poll(a.as_mut(), 1, 8).expect("a lends its sink");

    let err = poll(b.as_mut(), 1, 8).expect_err("b pushes through a's sink");

    assert!(
        matches!(&err, NetError::Faulted(m) if m.contains("test.net.b")),
        "{err:?}"
    );
    assert!(
        env.events(1, "b").contains(&"foreign push=6"),
        "refused with UMBRA_ERR_STATE"
    );
    let faults = env.host.faults();
    assert_eq!(faults.len(), 1, "{faults:?}");
    assert_eq!(faults[0].capability.as_str(), "test.net.b");
    assert!(
        faults[0].message.contains("stale or foreign token"),
        "{faults:?}"
    );
    let abort = env.host.take_abort().expect("abort pending");
    assert_eq!(abort.capability.as_str(), "test.net.b");
    a.publish(at(2), &every_kind()).expect("a is untouched");
    assert_eq!(poll(a.as_mut(), 2, 64).expect("a polls"), every_kind());
}

#[test]
fn one_plugins_two_capabilities_and_one_capability_under_two_names_are_independent() {
    let env = net_support::NetImages::load(
        vec![forged!(net_support::parts_for(
            "test.plugin.net_two",
            &["test.net.alpha", "test.net.beta"]
        ))],
        PluginFaultPolicy::Abort,
    );
    let rows = [
        ("test.net.alpha", "first", "f", 1000),
        ("test.net.alpha", "second", "s", 2000),
        ("test.net.beta", "third", "t", 3000),
    ];
    let configs: Vec<_> = rows
        .iter()
        .map(|&(cap, name, tag, base)| env.build(0, cap, name, "echo", tag, base))
        .collect();
    let mut adapters: Vec<_> = configs.iter().map(|c| Some(start(c.as_ref()))).collect();
    let mut configs: Vec<_> = configs.into_iter().map(Some).collect();
    let all = every_kind();
    for (i, &(_, name, tag, base)) in rows.iter().enumerate() {
        let config = configs[i].as_ref().expect("config");
        assert_eq!(config.key_space(), KeySpace { base, count: 100 });
        let first = env.events(0, tag)[0];
        assert!(first.contains(&format!(r#""name":"{name}""#)), "{first}");
        let adapter = adapters[i].as_mut().expect("adapter");
        adapter.publish(at(1), &all[i..=i]).expect("publish");
    }
    for (i, _) in rows.iter().enumerate() {
        let adapter = adapters[i].as_mut().expect("adapter");
        assert_eq!(poll(adapter.as_mut(), 1, 64).expect("poll"), all[i..=i]);
    }

    // Drop the middle one first, then the others; each stops and is destroyed at its own drop.
    for i in [1, 0, 2] {
        let tag = rows[i].2;
        drop(adapters[i].take());
        drop(configs[i].take());
        let events = env.events(0, tag);
        assert_eq!(
            events[events.len() - 3..],
            ["counters n=4 zeroed=true", "stop", "destroy"],
            "{tag}"
        );
        for (j, other) in rows.iter().enumerate() {
            if configs[j].is_some() {
                assert!(
                    !env.events(0, other.2).contains(&"stop"),
                    "{} still runs",
                    other.2
                );
            }
        }
    }
    for &(_, _, tag, _) in &rows {
        let events = env.events(0, tag);
        for call in ["configure", "key_space", "start", "stop", "destroy"] {
            let n = events.iter().filter(|e| e.starts_with(call)).count();
            assert_eq!(n, 1, "{tag}: {call} exactly once in {events:?}");
        }
    }
}

#[test]
fn one_adapter_faulting_leaves_another_plugins_adapter_running() {
    let env = two_images!(PluginFaultPolicy::DisableCapability);
    let ca = env.build(0, "test.net.a", "net_a", "poll_error", "a", 1000);
    let cb = env.build(1, "test.net.b", "net_b", "echo", "b", 5000);
    let mut a = start(ca.as_ref());
    let mut b = start(cb.as_ref());

    assert!(matches!(poll(a.as_mut(), 1, 8), Err(NetError::Faulted(_))));

    for tick in 1..=2 {
        b.publish(at(tick), &every_kind()).expect("b publishes");
        assert_eq!(poll(b.as_mut(), tick, 64).expect("b polls"), every_kind());
    }
    let faults = env.host.faults();
    assert_eq!(faults.len(), 1, "{faults:?}");
    assert_eq!(faults[0].capability.as_str(), "test.net.a");
    assert!(env.host.take_abort().is_none());
}

#[test]
fn with_two_images_each_adapter_is_stopped_and_destroyed_before_its_own_image_is_released() {
    let env = two_images!(PluginFaultPolicy::Abort);
    let ca = env.build(0, "test.net.a", "net_a", "echo", "a", 1000);
    let cb = env.build(1, "test.net.b", "net_b", "echo", "b", 5000);
    let a = start(ca.as_ref());
    let b = start(cb.as_ref());
    let net_support::NetImages {
        host,
        registry,
        images,
    } = env;
    let log = |i: usize| images[i].order.lock().expect("order").clone();

    drop(host);
    drop(registry);
    assert!(!log(0).contains(&"shutdown") && !log(1).contains(&"shutdown"));
    drop(a);
    drop(ca);
    let (l0, l1) = (log(0), log(1));
    assert_eq!(l0[l0.len() - 3..], ["a stop", "a destroy", "shutdown"]);
    assert!(
        !l1.contains(&"shutdown") && !l1.contains(&"b stop"),
        "image b untouched: {l1:?}"
    );
    drop(b);
    drop(cb);
    let l1 = log(1);
    assert_eq!(l1[l1.len() - 3..], ["b stop", "b destroy", "shutdown"]);
}
