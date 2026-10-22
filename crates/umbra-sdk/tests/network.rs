//! AT-17 (M6 WI-12): network adapters built with `umbra-sdk` (REQ-SDK-1, REQ-SDK-3, REQ-P-7, ADR 0033,
//! ASM-245, ASM-411..413). The plugin of this binary exports one behavior and two network adapters
//! with `export_plugin!`; the tests play the host through the raw ABI (`net_harness`).
//!
//! Covered: the descriptor and vtable minors (descriptor 1, network capability and vtable 1, behavior
//! capability and vtable still 0); the lifecycle through the vtable (configure with the ADR 0030
//! wrapper, key space, start, publish, poll, counters, stop, destroy) and its misuse; every exported
//! network function panic-guarded; malformed host input refused with `UMBRA_ERR_INVALID_ARGUMENT`
//! before any trait call; pushed messages checked, capped at `max` (and `UMBRA_MAX_NET_POLL`) and
//! written in minor 1; a host refusal ending the `poll` with the host's code.
#![expect(unsafe_code, reason = "plays the host side of the C ABI")]
#![expect(clippy::expect_used, reason = "the test harness plays the C side")]

mod net_harness;

use std::cell::RefCell;
use std::mem::size_of;

use net_harness::{
    Handle, blank, capabilities, configure, configure_raw, counters, destroy, host_api, key_space,
    logs, net_sink, net_vtable, poll, poll_with, publish, publish_batch, push_calls, pushed,
    refuse_from, reset, s, start, stop, vec3,
};
use umbra_sdk::abi::{
    UMBRA_ABI_VERSION, UMBRA_ABI_VERSION_MAJOR, UMBRA_CAPABILITY_BEHAVIOR,
    UMBRA_CAPABILITY_NETWORK_ADAPTER, UMBRA_DR_VELOCITY, UMBRA_ERR_INTERNAL,
    UMBRA_ERR_INVALID_ARGUMENT, UMBRA_ERR_PANIC, UMBRA_ERR_PARAMS, UMBRA_ERR_STATE,
    UMBRA_LOG_ERROR, UMBRA_MAX_ID_LEN, UMBRA_MAX_NET_POLL, UMBRA_NET_CONTROL,
    UMBRA_NET_CONTROL_PAUSE, UMBRA_NET_DETONATION, UMBRA_NET_ENTITY_REMOVED,
    UMBRA_NET_ENTITY_STATE, UMBRA_NET_FIRE, UMBRA_OK, UmbraBehaviorVTable, UmbraCapability,
    UmbraNetMessage, UmbraNetSink, UmbraNetworkVTable, UmbraPluginDescriptor,
};
use umbra_sdk::{
    Behavior, BehaviorFactory, BehaviorInstance, ConfigureInput, ControlAction, Counters,
    DeadReckoning, EntityState, EntityView, Error, Euler, Inbox, Intents, KeySpace, NetKey,
    NetMessage, NetworkAdapter, NetworkAdapterConfig, NetworkAdapterFactory, Params, SimTime,
    Status, TickContext, export_plugin,
};

// ---------------------------------------------------------------- the plugin

thread_local! {
    /// Every trait call of this thread, in order.
    static CALLS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    /// What `Inbox::push` returned to the adapter, in order (`Err` holds the code).
    static RESULTS: RefCell<Vec<Result<(), i32>>> = const { RefCell::new(Vec::new()) };
}

fn call(what: impl Into<String>) {
    CALLS.with(|c| c.borrow_mut().push(what.into()));
}

fn calls() -> Vec<String> {
    CALLS.with(|c| c.borrow().clone())
}

fn results() -> Vec<Result<(), i32>> {
    RESULTS.with(|r| r.borrow().clone())
}

fn clear() {
    reset();
    CALLS.with(|c| c.borrow_mut().clear());
    RESULTS.with(|r| r.borrow_mut().clear());
}

/// Every message kind as a plugin pushes it (strings `'static`).
fn every_kind() -> Vec<NetMessage<'static>> {
    vec![
        NetMessage::EntityState(EntityState {
            key: NetKey(5001),
            entity_type: Some("tank"),
            force: Some("red"),
            name: Some("Bravo 2"),
            position: [3_900_000.0, 300_000.0, 5_000_000.0],
            velocity: [1.0, 2.0, 3.0],
            acceleration: [0.5, 0.0, 0.0],
            angular_velocity: [0.0, 0.0, 0.1],
            orientation: Euler {
                yaw: 1.0,
                pitch: -0.5,
                roll: 0.25,
            },
            dead_reckoning: DeadReckoning::Velocity,
        }),
        NetMessage::EntityRemoved { key: NetKey(5002) },
        NetMessage::Fire {
            shooter: NetKey(5001),
            target: Some(NetKey(17)),
            weapon: "example.weapon.cannon",
            position: [1.0, 2.0, 3.0],
        },
        NetMessage::Detonation {
            shooter: None,
            target: None,
            position: [4.0, 5.0, 6.0],
        },
        NetMessage::SimulationControl(ControlAction::Pause),
    ]
}

