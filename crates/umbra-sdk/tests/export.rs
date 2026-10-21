//! AT-3 (export row): the descriptor the macro-generated entry returns. Requirements: REQ-SDK-1,
//! REQ-P-1, REQ-P-3, ADR 0019, ADR 0020. Step 4 of M4: the SDK was a stub until WI-7.
#![expect(unsafe_code, reason = "reads the raw descriptor like a host does")]

mod support;

use std::mem::size_of;

use support::{capability, descriptor, host_api, s, text, umbra_plugin_entry};
use umbra_sdk::abi::{
    UMBRA_ABI_VERSION, UMBRA_CAPABILITY_BEHAVIOR, UmbraBehaviorVTable, UmbraCapability,
    UmbraPluginDescriptor,
};

#[test]
fn descriptor_lists_capabilities_with_schema_and_metadata() {
    let d = descriptor();
    assert_eq!(text(d.id), "test.plugin.sdk");
    assert_eq!(text(d.name), "SDK test plugin");
    assert_eq!(text(d.version), "0.3.1");
    assert_eq!(d.capability_count, 2);

    let first = capability(0);
    assert_eq!(first.kind, UMBRA_CAPABILITY_BEHAVIOR);
    assert_eq!(text(first.id), "test.behavior.sdk_ok");
    assert_eq!(text(first.version), "", "empty means the plugin version");
    let schema: serde_json::Value =
        serde_json::from_str(&text(first.param_schema)).expect("schema is JSON");
    assert_eq!(schema["description"], "SDK test behavior");
    let meta: serde_json::Value =
        serde_json::from_str(&text(first.metadata)).expect("metadata is JSON");
    assert_eq!(meta["param_types"]["speed_mps"], "number");
    assert_eq!(meta["outputs"]["turns"], "integer");
    assert_eq!(meta["result"], "sdk done");

    let second = capability(1);
    assert_eq!(text(second.id), "test.behavior.sdk_second");
    assert_eq!(text(second.version), "2.0.0");
    let plain = text(second.metadata);
    assert!(plain.is_empty() || plain == "{}", "no interface: {plain:?}");
}

#[test]
fn vtable_has_every_required_entry_and_halt_running() {
    // SAFETY: the vtable is a static of the descriptor.
    let vt = unsafe { &*(capability(0).vtable.cast::<UmbraBehaviorVTable>()) };
    assert!(vt.configure.is_some() && vt.destroy_behavior.is_some());
    assert!(vt.instantiate.is_some() && vt.destroy_instance.is_some());
    assert!(vt.tick_batch.is_some() && vt.halt.is_some());
    assert!(
        vt.halt_running.is_some(),
        "the SDK always fills halt_running (ADR 0020)"
    );
    assert!(vt.set_input.is_some() && vt.output.is_some());
}

#[test]
fn entry_returns_the_same_pointer_twice() {
    let host = host_api();
    // SAFETY: `host` is a live, well-formed host API for both calls (the SDK copies what it keeps).
    let (a, b) = unsafe {
        (
            umbra_plugin_entry(&raw const host),
            umbra_plugin_entry(&raw const host),
        )
    };
    assert!(!a.is_null());
    assert_eq!(
        a, b,
        "the host requires a stable descriptor (UnstableEntry)"
    );
}

#[test]
fn sdk_writes_its_own_minor() {
    let d = descriptor();
    assert_eq!(
        d.abi_version,
        UMBRA_ABI_VERSION & 0xFFFF_0000,
        "behavior-only: the descriptor declares minor 0 (ADR 0033)"
    );
    assert_eq!(d.struct_size as usize, size_of::<UmbraPluginDescriptor>());
    let c = capability(0);
    assert_eq!(
        c.abi_version,
        UMBRA_ABI_VERSION & 0xFFFF_0000,
        "behavior-only: the capability declares minor 0 (ADR 0033)"
    );
    assert_eq!(c.struct_size as usize, size_of::<UmbraCapability>());
    // SAFETY: the vtable is a static of the descriptor.
    let vt = unsafe { &*(c.vtable.cast::<UmbraBehaviorVTable>()) };
    assert_eq!(
        vt.abi_version,
        UMBRA_ABI_VERSION & 0xFFFF_0000,
        "behavior-only: the vtable declares minor 0 (ADR 0033)"
    );
    assert_eq!(vt.struct_size as usize, size_of::<UmbraBehaviorVTable>());
    let _ = s("");
}
