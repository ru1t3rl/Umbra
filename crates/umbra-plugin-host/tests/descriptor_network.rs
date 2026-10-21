//! AT-16 (M6 WI-10, descriptor rows): the network adapter kind of ABI 0.1 at the descriptor level
//! (REQ-P-2, REQ-P-3, REQ-P-4, REQ-P-9, ADR 0019, ADR 0033). Descriptors forged in test code and loaded
//! with `PluginHost::load_static`.
//!
//! Kind 3 needs a descriptor declaring minor 1 or newer; with minor 0 it is rejected naming
//! `capabilities[i].kind`. With minor 1 the capability passes the header, kind, schema, metadata and
//! vtable checks; this host has no network bridge before WI-11, so the load then ends with
//! `Unsupported` (kind 3), never with an ABI or descriptor error. A descriptor declaring minor 0
//! (every plugin built before ABI 0.1) still loads.
#![expect(unsafe_code, reason = "forges raw ABI structs")]
#![expect(
    clippy::cast_possible_truncation,
    reason = "ABI struct sizes are small"
)]

#[macro_use]
mod support;

use std::ffi::c_void;
use std::mem::size_of;

use support::forge::{Parts, leak_str, parts};
use umbra_core::ComponentKind;
use umbra_plugin_api::{
    UMBRA_ABI_VERSION_MAJOR, UMBRA_CAPABILITY_NETWORK_ADAPTER, UMBRA_OK, UmbraCode, UmbraErrorSink,
    UmbraNetAdapter, UmbraNetBatch, UmbraNetSink, UmbraNetworkVTable, UmbraStr,
};
use umbra_plugin_host::{HostOptions, PluginError, PluginHost};

const ORIGIN: &str = "forged-network";

/// `major << 16 | minor` of this host's major.
const fn abi(minor: u32) -> u32 {
    (UMBRA_ABI_VERSION_MAJOR << 16) | minor
}

macro_rules! load {
    ($p:expr) => {{
        let f = forged!($p);
        let mut host = PluginHost::new(HostOptions::default());
        host.load_static(ORIGIN, f.entry).cloned()
    }};
}

unsafe extern "C" fn configure(
    _: UmbraStr,
    _: *mut *mut UmbraNetAdapter,
    _: *const UmbraErrorSink,
) -> UmbraCode {
    UMBRA_OK
}
unsafe extern "C" fn destroy(_: *mut UmbraNetAdapter) {}
unsafe extern "C" fn key_space(_: *mut UmbraNetAdapter, _: *mut u64, _: *mut u64) -> UmbraCode {
    UMBRA_OK
}
unsafe extern "C" fn start(
    _: *mut UmbraNetAdapter,
    _: u64,
    _: u32,
    _: *const UmbraErrorSink,
) -> UmbraCode {
    UMBRA_OK
}
unsafe extern "C" fn publish(
    _: *mut UmbraNetAdapter,
    _: *const UmbraNetBatch,
    _: *const UmbraErrorSink,
) -> UmbraCode {
    UMBRA_OK
}
unsafe extern "C" fn poll(
    _: *mut UmbraNetAdapter,
    _: u64,
    _: usize,
    _: *const UmbraNetSink,
    _: *const UmbraErrorSink,
) -> UmbraCode {
    UMBRA_OK
}
unsafe extern "C" fn stop(_: *mut UmbraNetAdapter) -> UmbraCode {
    UMBRA_OK
}

/// A complete network vtable of minor 1 (without the optional `counters`).
fn network_vtable() -> UmbraNetworkVTable {
    UmbraNetworkVTable {
        abi_version: abi(1),
        struct_size: size_of::<UmbraNetworkVTable>() as u32,
        configure: Some(configure),
        destroy: Some(destroy),
        key_space: Some(key_space),
        start: Some(start),
        publish: Some(publish),
        poll: Some(poll),
        stop: Some(stop),
        counters: None,
    }
}

