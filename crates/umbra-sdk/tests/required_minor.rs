//! AT-17 (part, M6 WI-10): the required-minor rule of ADR 0033 (clarified 2026-10-21) for a
//! behavior-only plugin. Every struct the SDK writes (descriptor, capabilities, vtables, intents,
//! values) declares a minor no newer than the descriptor's, and the descriptor of a behavior-only
//! plugin declares minor 0, so the plugin runs in an ABI 0.0 host. WI-12 adds the network kind rows.
#![expect(unsafe_code, reason = "plays the host side of the C ABI")]
#![expect(
    clippy::borrow_as_ptr,
    clippy::cast_possible_truncation,
    clippy::expect_used,
    reason = "test harness casts small ABI sizes and plays the C side"
)]

mod support;

use std::cell::RefCell;
use std::mem::size_of;

use support::{Rig, capability, descriptor, err_sink, pushed, reset, s, vtable};
use umbra_sdk::abi::{
    UMBRA_ABI_VERSION_MAJOR, UMBRA_OK, UMBRA_VALUE_NUMBER, UmbraBehaviorVTable, UmbraCode,
    UmbraStr, UmbraToken, UmbraValue, UmbraValueSink, UmbraVec3,
};

thread_local! {
    /// `abi_version` of every value the plugin reported to [`recording_sink`].
    static VALUE_VERSIONS: RefCell<Vec<u32>> = const { RefCell::new(Vec::new()) };
}

unsafe extern "C" fn record_value(_: UmbraToken, v: *const UmbraValue) -> UmbraCode {
    // SAFETY: the SDK passes a valid value (or null) for the duration of the call.
    if let Some(v) = unsafe { v.as_ref() } {
        VALUE_VERSIONS.with(|l| l.borrow_mut().push(v.abi_version));
    }
    UMBRA_OK
}

fn recording_sink() -> UmbraValueSink {
    UmbraValueSink {
        abi_version: UMBRA_ABI_VERSION_MAJOR << 16,
        struct_size: size_of::<UmbraValueSink>() as u32,
        ctx: 7,
        set: Some(record_value),
    }
}

const fn minor(abi_version: u32) -> u32 {
    abi_version & 0xFFFF
}

/// Every struct the SDK writes for a behavior-only plugin, with its declared minor.
fn written_minors() -> Vec<(&'static str, u32)> {
    reset();
    let d = descriptor();
    let mut out = vec![("descriptor", minor(d.abi_version))];
    for i in 0..d.capability_count {
        let c = capability(i);
        out.push(("capability", minor(c.abi_version)));
        // SAFETY: the vtable is a static of the descriptor.
        let vt = unsafe { &*(c.vtable.cast::<UmbraBehaviorVTable>()) };
        out.push(("vtable", minor(vt.abi_version)));
    }

    // Intents: one tick of the test plugin pushes every intent kind.
    let mut rig = Rig::new(&[(1, 11)], "{}");
    let (code, _) = rig.tick(0);
    assert_eq!(code, UMBRA_OK);
    let intents = pushed();
    assert!(!intents.is_empty(), "the tick pushed intents");
    out.extend(intents.iter().map(|p| ("intent", minor(p.abi_version))));

    // Values: set the echo input, read it back through a recording sink.
    let input = UmbraValue {
        abi_version: UMBRA_ABI_VERSION_MAJOR << 16,
        struct_size: size_of::<UmbraValue>() as u32,
        ty: UMBRA_VALUE_NUMBER,
        boolean: 0,
        reserved_0: [0; 3],
        number: 1.5,
        integer: 0,
        string: UmbraStr {
            ptr: std::ptr::null(),
            len: 0,
        },
        entity: 0,
        position: UmbraVec3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
    };
    let err = err_sink();
    let sink = recording_sink();
    let instance = rig.items[0].instance;
    // SAFETY: a live instance of the test plugin; valid strings, value and sinks for the calls.
    let (set, got) = unsafe {
        (
            vtable().set_input.expect("set_input")(instance, s("echo"), &input, &err),
            vtable().output.expect("output")(instance, s("echo"), &sink),
        )
    };
    assert_eq!((set, got), (UMBRA_OK, UMBRA_OK));
    let values = VALUE_VERSIONS.with(|l| l.borrow_mut().drain(..).collect::<Vec<_>>());
    assert_eq!(values.len(), 1, "one value reported");
    out.extend(values.iter().map(|v| ("value", minor(*v))));
    out
}

#[test]
fn no_struct_the_sdk_writes_declares_a_minor_newer_than_the_descriptor() {
    let written = written_minors();
    let descriptor_minor = written[0].1;

    for (what, m) in &written {
        assert!(
            *m <= descriptor_minor,
            "{what} declares minor {m}, newer than the descriptor's {descriptor_minor}"
        );
    }
    let kinds: std::collections::BTreeSet<&str> = written.iter().map(|(w, _)| *w).collect();
    assert_eq!(
        kinds.into_iter().collect::<Vec<_>>(),
        ["capability", "descriptor", "intent", "value", "vtable"],
        "every kind of SDK-written struct was checked"
    );
}

#[test]
fn a_behavior_only_plugin_writes_minor_0_everywhere() {
    for (what, m) in written_minors() {
        assert_eq!(m, 0, "{what}");
    }
}

// ---------------------------------------------------------------- M6 WI-12: the network kind's rows

