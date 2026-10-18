//! M4 WI-5: `PluginFaultPolicy`. Requirements: REQ-P-7, ADR 0021.

#![forbid(unsafe_code)]

use umbra_core::PluginFaultPolicy;

#[test]
fn default_is_abort() {
    assert_eq!(PluginFaultPolicy::default(), PluginFaultPolicy::Abort);
}

#[test]
fn names_round_trip() {
    for p in [
        PluginFaultPolicy::Abort,
        PluginFaultPolicy::DisableCapability,
    ] {
        assert_eq!(PluginFaultPolicy::from_name(p.name()), Some(p));
    }
    assert_eq!(PluginFaultPolicy::Abort.name(), "abort");
    assert_eq!(
        PluginFaultPolicy::DisableCapability.name(),
        "disable_capability"
    );
}

#[test]
fn unknown_names_are_none() {
    for s in ["", "Abort", "disable", "disable-capability", "abort "] {
        assert_eq!(PluginFaultPolicy::from_name(s), None, "{s:?}");
    }
}