/// Where the adapter panics or fails (`"panic"` param) and what its `poll` pushes (`"push"`).
struct NetFactory;

struct NetConfig {
    keys: KeySpace,
    panic: String,
    push: String,
}

struct NetAdapter {
    panic: String,
    push: String,
}

impl NetworkAdapterFactory for NetFactory {
    const ID: &'static str = "test.network.sdk_net";
    const PARAM_SCHEMA: &'static str =
        r#"{"type":"object","description":"SDK test network adapter"}"#;
    type Config = NetConfig;

    fn configure(input: &ConfigureInput) -> Result<NetConfig, Error> {
        let p = input.params();
        let text = |k: &str| p.get(k).and_then(|v| v.as_str()).unwrap_or("").to_owned();
        let panic = text("panic");
        call(format!("configure {}", input.name()));
        assert!(panic != "configure", "configure exploded");
        if p.get("bad").is_some() {
            return Err(Error::params("params: `bad` is not allowed"));
        }
        let base = p
            .get("base")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(1000);
        Ok(NetConfig {
            keys: KeySpace::new(base, 100)?,
            panic,
            push: text("push"),
        })
    }
}

impl NetworkAdapterConfig for NetConfig {
    type Adapter = NetAdapter;

    fn key_space(&self) -> KeySpace {
        assert!(self.panic != "key_space", "key_space exploded");
        self.keys
    }

    fn start(&mut self, now: SimTime) -> Result<NetAdapter, Error> {
        call(format!("start {}@{}", now.tick(), now.rate_hz()));
        match self.panic.as_str() {
            "start" => panic!("start exploded"),
            "start_error" => Err(Error::new("port in use")),
            _ => Ok(NetAdapter {
                panic: self.panic.clone(),
                push: self.push.clone(),
            }),
        }
    }
}

impl Drop for NetConfig {
    fn drop(&mut self) {
        call("drop config");
    }
}

impl NetAdapter {
    fn record(result: Result<(), Error>) {
        RESULTS.with(|r| r.borrow_mut().push(result.map_err(|e| e.code())));
    }
}