/// A one-capability plugin whose capability is a network adapter `test.net.forge`, the descriptor
/// declaring `minor`, after `edit` was applied to its vtable.
fn network_parts(minor: u32, edit: impl FnOnce(&mut UmbraNetworkVTable)) -> Parts {
    let mut p = parts();
    p.desc.abi_version = abi(minor);
    p.set_capability(0, "test.net.forge");
    p.caps[0].kind = UMBRA_CAPABILITY_NETWORK_ADAPTER;
    p.caps[0].abi_version = abi(minor);
    let mut vt = network_vtable();
    edit(&mut vt);
    let vt: &'static UmbraNetworkVTable = Box::leak(Box::new(vt));
    p.caps[0].vtable = std::ptr::from_ref(vt).cast::<c_void>();
    p
}

/// Asserts a `Descriptor` error naming `field` and the origin.
fn assert_descriptor_field(err: &PluginError, field: &str) {
    assert!(
        matches!(err, PluginError::Descriptor { field: f, .. } if f == field),
        "expected a descriptor error at {field}, got {err:?}"
    );
    let msg = err.to_string();
    assert!(msg.contains(ORIGIN) && msg.contains(field), "{msg}");
}

#[test]
fn kind_3_in_a_minor_0_descriptor_is_rejected_naming_the_kind() {
    let err = load!(network_parts(0, |_| {})).expect_err("kind 3 needs minor 1");

    assert_descriptor_field(&err, "capabilities[0].kind");
    assert!(err.to_string().contains("minor 1"), "{err}");
}

#[test]
fn kind_3_names_the_index_of_the_offending_capability() {
    let mut p = network_parts(0, |_| {});
    // A behavior first, so the network adapter is capabilities[1].
    let net = p.caps.remove(0);
    p.add_capability("test.behavior.first");
    p.caps[0].abi_version = abi(0);
    p.vtable.abi_version = abi(0);
    p.caps.push(net);
    p.desc.capabilities = p.caps.as_ptr();
    p.desc.capability_count = p.caps.len();

    let err = load!(p).expect_err("kind 3 needs minor 1");

    assert_descriptor_field(&err, "capabilities[1].kind");
}

#[test]
fn kind_3_in_a_minor_1_descriptor_passes_every_descriptor_check() {
    let err = load!(network_parts(1, |_| {})).expect_err("no network bridge before WI-11");

    assert!(
        matches!(&err, PluginError::Unsupported { capability, kind: 3, .. } if capability == "test.net.forge"),
        "header, kind, schema and vtable are accepted; only the missing bridge refuses: {err:?}"
    );
}

#[test]
fn the_optional_counters_entry_is_accepted() {
    unsafe extern "C" fn counters(_: *mut UmbraNetAdapter, _: *mut u64, _: usize) -> UmbraCode {
        UMBRA_OK
    }
    let err = load!(network_parts(1, |vt| vt.counters = Some(counters))).expect_err("no bridge");

    assert!(
        matches!(err, PluginError::Unsupported { kind: 3, .. }),
        "{err:?}"
    );
}

#[test]
fn every_required_network_entry_is_checked() {
    type Edit = fn(&mut UmbraNetworkVTable);
    let rows: [(&str, Edit); 7] = [
        ("configure", |vt| vt.configure = None),
        ("destroy", |vt| vt.destroy = None),
        ("key_space", |vt| vt.key_space = None),
        ("start", |vt| vt.start = None),
        ("publish", |vt| vt.publish = None),
        ("poll", |vt| vt.poll = None),
        ("stop", |vt| vt.stop = None),
    ];
    for (entry, edit) in rows {
        let f = forged!(network_parts(1, edit));
        let mut host = PluginHost::new(HostOptions::default());
        let err = host.load_static(ORIGIN, f.entry).cloned().expect_err(entry);

        assert_descriptor_field(&err, &format!("capabilities[0].vtable.{entry}"));
    }
}

