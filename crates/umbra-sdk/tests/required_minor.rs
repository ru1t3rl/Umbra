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
