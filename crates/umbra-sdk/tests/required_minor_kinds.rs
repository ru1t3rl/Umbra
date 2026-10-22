//! AT-17 (part, M7 WI-2): the required-minor rule of ADR 0033 for the ABI 0.2 kinds (ADR 0050). A
//! plugin that exports a motion model, a sensor, a condition or an action declares minor 2 (the
//! descriptor, the capability and its vtable); a plugin that mixes kinds keeps each capability at its
//! own kind's minor under a descriptor of the highest. The behavior-only (minor 0) and network-only
//! (minor 1) rows are in `required_minor.rs` and keep passing.
//!
//! The rows are ignored until WI-10 (`Capabilities::{motion_model, sensor, condition, action}` are
//! stubs that panic with `M7 WI-10`). The vtable struct minors are written with WI-7, which creates
//! the vtable types.
#![expect(clippy::expect_used, reason = "test harness")]

mod kinds_plugins;
mod net_harness;

use kinds_plugins::{ActionOnly, AllKinds, ConditionOnly, MotionOnly, SensorOnly};
use umbra_sdk::__private::{OwnedDescriptor, Plugin};
use umbra_sdk::abi::{
    UMBRA_ABI_VERSION_MAJOR, UMBRA_CAPABILITY_ACTION, UMBRA_CAPABILITY_BEHAVIOR,
    UMBRA_CAPABILITY_CONDITION, UMBRA_CAPABILITY_MOTION_MODEL, UMBRA_CAPABILITY_NETWORK_ADAPTER,
    UMBRA_CAPABILITY_SENSOR, UmbraCapabilityKind,
};

const fn minor(abi_version: u32) -> u32 {
    abi_version & 0xFFFF
}

/// `(kind, capability minor)` of every capability of `P`'s descriptor, and the descriptor's own
/// `(major, minor)`.
fn declared<P: Plugin>() -> ((u32, u32), Vec<(UmbraCapabilityKind, u32)>) {
    let built = OwnedDescriptor::build::<P>();
    let d = built.descriptor();
    let caps = net_harness::capabilities(d)
        .iter()
        .map(|c| (c.kind, minor(c.abi_version)))
        .collect();
    ((d.abi_version >> 16, minor(d.abi_version)), caps)
}

#[test]
#[ignore = "M7 WI-10"]
fn a_motion_model_plugin_declares_kind_2_at_minor_2() {
    let (descriptor, caps) = declared::<MotionOnly>();

    assert_eq!(descriptor, (UMBRA_ABI_VERSION_MAJOR, 2));
    assert_eq!(caps, [(UMBRA_CAPABILITY_MOTION_MODEL, 2)]);
}

#[test]
#[ignore = "M7 WI-10"]
fn a_sensor_plugin_declares_kind_4_at_minor_2() {
    let (descriptor, caps) = declared::<SensorOnly>();

    assert_eq!(descriptor, (UMBRA_ABI_VERSION_MAJOR, 2));
    assert_eq!(caps, [(UMBRA_CAPABILITY_SENSOR, 2)]);
}

#[test]
#[ignore = "M7 WI-10"]
fn a_condition_plugin_declares_kind_6_at_minor_2() {
    let (descriptor, caps) = declared::<ConditionOnly>();

    assert_eq!(descriptor, (UMBRA_ABI_VERSION_MAJOR, 2));
    assert_eq!(caps, [(UMBRA_CAPABILITY_CONDITION, 2)]);
}

#[test]
#[ignore = "M7 WI-10"]
fn an_action_plugin_declares_kind_7_at_minor_2() {
    let (descriptor, caps) = declared::<ActionOnly>();

    assert_eq!(descriptor, (UMBRA_ABI_VERSION_MAJOR, 2));
    assert_eq!(caps, [(UMBRA_CAPABILITY_ACTION, 2)]);
}

/// ADR 0033: each capability carries its kind's minor under a descriptor of the highest.
#[test]
#[ignore = "M7 WI-10"]
fn a_mixed_plugin_keeps_each_capability_at_its_kinds_minor_under_the_highest_descriptor() {
    let (descriptor, caps) = declared::<AllKinds>();

    assert_eq!(descriptor, (UMBRA_ABI_VERSION_MAJOR, 2));
    assert_eq!(
        caps,
        [
            (UMBRA_CAPABILITY_NETWORK_ADAPTER, 1),
            (UMBRA_CAPABILITY_MOTION_MODEL, 2),
            (UMBRA_CAPABILITY_SENSOR, 2),
            (UMBRA_CAPABILITY_CONDITION, 2),
            (UMBRA_CAPABILITY_ACTION, 2),
        ],
        "declaration order, kind minors"
    );
    assert!(
        caps.iter()
            .all(|&(kind, _)| kind != UMBRA_CAPABILITY_BEHAVIOR),
        "no behavior was declared"
    );
}

// The macro accepts the four new lists next to the older two (a compile-time row: the entry is not
// called here, the descriptor rows above build the same declarations through `OwnedDescriptor`).
umbra_sdk::export_plugin! {
    id: "test.plugin.kinds_export",
    name: "kinds export",
    version: "0.1.0",
    network_adapters: [kinds_plugins::NullNet],
    motion_models: [kinds_plugins::Glide],
    sensors: [kinds_plugins::Ping],
    conditions: [kinds_plugins::Always],
    actions: [kinds_plugins::Once],
}