impl NetworkAdapter for NetAdapter {
    fn publish(&mut self, now: SimTime, out: &[NetMessage<'_>]) -> Result<(), Error> {
        assert!(self.panic != "publish", "publish exploded");
        call(format!("publish {} at {}", out.len(), now.tick()));
        for m in out {
            call(format!("{m:?}"));
        }
        Ok(())
    }

    fn poll(&mut self, now: SimTime, inbox: &mut Inbox<'_>) -> Result<(), Error> {
        assert!(self.panic != "poll", "poll exploded");
        call(format!(
            "poll at {}@{} remaining {}",
            now.tick(),
            now.rate_hz(),
            inbox.remaining()
        ));
        match self.push.as_str() {
            "every_kind" => {
                for m in every_kind() {
                    inbox.push(&m)?;
                }
            }
            "invalid" => {
                Self::record(inbox.push(&NetMessage::EntityRemoved { key: NetKey(0) }));
                Self::record(inbox.push(&NetMessage::EntityRemoved { key: NetKey(7) }));
            }
            // Bounded, so a missing cap fails the test instead of exhausting memory.
            "flood" => {
                for _ in 0..2 * UMBRA_MAX_NET_POLL {
                    let r = inbox.push(&NetMessage::EntityRemoved { key: NetKey(7) });
                    let stop = r.is_err();
                    Self::record(r);
                    if stop {
                        break;
                    }
                }
            }
            "ignore_refusal" => {
                for key in 1..=3 {
                    Self::record(inbox.push(&NetMessage::EntityRemoved { key: NetKey(key) }));
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn counters(&self) -> Counters {
        assert!(self.panic != "counters", "counters exploded");
        let mut c = Counters::default();
        c.tx_dropped = 1;
        c.rx_dropped = 2;
        c.decode_errors = 3;
        c.io_errors = 4;
        c
    }

    fn stop(&mut self) {
        call("stop");
        assert!(
            self.panic != "stop" && self.panic != "stop_and_drop",
            "stop exploded"
        );
    }
}

impl Drop for NetAdapter {
    fn drop(&mut self) {
        call("drop adapter");
        assert!(
            self.panic != "drop" && self.panic != "stop_and_drop",
            "drop exploded"
        );
    }
}

/// A second network capability (another handle type).
struct SecondFactory;
struct SecondConfig;
struct SecondAdapter;

impl NetworkAdapterFactory for SecondFactory {
    const ID: &'static str = "test.network.sdk_net_second";
    const PARAM_SCHEMA: &'static str = r#"{"type":"object"}"#;
    const VERSION: Option<&'static str> = Some("2.0.0");
    type Config = SecondConfig;
    fn configure(_: &ConfigureInput) -> Result<SecondConfig, Error> {
        Ok(SecondConfig)
    }
}

impl NetworkAdapterConfig for SecondConfig {
    type Adapter = SecondAdapter;
    fn key_space(&self) -> KeySpace {
        KeySpace::new(0, 1).expect("one key")
    }
    fn start(&mut self, _: SimTime) -> Result<SecondAdapter, Error> {
        Ok(SecondAdapter)
    }
}

impl NetworkAdapter for SecondAdapter {
    fn publish(&mut self, _: SimTime, _: &[NetMessage<'_>]) -> Result<(), Error> {
        Ok(())
    }
    fn poll(&mut self, _: SimTime, _: &mut Inbox<'_>) -> Result<(), Error> {
        Ok(())
    }
    fn stop(&mut self) {}
}

struct BehFactory;
struct Beh;
struct BehInstance;

impl BehaviorFactory for BehFactory {
    const ID: &'static str = "test.behavior.sdk_net_neighbour";
    const PARAM_SCHEMA: &'static str = r#"{"type":"object"}"#;
    type Behavior = Beh;
    fn configure(_: &Params<'_>) -> Result<Beh, Error> {
        Ok(Beh)
    }
}

impl Behavior for Beh {
    type Instance = BehInstance;
    fn instantiate(&self, _: u32) -> Result<BehInstance, Error> {
        Ok(BehInstance)
    }
}

impl BehaviorInstance for BehInstance {
    fn tick(&mut self, _: &mut TickContext<'_>, _: &EntityView<'_>, _: &mut Intents<'_>) -> Status {
        Status::Running
    }
    fn halt(&mut self) {}
}

export_plugin! {
    id: "test.plugin.sdk_net",
    name: "SDK network test plugin",
    version: "0.2.0",
    behaviors: [BehFactory],
    network_adapters: [NetFactory, SecondFactory],
}

// ---------------------------------------------------------------- helpers

/// One edit of an invalid-message row.
type Edit<'a> = Box<dyn Fn(&mut UmbraNetMessage) + 'a>;

fn descriptor() -> &'static UmbraPluginDescriptor {
    let host = Box::leak(Box::new(host_api()));
    // SAFETY: `host` is a leaked, well-formed host API (process lifetime).
    let ptr = unsafe { umbra_plugin_entry(host) };
    assert!(!ptr.is_null(), "entry returned null");
    // SAFETY: the SDK's descriptor is a process-lifetime static.
    unsafe { &*ptr }
}

fn cap(i: usize) -> &'static UmbraCapability {
    &capabilities(descriptor())[i]
}

fn vt() -> &'static UmbraNetworkVTable {
    net_vtable(cap(1))
}

fn second() -> &'static UmbraNetworkVTable {
    net_vtable(cap(2))
}

const fn minor(abi_version: u32) -> u32 {
    abi_version & 0xFFFF
}

/// A configured and started adapter of `test.network.sdk_net` with `params`.
fn started(params: &str) -> Handle {
    let (code, h, err) = configure(vt(), params);
    assert_eq!(code, UMBRA_OK, "{err:?}");
    let (code, err) = start(vt(), h, 30);
    assert_eq!(code, UMBRA_OK, "{err:?}");
    h
}

/// The host-written form of every kind (as `every_kind`), strings borrowed from `'static` text.
fn host_every_kind() -> Vec<UmbraNetMessage> {
    let mut state = blank(UMBRA_NET_ENTITY_STATE);
    state.key = 42;
    state.dead_reckoning = UMBRA_DR_VELOCITY;
    state.position = vec3(1.0, 2.0, 3.0);
    state.orientation = vec3(0.5, -0.25, 1.5);
    state.entity_type = s("1:2:225:1:1:0:0");
    state.force = s("blue");
    let mut removed = blank(UMBRA_NET_ENTITY_REMOVED);
    removed.key = 7;
    let mut fire = blank(UMBRA_NET_FIRE);
    fire.key = 1;
    fire.other_key = 2;
    fire.weapon = s("example.weapon.cannon");
    let mut det = blank(UMBRA_NET_DETONATION);
    det.other_key = 3;
    det.position = vec3(-1.0, 0.0, 1.0);
    let mut control = blank(UMBRA_NET_CONTROL);
    control.control = UMBRA_NET_CONTROL_PAUSE;
    vec![state, removed, fire, det, control]
}

// ---------------------------------------------------------------- AT-17: minors

#[test]
fn export_plugin_with_a_network_adapter_declares_minor_1_and_its_vtable_minor_1() {
    let d = descriptor();
    assert_eq!(
        minor(d.abi_version),
        1,
        "the descriptor declares the network kind's minor"
    );
    assert_eq!(d.abi_version >> 16, UMBRA_ABI_VERSION_MAJOR);
    let caps = capabilities(d);
    assert_eq!(caps.len(), 3, "behaviors first, then the network adapters");

    let behavior = &caps[0];
    assert_eq!(behavior.kind, UMBRA_CAPABILITY_BEHAVIOR);
    assert_eq!(
        minor(behavior.abi_version),
        0,
        "the behavior stays at minor 0"
    );
    // SAFETY: capability 0 is a behavior capability; its vtable is a static of the descriptor.
    let bvt = unsafe { &*behavior.vtable.cast::<UmbraBehaviorVTable>() };
    assert_eq!(minor(bvt.abi_version), 0);

    for (i, id, version) in [
        (1, "test.network.sdk_net", ""),
        (2, "test.network.sdk_net_second", "2.0.0"),
    ] {
        let c = &caps[i];
        assert_eq!(c.kind, UMBRA_CAPABILITY_NETWORK_ADAPTER, "{id}");
        assert_eq!(net_harness::text(c.id), id);
        assert_eq!(net_harness::text(c.version), version);
        assert_eq!(minor(c.abi_version), 1, "{id}: capability minor");
        assert_eq!(c.struct_size as usize, size_of::<UmbraCapability>());
        assert_eq!(
            net_harness::text(c.metadata),
            "",
            "ABI 0.1 defines no network metadata"
        );
        let v = net_vtable(c);
        assert_eq!(
            v.abi_version,
            (UMBRA_ABI_VERSION & 0xFFFF_0000) | 1,
            "{id}: vtable minor 1"
        );
        assert_eq!(v.struct_size as usize, size_of::<UmbraNetworkVTable>());
        assert!(v.configure.is_some() && v.destroy.is_some() && v.key_space.is_some());
        assert!(v.start.is_some() && v.publish.is_some() && v.poll.is_some());
        assert!(
            v.stop.is_some() && v.counters.is_some(),
            "every entry is filled"
        );
    }
}

// ---------------------------------------------------------------- the lifecycle

#[test]
fn the_lifecycle_runs_through_the_vtable_in_order() {
    clear();
    let (code, h, err) = configure(vt(), r#"{"base":5000}"#);
    assert_eq!((code, err), (UMBRA_OK, None));
    assert_eq!(key_space(vt(), h), (UMBRA_OK, 5000, 100));
    assert_eq!(
        counters(vt(), h, 4),
        (UMBRA_OK, vec![0; 4]),
        "nothing before start"
    );
    assert_eq!(start(vt(), h, 30), (UMBRA_OK, None));
    assert_eq!(publish(vt(), h, &[]), (UMBRA_OK, None));
    assert_eq!(poll(vt(), h, 16), (UMBRA_OK, None));
    assert_eq!(
        counters(vt(), h, 4),
        (UMBRA_OK, vec![1, 2, 3, 4]),
        "the ABI order"
    );
    assert_eq!(counters(vt(), h, 2), (UMBRA_OK, vec![1, 2]), "at most n");
    assert_eq!(
        counters(vt(), h, 8),
        (UMBRA_OK, vec![1, 2, 3, 4, 0, 0, 0, 0]),
        "at most 4"
    );
    assert_eq!(stop(vt(), h), UMBRA_OK);
    assert_eq!(stop(vt(), h), UMBRA_OK, "stop is idempotent");
    destroy(vt(), h);

    assert_eq!(
        calls(),
        [
            "configure net",
            "start 0@30",
            "publish 0 at 7",
            "poll at 8@30 remaining 16",
            "stop",
            "drop adapter",
            "drop config",
        ],
        "the adapter's stop runs once, the adapter drops before its configuration"
    );
}

#[test]
fn destroy_without_stop_stops_the_adapter_first_and_a_dry_run_only_drops_the_config() {
    clear();
    let h = started("{}");
    destroy(vt(), h);
    assert_eq!(
        calls(),
        [
            "configure net",
            "start 0@30",
            "stop",
            "drop adapter",
            "drop config"
        ]
    );

    clear();
    let (_, h, _) = configure(vt(), "{}");
    assert_eq!(stop(vt(), h), UMBRA_OK, "stop without start is valid");
    destroy(vt(), h);
    assert_eq!(calls(), ["configure net", "drop config"]);
}

#[test]
fn lifecycle_misuse_is_refused_without_a_trait_call() {
    clear();
    let (_, h, _) = configure(vt(), "{}");
    let (code, err) = publish(vt(), h, &host_every_kind());
    assert_eq!(code, UMBRA_ERR_STATE);
    assert!(err.is_some_and(|e| e.contains("not started")));
    assert_eq!(poll(vt(), h, 4).0, UMBRA_ERR_STATE, "poll before start");
    assert_eq!(start(vt(), h, 30).0, UMBRA_OK);
    let (code, err) = start(vt(), h, 30);
    assert_eq!(code, UMBRA_ERR_STATE, "a second start");
    assert!(err.is_some_and(|e| e.contains("at most once")));
    assert_eq!(stop(vt(), h), UMBRA_OK);
    assert_eq!(
        publish(vt(), h, &[]).0,
        UMBRA_ERR_STATE,
        "publish after stop"
    );
    let (code, err) = poll(vt(), h, 4);
    assert_eq!(code, UMBRA_ERR_STATE, "poll after stop");
    assert!(err.is_some_and(|e| e.contains("after stop")));
    assert_eq!(start(vt(), h, 30).0, UMBRA_ERR_STATE, "start after stop");
    destroy(vt(), h);
    assert_eq!(
        calls(),
        [
            "configure net",
            "start 0@30",
            "stop",
            "drop adapter",
            "drop config"
        ],
        "no trait call for a refused one"
    );

    clear();
    let (_, h, _) = configure(vt(), r#"{"panic":"start_error"}"#);
    let (code, err) = start(vt(), h, 30);
    assert_eq!(
        code, UMBRA_ERR_INTERNAL,
        "a start error is the plugin's code"
    );
    assert!(err.is_some_and(|e| e.contains("port in use")));
    assert_eq!(poll(vt(), h, 4).0, UMBRA_ERR_STATE, "never started");
    destroy(vt(), h);
}

#[test]
fn configure_reads_the_adr_0030_wrapper_and_refuses_anything_else() {
    clear();
    let (code, h, err) = configure(vt(), r#"{"bad":1}"#);
    assert_eq!(code, UMBRA_ERR_PARAMS, "a params error keeps its code");
    assert!(h.is_null());
    assert!(err.is_some_and(|e| e.contains("`bad` is not allowed")));

    for (json, needle) in [
        ("[1]", "not an object"),
        (r#"{"params":{}}"#, "no string `name`"),
        (r#"{"name":"x"}"#, "no `params`"),
        (r#"{"name":"x","params":{},"templates":1}"#, "`templates`"),
        ("{", "not JSON"),
    ] {
        let (code, h, err) = configure_raw(vt(), json);
        assert_eq!(code, UMBRA_ERR_INVALID_ARGUMENT, "{json}");
        assert!(h.is_null(), "{json}");
        assert!(
            err.as_deref().is_some_and(|e| e.contains(needle)),
            "{json}: {err:?}"
        );
    }
    assert_eq!(
        calls(),
        ["configure net"],
        "the wrapper is checked before the factory runs"
    );

    let bytes = [0xFF_u8, 0xFE];
    let err = net_harness::err_sink();
    let mut out: Handle = std::ptr::null_mut();
    let raw = umbra_sdk::abi::UmbraStr {
        ptr: bytes.as_ptr(),
        len: 2,
    };
    // SAFETY: a live 2-byte array, a valid out pointer and error sink.
    let code = unsafe { vt().configure.expect("configure")(raw, &raw mut out, &raw const err) };
    assert_eq!(code, UMBRA_ERR_INVALID_ARGUMENT, "non-UTF-8 params");
    // SAFETY: a null out pointer is refused before any write.
    let code = unsafe {
        vt().configure.expect("configure")(s("{}"), std::ptr::null_mut(), &raw const err)
    };
    assert_eq!(code, UMBRA_ERR_INVALID_ARGUMENT, "null out pointer");
}

// ---------------------------------------------------------------- messages

#[test]
fn published_messages_reach_the_adapter_converted_and_complete() {
    clear();
    let h = started("{}");
    assert_eq!(publish(vt(), h, &host_every_kind()), (UMBRA_OK, None));
    destroy(vt(), h);

    let seen = calls();
    let want = [
        NetMessage::EntityState(EntityState {
            key: NetKey(42),
            entity_type: Some("1:2:225:1:1:0:0"),
            force: Some("blue"),
            name: None,
            position: [1.0, 2.0, 3.0],
            velocity: [0.0; 3],
            acceleration: [0.0; 3],
            angular_velocity: [0.0; 3],
            orientation: Euler {
                yaw: 0.5,
                pitch: -0.25,
                roll: 1.5,
            },
            dead_reckoning: DeadReckoning::Velocity,
        }),
        NetMessage::EntityRemoved { key: NetKey(7) },
        NetMessage::Fire {
            shooter: NetKey(1),
            target: Some(NetKey(2)),
            weapon: "example.weapon.cannon",
            position: [0.0; 3],
        },
        NetMessage::Detonation {
            shooter: None,
            target: Some(NetKey(3)),
            position: [-1.0, 0.0, 1.0],
        },
        NetMessage::SimulationControl(ControlAction::Pause),
    ];
    assert_eq!(seen[2], "publish 5 at 7");
    let got: Vec<&str> = seen[3..8].iter().map(String::as_str).collect();
    let want: Vec<String> = want.iter().map(|m| format!("{m:?}")).collect();
    assert_eq!(got, want);
}

#[test]
fn malformed_host_input_is_invalid_argument_before_any_trait_call() {
    clear();
    let h = started("{}");
    let long = "x".repeat(UMBRA_MAX_ID_LEN + 1);
    let bad_utf8 = [0xC3_u8, 0x28];
    let rows: Vec<(&str, Edit<'_>)> = vec![
        ("batch.messages[0].kind", Box::new(|m| m.kind = 9)),
        (
            "batch.messages[0].dead_reckoning",
            Box::new(|m| m.dead_reckoning = 0),
        ),
        (
            "batch.messages[0]: ENTITY_STATE key is 0",
            Box::new(|m| m.key = 0),
        ),
        (
            "batch.messages[0]: ENTITY_STATE velocity is not finite",
            Box::new(|m| m.velocity.y = f64::NAN),
        ),
        (
            "batch.messages[0].name is 129 bytes",
            Box::new(|m| m.name = s(&long)),
        ),
        (
            "batch.messages[0].force: malformed",
            Box::new(|m| {
                m.force = umbra_sdk::abi::UmbraStr {
                    ptr: bad_utf8.as_ptr(),
                    len: 2,
                };
            }),
        ),
        (
            "batch.messages[0]: malformed (header)",
            Box::new(|m| m.struct_size += 8),
        ),
        (
            "batch.messages[0]: malformed (header)",
            Box::new(|m| m.abi_version = (1 << 16) | 1),
        ),
    ];
    let before = calls().len();
    for (needle, edit) in rows {
        let mut messages = host_every_kind();
        edit(&mut messages[0]);
        let (code, err) = publish(vt(), h, &messages);
        assert_eq!(code, UMBRA_ERR_INVALID_ARGUMENT, "{needle}");
        assert!(
            err.as_deref().is_some_and(|e| e.contains(needle)),
            "{needle}: {err:?}"
        );
    }
    let mut fire = host_every_kind();
    fire[2].weapon = s("");
    let (code, err) = publish(vt(), h, &fire);
    assert_eq!(code, UMBRA_ERR_INVALID_ARGUMENT);
    assert!(err.is_some_and(|e| e.contains("batch.messages[2]: FIRE weapon is empty")));
    let (code, _) = publish_batch(vt(), h, std::ptr::null());
    assert_eq!(code, UMBRA_ERR_INVALID_ARGUMENT, "null batch");
    let mut b = net_harness::batch(&[], 1);
    b.rate_hz = 0;
    assert_eq!(
        publish_batch(vt(), h, &raw const b).0,
        UMBRA_ERR_INVALID_ARGUMENT,
        "rate 0"
    );
    assert_eq!(calls().len(), before, "no trait call for malformed input");
    destroy(vt(), h);
}

#[test]
fn a_malformed_sink_out_pointer_handle_or_rate_is_invalid_argument_before_any_trait_call() {
    clear();
    let h = started("{}");
    let before = calls().len();

    let mut no_push = net_sink();
    no_push.push = None;
    assert_eq!(
        poll_with(vt(), h, 4, &raw const no_push).0,
        UMBRA_ERR_INVALID_ARGUMENT
    );
    assert_eq!(
        poll_with(vt(), h, 4, std::ptr::null()).0,
        UMBRA_ERR_INVALID_ARGUMENT
    );
    let mut short = net_sink();
    short.struct_size -= 8;
    assert_eq!(
        poll_with(vt(), h, 4, &raw const short).0,
        UMBRA_ERR_INVALID_ARGUMENT
    );
    let mut old: UmbraNetSink = net_sink();
    old.abi_version = UMBRA_ABI_VERSION_MAJOR << 16;
    assert_eq!(
        poll_with(vt(), h, 4, &raw const old).0,
        UMBRA_ERR_INVALID_ARGUMENT,
        "a network struct declaring minor 0 (older than the struct)"
    );
    assert_eq!(calls().len(), before, "no trait call for a malformed sink");

    // Out pointers and foreign handles.
    // SAFETY: null out pointers are refused before any write.
    let code = unsafe {
        vt().key_space.expect("key_space")(h, std::ptr::null_mut(), std::ptr::null_mut())
    };
    assert_eq!(code, UMBRA_ERR_INVALID_ARGUMENT);
    // SAFETY: a null `out` with `n > 0` is refused before any write.
    let code = unsafe { vt().counters.expect("counters")(h, std::ptr::null_mut(), 4) };
    assert_eq!(code, UMBRA_ERR_INVALID_ARGUMENT);
    let (_, foreign, _) = configure(second(), "{}");
    assert_eq!(
        key_space(vt(), foreign).0,
        UMBRA_ERR_INVALID_ARGUMENT,
        "another capability's handle"
    );
    assert_eq!(start(vt(), foreign, 30).0, UMBRA_ERR_INVALID_ARGUMENT);
    assert_eq!(poll(vt(), foreign, 4).0, UMBRA_ERR_INVALID_ARGUMENT);
    assert_eq!(stop(vt(), foreign), UMBRA_ERR_INVALID_ARGUMENT);
    assert_eq!(
        key_space(vt(), std::ptr::null_mut()).0,
        UMBRA_ERR_INVALID_ARGUMENT,
        "null handle"
    );
    destroy(second(), foreign);
    let (_, fresh, _) = configure(vt(), "{}");
    let (code, err) = start(vt(), fresh, 0);
    assert_eq!(code, UMBRA_ERR_INVALID_ARGUMENT, "rate 0 at start");
    assert!(err.is_some_and(|e| e.contains("start.rate_hz")));
    destroy(vt(), fresh);
    destroy(vt(), h);
}

#[test]
fn pushed_messages_are_written_in_minor_1_with_unused_fields_zero() {
    clear();
    let h = started(r#"{"push":"every_kind"}"#);
    assert_eq!(poll(vt(), h, 16), (UMBRA_OK, None));
    destroy(vt(), h);

    let got = pushed();
    assert_eq!(got.len(), 5);
    for p in &got {
        assert_eq!(
            p.abi_version,
            (UMBRA_ABI_VERSION_MAJOR << 16) | 1,
            "minor 1"
        );
        assert_eq!(p.struct_size as usize, size_of::<UmbraNetMessage>());
    }
    let state = &got[0];
    assert_eq!(
        (
            state.kind,
            state.key,
            state.dead_reckoning,
            state.entity_type.as_str(),
            state.force.as_str(),
            state.name.as_str()
        ),
        (
            UMBRA_NET_ENTITY_STATE,
            5001,
            UMBRA_DR_VELOCITY,
            "tank",
            "red",
            "Bravo 2"
        )
    );
    assert_eq!(
        state.orientation,
        [1.0, -0.5, 0.25],
        "x = yaw, y = pitch, z = roll"
    );
    assert_eq!(
        (state.weapon.as_str(), state.other_key, state.control),
        ("", 0, 0)
    );
    assert_eq!(
        (got[1].kind, got[1].key, got[1].position),
        (UMBRA_NET_ENTITY_REMOVED, 5002, [0.0; 3])
    );
    assert_eq!(
        (
            got[2].kind,
            got[2].key,
            got[2].other_key,
            got[2].weapon.as_str()
        ),
        (UMBRA_NET_FIRE, 5001, 17, "example.weapon.cannon")
    );
    assert_eq!((got[2].force.as_str(), got[2].dead_reckoning), ("", 0));
    assert_eq!(
        (got[3].kind, got[3].key, got[3].other_key),
        (UMBRA_NET_DETONATION, 0, 0),
        "none is 0"
    );
    assert_eq!(
        (got[4].kind, got[4].control),
        (UMBRA_NET_CONTROL, UMBRA_NET_CONTROL_PAUSE)
    );
}

#[test]
fn an_invalid_pushed_message_is_refused_before_the_host_and_faults_nothing() {
    clear();
    let h = started(r#"{"push":"invalid"}"#);
    assert_eq!(poll(vt(), h, 16), (UMBRA_OK, None));
    destroy(vt(), h);

    assert_eq!(results(), [Err(UMBRA_ERR_INVALID_ARGUMENT), Ok(())]);
    assert_eq!(push_calls(), 1, "only the valid message reached the host");
    assert_eq!(pushed()[0].key, 7);
}

#[test]
fn at_most_max_messages_reach_the_host_and_the_abi_cap_applies() {
    clear();
    let h = started(r#"{"push":"flood"}"#);
    assert_eq!(poll(vt(), h, 2), (UMBRA_OK, None));
    assert_eq!(
        results(),
        [Ok(()), Ok(()), Err(UMBRA_ERR_STATE)],
        "full after max"
    );
    assert_eq!(push_calls(), 2, "the refused push never reached the host");

    clear();
    assert_eq!(poll(vt(), h, usize::MAX), (UMBRA_OK, None));
    assert_eq!(
        push_calls(),
        UMBRA_MAX_NET_POLL,
        "max is capped by UMBRA_MAX_NET_POLL"
    );
    assert!(calls()[0].ends_with(&format!("remaining {UMBRA_MAX_NET_POLL}")));
    destroy(vt(), h);
}

#[test]
fn a_host_refusal_ends_the_poll_with_the_hosts_code_whatever_the_adapter_returns() {
    clear();
    let h = started(r#"{"push":"ignore_refusal"}"#);
    refuse_from(1, UMBRA_ERR_INVALID_ARGUMENT);

    let (code, err) = poll(vt(), h, 16);

    assert_eq!(
        code, UMBRA_ERR_INVALID_ARGUMENT,
        "the host's code, though the adapter returned Ok"
    );
    assert!(err.is_some_and(|e| e.contains("message push refused by the host")));
    assert_eq!(
        results(),
        [
            Ok(()),
            Err(UMBRA_ERR_INVALID_ARGUMENT),
            Err(UMBRA_ERR_INVALID_ARGUMENT)
        ]
    );
    assert_eq!(
        push_calls(),
        2,
        "after a refusal no further push reaches the host"
    );
    destroy(vt(), h);
}

// ---------------------------------------------------------------- panic guards

#[test]
fn every_exported_network_function_is_panic_guarded() {
    // Calls with an error sink report the panic there.
    for (panic, run) in [
        (
            "configure",
            (|| configure(vt(), r#"{"panic":"configure"}"#).0) as fn() -> i32,
        ),
        ("start", || {
            let (_, h, _) = configure(vt(), r#"{"panic":"start"}"#);
            let code = start(vt(), h, 30).0;
            destroy(vt(), h);
            code
        }),
        ("publish", || {
            let h = started(r#"{"panic":"publish"}"#);
            let code = publish(vt(), h, &[]).0;
            destroy(vt(), h);
            code
        }),
        ("poll", || {
            let h = started(r#"{"panic":"poll"}"#);
            let code = poll(vt(), h, 4).0;
            destroy(vt(), h);
            code
        }),
    ] {
        clear();
        net_harness::take_err();
        assert_eq!(run(), UMBRA_ERR_PANIC, "{panic}");
    }

    // The error message of a guarded call with a sink.
    clear();
    let (code, _, err) = configure(vt(), r#"{"panic":"configure"}"#);
    assert_eq!(code, UMBRA_ERR_PANIC);
    assert!(err.is_some_and(|e| e.contains("configure exploded")));
    let h = started(r#"{"panic":"poll"}"#);
    let (code, err) = poll(vt(), h, 4);
    assert_eq!(code, UMBRA_ERR_PANIC);
    assert!(err.is_some_and(|e| e.contains("poll exploded")));
    destroy(vt(), h);

    // Calls without an error sink log the panic at error level through the host API.
    descriptor(); // the entry stored the host API
    for (panic, needle) in [
        (
            "key_space",
            "key_space: plugin panicked: key_space exploded",
        ),
        ("counters", "counters: plugin panicked: counters exploded"),
        ("stop", "stop: plugin panicked: stop exploded"),
        ("drop", "destroy: plugin panicked: drop exploded"),
    ] {
        clear();
        let (_, h, _) = configure(vt(), &format!(r#"{{"panic":"{panic}"}}"#));
        let code = match panic {
            "key_space" => key_space(vt(), h).0,
            "counters" => {
                start(vt(), h, 30);
                counters(vt(), h, 4).0
            }
            "stop" => {
                start(vt(), h, 30);
                stop(vt(), h)
            }
            _ => {
                start(vt(), h, 30);
                destroy(vt(), h);
                UMBRA_ERR_PANIC
            }
        };
        assert_eq!(code, UMBRA_ERR_PANIC, "{panic}");
        if panic != "drop" {
            destroy(vt(), h);
        }
        assert!(
            logs()
                .iter()
                .any(|(level, text)| *level == UMBRA_LOG_ERROR && text.contains(needle)),
            "{panic}: {:?}",
            logs()
        );
    }
}

/// Review fix (WI-12): `destroy` of a started adapter whose `stop` and `Drop` both panic returns
/// cleanly: `stop` runs under its own guard before the drop, so the drop's panic is the only one in
/// flight (inside a `Drop`, the second panic would abort the test process).
#[test]
fn destroy_survives_a_panicking_stop_followed_by_a_panicking_drop() {
    clear();
    descriptor(); // the entry stored the host API, so the panics are logged
    let h = started(r#"{"panic":"stop_and_drop"}"#);

    destroy(vt(), h);

    assert_eq!(
        calls(),
        [
            "configure net",
            "start 0@30",
            "stop",
            "drop adapter",
            "drop config"
        ],
        "stopped once, then everything dropped"
    );
    let errors: Vec<String> = logs()
        .into_iter()
        .filter(|(level, _)| *level == UMBRA_LOG_ERROR)
        .map(|(_, t)| t)
        .collect();
    assert_eq!(
        errors,
        [
            "destroy: plugin panicked: stop exploded",
            "destroy: plugin panicked: drop exploded"
        ]
    );
}