mod net_harness;

use umbra_sdk::__private::{Capabilities, OwnedDescriptor, Plugin};
use umbra_sdk::abi::{
    UMBRA_CAPABILITY_BEHAVIOR, UMBRA_CAPABILITY_NETWORK_ADAPTER, UmbraPluginDescriptor,
};
use umbra_sdk::{
    ConfigureInput, Error, Inbox, KeySpace, NetKey, NetMessage, NetworkAdapter,
    NetworkAdapterConfig, NetworkAdapterFactory, SimTime,
};

/// A network adapter whose `poll` pushes one message.
struct MinorNet;
struct MinorNetConfig;
struct MinorNetAdapter;

impl NetworkAdapterFactory for MinorNet {
    const ID: &'static str = "test.network.minor";
    const PARAM_SCHEMA: &'static str = r#"{"type":"object"}"#;
    type Config = MinorNetConfig;
    fn configure(_: &ConfigureInput) -> Result<MinorNetConfig, Error> {
        Ok(MinorNetConfig)
    }
}

impl NetworkAdapterConfig for MinorNetConfig {
    type Adapter = MinorNetAdapter;
    fn key_space(&self) -> KeySpace {
        KeySpace::new(100, 10).expect("valid")
    }
    fn start(&mut self, _: SimTime) -> Result<MinorNetAdapter, Error> {
        Ok(MinorNetAdapter)
    }
}

impl NetworkAdapter for MinorNetAdapter {
    fn publish(&mut self, _: SimTime, _: &[NetMessage<'_>]) -> Result<(), Error> {
        Ok(())
    }
    fn poll(&mut self, _: SimTime, inbox: &mut Inbox<'_>) -> Result<(), Error> {
        inbox.push(&NetMessage::EntityRemoved { key: NetKey(7) })
    }
    fn stop(&mut self) {}
}

/// A plugin with only a network adapter.
struct NetworkOnly;
impl Plugin for NetworkOnly {
    const ID: &'static str = "test.plugin.minor_network";
    const NAME: &'static str = "network only";
    const VERSION: &'static str = "0.1.0";
    fn capabilities(caps: &mut Capabilities) {
        caps.network_adapter::<MinorNet>();
    }
}

/// A plugin with a behavior and a network adapter.
struct Mixed;
impl Plugin for Mixed {
    const ID: &'static str = "test.plugin.minor_mixed";
    const NAME: &'static str = "mixed";
    const VERSION: &'static str = "0.1.0";
    fn capabilities(caps: &mut Capabilities) {
        caps.behavior::<support::OkFactory>();
        caps.network_adapter::<MinorNet>();
    }
}

/// Every struct the SDK writes for `d`, with its declared minor: the descriptor, each capability and
/// vtable (named by kind), and the message a network adapter's `poll` pushes.
fn written_by(d: &UmbraPluginDescriptor) -> Vec<(&'static str, u32)> {
    net_harness::reset();
    let mut out = vec![("descriptor", minor(d.abi_version))];
    for c in net_harness::capabilities(d) {
        match c.kind {
            UMBRA_CAPABILITY_BEHAVIOR => {
                out.push(("behavior capability", minor(c.abi_version)));
                // SAFETY: a behavior capability's vtable lives as long as the descriptor.
                let vt = unsafe { &*(c.vtable.cast::<UmbraBehaviorVTable>()) };
                out.push(("behavior vtable", minor(vt.abi_version)));
            }
            UMBRA_CAPABILITY_NETWORK_ADAPTER => {
                out.push(("network capability", minor(c.abi_version)));
                let vt = net_harness::net_vtable(c);
                out.push(("network vtable", minor(vt.abi_version)));
                let (code, h, err) = net_harness::configure(vt, "{}");
                assert_eq!(code, UMBRA_OK, "{err:?}");
                assert_eq!(net_harness::start(vt, h, 30).0, UMBRA_OK);
                assert_eq!(net_harness::poll(vt, h, 4).0, UMBRA_OK);
                net_harness::destroy(vt, h);
                let pushed = net_harness::pushed();
                assert_eq!(pushed.len(), 1, "the poll pushed one message");
                out.push(("network message", minor(pushed[0].abi_version)));
            }
            other => panic!("unexpected kind {other}"),
        }
    }
    out
}

#[test]
fn a_network_adapter_plugin_declares_minor_1_and_writes_every_network_struct_in_minor_1() {
    let built = OwnedDescriptor::build::<NetworkOnly>();

    let written = written_by(built.descriptor());

    assert_eq!(
        written,
        [
            ("descriptor", 1),
            ("network capability", 1),
            ("network vtable", 1),
            ("network message", 1),
        ]
    );
}

#[test]
fn a_mixed_plugin_keeps_its_behavior_structs_at_minor_0_under_a_minor_1_descriptor() {
    let built = OwnedDescriptor::build::<Mixed>();

    let written = written_by(built.descriptor());

    assert_eq!(
        written,
        [
            ("descriptor", 1),
            ("behavior capability", 0),
            ("behavior vtable", 0),
            ("network capability", 1),
            ("network vtable", 1),
            ("network message", 1),
        ],
        "each struct carries its kind's minor; the descriptor the highest"
    );
    let descriptor_minor = written[0].1;
    assert!(written.iter().all(|(_, m)| *m <= descriptor_minor));
}