#[test]
fn a_network_vtable_declaring_minor_0_is_rejected() {
    let err = load!(network_parts(1, |vt| vt.abi_version = abi(0))).expect_err("defined in 0.1");

    assert_descriptor_field(&err, "capabilities[0].vtable.abi_version");
}

#[test]
fn a_network_vtable_of_a_newer_minor_is_an_abi_mismatch() {
    let err = load!(network_parts(1, |vt| vt.abi_version = abi(2))).expect_err("newer minor");

    assert!(
        matches!(&err, PluginError::AbiMismatch { field, .. } if field == "capabilities[0].vtable.abi_version"),
        "{err:?}"
    );
}

#[test]
fn network_vtable_sizes_are_checked() {
    let own = size_of::<UmbraNetworkVTable>() as u32;
    for bad in [8, own - 8, own - 4, own + 8] {
        let f = forged!(network_parts(1, |vt| vt.struct_size = bad));
        let mut host = PluginHost::new(HostOptions::default());
        let err = host
            .load_static(ORIGIN, f.entry)
            .cloned()
            .expect_err("bad size");

        assert_descriptor_field(&err, "capabilities[0].vtable.struct_size");
    }
}

#[test]
fn a_null_network_vtable_is_rejected() {
    let mut p = network_parts(1, |_| {});
    p.caps[0].vtable = std::ptr::null();

    let err = load!(p).expect_err("null vtable");

    assert_descriptor_field(&err, "capabilities[0].vtable");
}

#[test]
fn network_metadata_must_be_empty_in_abi_0_1() {
    let mut p = network_parts(1, |_| {});
    p.caps[0].metadata = leak_str(r#"{"param_types":{}}"#);

    let err = load!(p).expect_err("no network metadata is defined");

    assert_descriptor_field(&err, "capabilities[0].metadata");
}

#[test]
fn a_network_param_schema_is_still_checked() {
    let mut p = network_parts(1, |_| {});
    p.caps[0].param_schema = leak_str("{");

    let err = load!(p).expect_err("bad schema");

    assert_descriptor_field(&err, "capabilities[0].param_schema");
}

#[test]
fn a_minor_0_behavior_plugin_still_loads() {
    let mut p = parts();
    p.desc.abi_version = abi(0);
    p.caps[0].abi_version = abi(0);
    p.vtable.abi_version = abi(0);
    p.link();

    let info = load!(p).expect("an ABI 0.0 plugin loads in an ABI 0.1 host");

    assert_eq!(info.abi, (0, 0));
    assert_eq!(info.capabilities.len(), 1);
    assert_eq!(info.capabilities[0].kind, ComponentKind::Behavior);
}

#[test]
fn a_minor_1_behavior_plugin_loads() {
    let mut p = parts();
    p.desc.abi_version = abi(1);
    p.link();

    let info = load!(p).expect("behaviors exist since minor 0");

    assert_eq!(info.abi, (0, 1));
    assert_eq!(info.capabilities[0].kind, ComponentKind::Behavior);
}

// The descriptor's minor is the ceiling of every nested header (ADR 0033, clarified 2026-10-21).

#[test]
fn a_capability_newer_than_its_descriptor_is_rejected() {
    let mut p = parts();
    p.desc.abi_version = abi(0);
    p.vtable.abi_version = abi(0);
    p.link();
    p.caps[0].abi_version = abi(1);

    let err = load!(p).expect_err("capability 0.1 in a 0.0 descriptor");

    assert_descriptor_field(&err, "capabilities[0].abi_version");
}

#[test]
fn a_behavior_vtable_newer_than_its_descriptor_is_rejected() {
    let mut p = parts();
    p.desc.abi_version = abi(0);
    p.caps[0].abi_version = abi(0);
    p.vtable.abi_version = abi(1);
    p.link();

    let err = load!(p).expect_err("vtable 0.1 in a 0.0 descriptor");

    assert_descriptor_field(&err, "capabilities[0].vtable.abi_version");
}
