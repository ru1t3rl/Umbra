//! AT-15 (M7 WI-2, host descriptor rows): the capability kinds of ABI 0.2 at the descriptor level
//! (REQ-P-2, REQ-P-4, REQ-P-9, ADR 0019, ADR 0033, ADR 0050). Descriptors forged in test code and
//! loaded with `PluginHost::load_static`.
//!
//! Kinds 2 (motion model), 4 (sensor), 6 (condition) and 7 (action) need a descriptor declaring minor 2
//! or newer; a descriptor declaring minor 0 or 1 that names one is rejected naming the kind (today
//! `Unsupported { kind }`, from WI-7 possibly `Descriptor { field: "capabilities[i].kind" }` as for
//! kind 3: the rows accept both), and kind 5 (`component`, no defined semantics, ASM-620) is rejected
//! at every minor. The rows that hold today run now; the rows that need the host to accept minor 2 are ignored
//! until WI-7. The vtable, metadata and size checks of the new vtables need their structs and are
//! written with WI-7 (the layout pins, as in M6).
#![expect(unsafe_code, reason = "forges raw ABI structs")]

#[macro_use]
mod support;

use support::forge::{Parts, parts};
use umbra_plugin_api::{
    UMBRA_ABI_VERSION_MAJOR, UMBRA_CAPABILITY_ACTION, UMBRA_CAPABILITY_COMPONENT,
    UMBRA_CAPABILITY_CONDITION, UMBRA_CAPABILITY_MOTION_MODEL, UMBRA_CAPABILITY_SENSOR,
    UmbraCapabilityKind,
};
use umbra_plugin_host::{HostOptions, PluginError, PluginHost};

const ORIGIN: &str = "forged-kinds";

/// The kinds ABI 0.2 adds, with their names.
const NEW_KINDS: [(&str, UmbraCapabilityKind); 4] = [
    ("motion model", UMBRA_CAPABILITY_MOTION_MODEL),
    ("sensor", UMBRA_CAPABILITY_SENSOR),
    ("condition", UMBRA_CAPABILITY_CONDITION),
    ("action", UMBRA_CAPABILITY_ACTION),
];

/// `major << 16 | minor` of this host's major.
const fn abi(minor: u32) -> u32 {
    (UMBRA_ABI_VERSION_MAJOR << 16) | minor
}

/// A one-capability plugin whose capability has `kind` (behind the behavior vtable, which no check
/// before the kind rule reads), the descriptor and the capability declaring `minor`.
fn kind_parts(kind: UmbraCapabilityKind, minor: u32) -> Parts {
    let mut p = parts();
    p.desc.abi_version = abi(minor);
    p.caps[0].abi_version = abi(minor);
    p.caps[0].kind = kind;
    p
}

macro_rules! load {
    ($p:expr) => {{
        let f = forged!($p);
        let mut host = PluginHost::new(HostOptions::default());
        host.load_static(ORIGIN, f.entry).cloned()
    }};
}

/// Whether `err` rejects capability kind `kind`: the reserved-kind answer of ABI 0.0/0.1 hosts
/// (`Unsupported`) or the kind-rule answer (`Descriptor` at `capabilities[0].kind`).
fn rejects_the_kind(err: &PluginError, kind: UmbraCapabilityKind) -> bool {
    match err {
        PluginError::Unsupported { kind: k, .. } => *k == kind,
        PluginError::Descriptor { field, .. } => field == "capabilities[0].kind",
        _ => false,
    }
}

#[test]
fn a_new_kind_in_a_minor_1_descriptor_is_rejected_naming_the_kind() {
    for (name, kind) in NEW_KINDS {
        let err = load!(kind_parts(kind, 1)).expect_err(name);

        assert!(rejects_the_kind(&err, kind), "{name}: {err:?}");
        assert!(err.to_string().contains(ORIGIN), "{name}: {err}");
    }
}

#[test]
fn a_new_kind_in_a_minor_0_descriptor_is_rejected_naming_the_kind() {
    for (name, kind) in NEW_KINDS {
        let err = load!(kind_parts(kind, 0)).expect_err(name);

        assert!(rejects_the_kind(&err, kind), "{name}: {err:?}");
    }
}

#[test]
fn the_component_kind_is_rejected_in_a_minor_1_descriptor() {
    let err = load!(kind_parts(UMBRA_CAPABILITY_COMPONENT, 1)).expect_err("kind 5 is reserved");

    assert!(
        rejects_the_kind(&err, UMBRA_CAPABILITY_COMPONENT),
        "{err:?}"
    );
}

#[test]
#[ignore = "M7 WI-7"]
fn the_component_kind_is_rejected_in_a_minor_2_descriptor() {
    let err = load!(kind_parts(UMBRA_CAPABILITY_COMPONENT, 2)).expect_err("kind 5 stays reserved");

    assert!(
        rejects_the_kind(&err, UMBRA_CAPABILITY_COMPONENT),
        "{err:?}"
    );
}

/// ADR 0050: at minor 2 the header and kind rules pass. With the behavior vtable behind the capability
/// the vtable checks reject the plugin (or, once the checks pass, `Unsupported` until WI-8/9), so the
/// error is a vtable or metadata field, or `Unsupported`.
#[test]
#[ignore = "M7 WI-7"]
fn a_new_kind_in_a_minor_2_descriptor_passes_the_header_and_the_kind_checks() {
    for (name, kind) in NEW_KINDS {
        let err = load!(kind_parts(kind, 2)).expect_err(name);

        let past_the_kind_rule = match &err {
            PluginError::Unsupported { .. } => true,
            PluginError::Descriptor { field, .. } => {
                field.starts_with("capabilities[0].vtable")
                    || field.starts_with("capabilities[0].metadata")
            }
            PluginError::AbiMismatch { field, .. } => field.starts_with("capabilities[0].vtable"),
            _ => false,
        };
        assert!(past_the_kind_rule, "{name}: {err:?}");
    }
}

#[test]
#[ignore = "M7 WI-7"]
fn a_minor_2_descriptor_with_a_behavior_loads() {
    let mut p = parts();
    p.desc.abi_version = abi(2);
    p.caps[0].abi_version = abi(2);
    p.vtable.abi_version = abi(2);
    p.link();

    let info = load!(p).expect("an ABI 0.2 host accepts a minor 2 descriptor");

    assert_eq!(info.abi, (0, 2));
}
