//! AT-17 (part, M7 WI-2): the required-minor rule of ADR 0033 for the ABI 0.2 kinds (ADR 0050). A
//! plugin that exports a motion model, a sensor, a condition or an action declares minor 2 (the
//! descriptor, the capability and its vtable); a plugin that mixes kinds keeps each capability at its
//! own kind's minor under a descriptor of the highest. The behavior-only (minor 0) and network-only
//! (minor 1) rows are in `required_minor.rs` and keep passing.
//!
//! WI-10 un-ignored the rows and added: the public kind minors (`umbra_sdk::MOTION_MODEL_MINOR` and the
//! others) equal what the descriptor writes for each kind, capability and vtable alike; with this SDK a
//! behavior-only plugin still declares minor 0 and a network-only plugin minor 1 (ADR 0033: a newer SDK
//! needs a newer host only for a newer kind), and adding an ABI 0.2 kind lifts only the descriptor.
#![expect(clippy::expect_used, reason = "test harness")]
#![expect(unsafe_code, reason = "reads the vtable headers of the descriptor")]

mod kinds_plugins;
mod net_harness;

use kinds_plugins::{
    ActionOnly, AllKinds, BehaviorAndMotion, BehaviorAndNet, BehaviorOnly, ConditionOnly,
    MotionOnly, NetOnly, SensorOnly,
};
use umbra_sdk::__private::{OwnedDescriptor, Plugin};
use umbra_sdk::abi::{
    UMBRA_ABI_VERSION_MAJOR, UMBRA_ABI_VERSION_MINOR, UMBRA_CAPABILITY_ACTION,
    UMBRA_CAPABILITY_BEHAVIOR, UMBRA_CAPABILITY_CONDITION, UMBRA_CAPABILITY_MOTION_MODEL,
    UMBRA_CAPABILITY_NETWORK_ADAPTER, UMBRA_CAPABILITY_SENSOR, UmbraCapabilityKind,
};
use umbra_sdk::{
    ACTION_MINOR, BEHAVIOR_MINOR, CONDITION_MINOR, MOTION_MODEL_MINOR, NETWORK_ADAPTER_MINOR,
    SENSOR_MINOR,
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
fn a_motion_model_plugin_declares_kind_2_at_minor_2() {
    let (descriptor, caps) = declared::<MotionOnly>();

    assert_eq!(descriptor, (UMBRA_ABI_VERSION_MAJOR, 2));
    assert_eq!(caps, [(UMBRA_CAPABILITY_MOTION_MODEL, 2)]);
}

#[test]
fn a_sensor_plugin_declares_kind_4_at_minor_2() {
    let (descriptor, caps) = declared::<SensorOnly>();

    assert_eq!(descriptor, (UMBRA_ABI_VERSION_MAJOR, 2));
    assert_eq!(caps, [(UMBRA_CAPABILITY_SENSOR, 2)]);
}

#[test]
fn a_condition_plugin_declares_kind_6_at_minor_2() {
    let (descriptor, caps) = declared::<ConditionOnly>();

    assert_eq!(descriptor, (UMBRA_ABI_VERSION_MAJOR, 2));
    assert_eq!(caps, [(UMBRA_CAPABILITY_CONDITION, 2)]);
}

#[test]
fn an_action_plugin_declares_kind_7_at_minor_2() {
    let (descriptor, caps) = declared::<ActionOnly>();

    assert_eq!(descriptor, (UMBRA_ABI_VERSION_MAJOR, 2));
    assert_eq!(caps, [(UMBRA_CAPABILITY_ACTION, 2)]);
}

/// ADR 0033: each capability carries its kind's minor under a descriptor of the highest.
#[test]
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

/// The `abi_version` minor of the vtable of every capability of `P` (the first `u32` pair of every
/// vtable is its header, ADR 0019).
fn vtable_minors<P: Plugin>() -> Vec<u32> {
    let built = OwnedDescriptor::build::<P>();
    net_harness::capabilities(built.descriptor())
        .iter()
        .map(|c| {
            // SAFETY: every SDK vtable is a static `#[repr(C)]` struct starting with its header.
            let header = unsafe { c.vtable.cast::<[u32; 2]>().read() };
            minor(header[0])
        })
        .collect()
}

/// The public kind minors are the values the descriptor writes for each kind: descriptor, capability
/// and vtable.
#[test]
fn the_public_kind_minors_equal_what_the_descriptor_writes_for_each_kind() {
    let rows = [
        (
            declared::<MotionOnly>(),
            vtable_minors::<MotionOnly>(),
            MOTION_MODEL_MINOR,
        ),
        (
            declared::<SensorOnly>(),
            vtable_minors::<SensorOnly>(),
            SENSOR_MINOR,
        ),
        (
            declared::<ConditionOnly>(),
            vtable_minors::<ConditionOnly>(),
            CONDITION_MINOR,
        ),
        (
            declared::<ActionOnly>(),
            vtable_minors::<ActionOnly>(),
            ACTION_MINOR,
        ),
        (
            declared::<BehaviorOnly>(),
            vtable_minors::<BehaviorOnly>(),
            BEHAVIOR_MINOR,
        ),
        (
            declared::<NetOnly>(),
            vtable_minors::<NetOnly>(),
            NETWORK_ADAPTER_MINOR,
        ),
    ];

    for (((_, descriptor_minor), caps), vtables, constant) in rows {
        assert_eq!(descriptor_minor, constant);
        assert_eq!(caps.len(), 1);
        assert_eq!(caps[0].1, constant);
        assert_eq!(vtables, [constant]);
    }
    assert_eq!(
        [
            MOTION_MODEL_MINOR,
            SENSOR_MINOR,
            CONDITION_MINOR,
            ACTION_MINOR
        ],
        [2; 4],
        "ADR 0050: the four kinds exist since minor 2"
    );
    assert!(
        [
            MOTION_MODEL_MINOR,
            SENSOR_MINOR,
            CONDITION_MINOR,
            ACTION_MINOR
        ]
        .iter()
        .all(|&m| m <= UMBRA_ABI_VERSION_MINOR)
    );
}

/// ADR 0033: built with the ABI 0.2 SDK, a plugin without an ABI 0.2 kind still declares the minor of
/// its newest kind, so it loads into the hosts it loaded into before.
#[test]
fn without_an_abi_0_2_kind_a_plugin_keeps_its_old_minor() {
    assert_eq!(
        declared::<BehaviorOnly>(),
        (
            (UMBRA_ABI_VERSION_MAJOR, 0),
            vec![(UMBRA_CAPABILITY_BEHAVIOR, 0)]
        )
    );
    assert_eq!(
        declared::<NetOnly>(),
        (
            (UMBRA_ABI_VERSION_MAJOR, 1),
            vec![(UMBRA_CAPABILITY_NETWORK_ADAPTER, 1)]
        )
    );
    assert_eq!(
        declared::<BehaviorAndNet>(),
        (
            (UMBRA_ABI_VERSION_MAJOR, 1),
            vec![
                (UMBRA_CAPABILITY_BEHAVIOR, 0),
                (UMBRA_CAPABILITY_NETWORK_ADAPTER, 1)
            ]
        )
    );
}

/// Adding an ABI 0.2 kind lifts the descriptor to 2; the behavior keeps minor 0 (capability and vtable).
#[test]
fn adding_an_abi_0_2_kind_lifts_only_the_descriptor() {
    assert_eq!(
        declared::<BehaviorAndMotion>(),
        (
            (UMBRA_ABI_VERSION_MAJOR, 2),
            vec![
                (UMBRA_CAPABILITY_BEHAVIOR, 0),
                (UMBRA_CAPABILITY_MOTION_MODEL, 2)
            ]
        )
    );
    assert_eq!(vtable_minors::<BehaviorAndMotion>(), [0, 2]);
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
