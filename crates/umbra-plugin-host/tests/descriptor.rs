//! AT-4 (load and descriptor rows that need no real library): descriptors forged in test code and
//! loaded with `PluginHost::load_static`. Requirements: REQ-P-1..4, REQ-P-8, REQ-P-9, ADR 0019
//! (bounds, caps, per-struct major and size), ADR 0020 (metadata), ADR 0021 (init/shutdown), ASM-119.
//!
//! Step 4 of M4: written before WI-8; the host API is a `todo!()` stub, so every test is ignored with
//! that work item. Every failure checks the error variant and that the message names the plugin origin
//! and the offending field (field spellings are WI-8's; the needles below are the plain ABI names).
#![expect(unsafe_code, reason = "forges raw ABI structs")]
#![expect(
    clippy::cast_possible_truncation,
    reason = "ABI struct sizes are small"
)]

#[macro_use]
mod support;

use std::mem::size_of;
use std::sync::atomic::Ordering;

use support::forge::{
    Parts, bad_utf8, dummy_output, dummy_set_input, leak_str, misaligned_caps, parts,
};
use umbra_core::{ComponentKind, Provenance};
use umbra_plugin_api::{
    UMBRA_CAPABILITY_BEHAVIOR, UMBRA_CAPABILITY_MOTION_MODEL, UmbraBehaviorVTable, UmbraCapability,
    UmbraPluginDescriptor,
};
use umbra_plugin_host::{HostOptions, PluginError, PluginHost, PluginInfo};

const ORIGIN: &str = "forged-origin";

fn load(p: Parts) -> Result<PluginInfo, PluginError> {
    let f = forged!(p);
    let mut host = PluginHost::new(HostOptions::default());
    host.load_static(ORIGIN, f.entry).cloned()
}

macro_rules! rejected {
    ($name:ident, $edit:expr, $variant:pat, $needle:expr) => {
        #[test]
        #[ignore = "WI-8"]
        fn $name() {
            let mut p = parts();
            let edit: fn(&mut Parts) = $edit;
            edit(&mut p);

            let err = load(p).expect_err("the descriptor must be rejected");

            assert!(matches!(err, $variant), "variant: {err:?}");
            let msg = err.to_string();
            assert!(msg.contains(ORIGIN), "names the origin: {msg}");
            assert!(msg.contains($needle), "names {:?}: {msg}", $needle);
        }
    };
}

const DESC: u32 = size_of::<UmbraPluginDescriptor>() as u32;
const CAP: u32 = size_of::<UmbraCapability>() as u32;
const VT: u32 = size_of::<UmbraBehaviorVTable>() as u32;

#[test]
#[ignore = "WI-8"]
fn given_a_valid_descriptor_when_loaded_then_info_is_copied() {
    let info = load(parts()).expect("valid");

    assert_eq!(info.id.as_str(), "test.plugin.forge");
    assert_eq!(info.name, "Forge");
    assert_eq!(info.version.to_string(), "0.1.0");
    assert_eq!(info.abi, (0, 0));
    assert!(info.origin.contains(ORIGIN));
    assert_eq!(info.capabilities.len(), 1);
    let c = &info.capabilities[0];
    assert_eq!(c.kind, ComponentKind::Behavior);
    assert_eq!(c.id.as_str(), "test.behavior.forge");
    assert_eq!(
        c.version, info.version,
        "an empty capability version is the plugin version"
    );
    assert!(!c.faulted);
    let _ = Provenance::Plugin;
}

// Sizes (ADR 0019): at least the minor-0 size, at most the host's, a multiple of 8.
rejected!(
    descriptor_struct_size_too_small,
    |p| p.desc.struct_size = 16,
    PluginError::Descriptor { .. },
    "struct_size"
);
rejected!(
    descriptor_struct_size_above_host,
    |p| p.desc.struct_size = DESC + 8,
    PluginError::Descriptor { .. },
    "struct_size"
);
rejected!(
    descriptor_struct_size_not_a_multiple_of_8,
    |p| p.desc.struct_size = DESC - 4,
    PluginError::Descriptor { .. },
    "struct_size"
);
rejected!(
    capability_struct_size_too_small,
    |p| p.caps[0].struct_size = 16,
    PluginError::Descriptor { .. },
    "struct_size"
);
rejected!(
    capability_struct_size_above_host,
    |p| p.caps[0].struct_size = CAP + 8,
    PluginError::Descriptor { .. },
    "struct_size"
);
rejected!(
    capability_struct_size_not_a_multiple_of_8,
    |p| p.caps[0].struct_size = CAP - 4,
    PluginError::Descriptor { .. },
    "struct_size"
);
rejected!(
    vtable_struct_size_too_small,
    |p| p.vtable.struct_size = 16,
    PluginError::Descriptor { .. },
    "struct_size"
);
rejected!(
    vtable_struct_size_above_host,
    |p| p.vtable.struct_size = VT + 8,
    PluginError::Descriptor { .. },
    "struct_size"
);
rejected!(
    vtable_struct_size_not_a_multiple_of_8,
    |p| p.vtable.struct_size = VT - 4,
    PluginError::Descriptor { .. },
    "struct_size"
);

// Required vtable entries.
rejected!(
    vtable_configure_null,
    |p| p.vtable.configure = None,
    PluginError::Descriptor { .. },
    "configure"
);
rejected!(
    vtable_destroy_behavior_null,
    |p| p.vtable.destroy_behavior = None,
    PluginError::Descriptor { .. },
    "destroy_behavior"
);
rejected!(
    vtable_instantiate_null,
    |p| p.vtable.instantiate = None,
    PluginError::Descriptor { .. },
    "instantiate"
);
rejected!(
    vtable_destroy_instance_null,
    |p| p.vtable.destroy_instance = None,
    PluginError::Descriptor { .. },
    "destroy_instance"
);
rejected!(
    vtable_tick_batch_null,
    |p| p.vtable.tick_batch = None,
    PluginError::Descriptor { .. },
    "tick_batch"
);
rejected!(
    vtable_halt_null,
    |p| p.vtable.halt = None,
    PluginError::Descriptor { .. },
    "halt"
);

// Bounds and caps.
rejected!(
    capability_count_above_256,
    |p| p.desc.capability_count = 257,
    PluginError::Descriptor { .. },
    "capability_count"
);
rejected!(
    null_capabilities_with_a_count,
    |p| p.desc.capabilities = std::ptr::null(),
    PluginError::Descriptor { .. },
    "capabilities"
);
rejected!(
    misaligned_capability_array,
    |p| {
        let m = misaligned_caps(p);
        p.desc.capabilities = m;
    },
    PluginError::Descriptor { .. },
    "capabilities"
);
rejected!(
    plugin_id_over_128_bytes,
    |p| p.desc.id = leak_str(&format!("a.{}", "b".repeat(130))),
    PluginError::Descriptor { .. },
    "id"
);
rejected!(
    capability_id_over_128_bytes,
    |p| p.caps[0].id = leak_str(&format!("test.{}", "b".repeat(130))),
    PluginError::Descriptor { .. },
    "id"
);
rejected!(
    schema_over_1_mib,
    |p| p.caps[0].param_schema =
        leak_str(&format!("{{\"description\":\"{}\"}}", "x".repeat(1 << 20))),
    PluginError::Descriptor { .. },
    "param_schema"
);
rejected!(
    metadata_over_64_kib,
    |p| p.caps[0].metadata = leak_str(&format!("{{\"result\":\"{}\"}}", "x".repeat(1 << 16))),
    PluginError::Descriptor { .. },
    "metadata"
);

// UTF-8.
rejected!(
    invalid_utf8_in_plugin_id,
    |p| p.desc.id = bad_utf8(),
    PluginError::Descriptor { .. },
    "id"
);
rejected!(
    invalid_utf8_in_plugin_name,
    |p| p.desc.name = bad_utf8(),
    PluginError::Descriptor { .. },
    "name"
);
rejected!(
    invalid_utf8_in_schema,
    |p| p.caps[0].param_schema = bad_utf8(),
    PluginError::Descriptor { .. },
    "param_schema"
);
rejected!(
    invalid_utf8_in_metadata,
    |p| p.caps[0].metadata = bad_utf8(),
    PluginError::Descriptor { .. },
    "metadata"
);

// Grammar.
rejected!(
    bad_plugin_id_grammar,
    |p| p.desc.id = leak_str("Bad Id!"),
    PluginError::Descriptor { .. },
    "id"
);
rejected!(
    bad_plugin_semver,
    |p| p.desc.version = leak_str("1.x"),
    PluginError::Descriptor { .. },
    "version"
);
rejected!(
    bad_capability_semver,
    |p| p.caps[0].version = leak_str("not-semver"),
    PluginError::Descriptor { .. },
    "version"
);
rejected!(
    builtin_capability_id_is_reserved,
    |p| p.caps[0].id = leak_str("builtin.behavior.evil"),
    PluginError::ReservedId { .. },
    "builtin.behavior.evil"
);

// Kinds.
rejected!(
    motion_model_kind_is_unsupported,
    |p| p.caps[0].kind = UMBRA_CAPABILITY_MOTION_MODEL,
    PluginError::Unsupported { .. },
    "test.behavior.forge"
);
rejected!(
    unknown_kind_is_unsupported,
    |p| p.caps[0].kind = 99,
    PluginError::Unsupported { .. },
    "test.behavior.forge"
);

// Schema and metadata.
rejected!(
    schema_that_is_not_json,
    |p| p.caps[0].param_schema = leak_str("{"),
    PluginError::Descriptor { .. },
    "param_schema"
);
rejected!(
    schema_that_is_not_an_object,
    |p| p.caps[0].param_schema = leak_str("[1]"),
    PluginError::Descriptor { .. },
    "param_schema"
);
rejected!(
    metadata_that_is_not_json,
    |p| p.caps[0].metadata = leak_str("{"),
    PluginError::Descriptor { .. },
    "metadata"
);
rejected!(
    metadata_with_an_unknown_key,
    |p| p.caps[0].metadata = leak_str(r#"{"bogus":1}"#),
    PluginError::Descriptor { .. },
    "metadata"
);
rejected!(
    param_types_without_set_input,
    |p| p.caps[0].metadata = leak_str(r#"{"param_types":{"speed":"number"}}"#),
    PluginError::Descriptor { .. },
    "set_input"
);
rejected!(
    outputs_without_output_entry,
    |p| p.caps[0].metadata = leak_str(r#"{"outputs":{"turns":"integer"}}"#),
    PluginError::Descriptor { .. },
    "output"
);
rejected!(
    param_type_that_disagrees_with_the_schema,
    |p| {
        p.vtable.set_input = Some(dummy_set_input);
        p.caps[0].param_schema =
            leak_str(r#"{"type":"object","properties":{"speed":{"type":"number"}}}"#);
        p.caps[0].metadata = leak_str(r#"{"param_types":{"speed":"string"}}"#);
    },
    PluginError::Descriptor { .. },
    "speed"
);

// ABI versions (REQ-P-9): the descriptor's and every struct's own major.
rejected!(
    descriptor_abi_major_differs,
    |p| p.desc.abi_version = 1 << 16,
    PluginError::AbiMismatch { .. },
    "ABI"
);
rejected!(
    descriptor_abi_minor_is_newer,
    |p| p.desc.abi_version = 1,
    PluginError::AbiMismatch { .. },
    "ABI"
);
rejected!(
    capability_abi_major_differs,
    |p| p.caps[0].abi_version = 1 << 16,
    PluginError::AbiMismatch { .. },
    "ABI"
);
rejected!(
    vtable_abi_major_differs,
    |p| p.vtable.abi_version = 1 << 16,
    PluginError::AbiMismatch { .. },
    "ABI"
);

#[test]
#[ignore = "WI-8"]
fn given_consistent_metadata_and_entries_when_loaded_then_it_is_accepted() {
    let mut p = parts();
    p.vtable.set_input = Some(dummy_set_input);
    p.vtable.output = Some(dummy_output);
    p.caps[0].param_schema =
        leak_str(r#"{"type":"object","properties":{"speed":{"type":"number"}}}"#);
    p.caps[0].metadata = leak_str(
        r#"{"param_types":{"speed":"number","target":"entity"},"outputs":{"turns":"integer"},"result":"done"}"#,
    );

    assert!(load(p).is_ok());
}

#[test]
#[ignore = "WI-8"]
fn given_a_duplicate_capability_in_one_plugin_when_loaded_then_rejected() {
    let mut p = parts();
    p.add_capability("test.behavior.forge");

    let err = load(p).expect_err("duplicate");

    let msg = err.to_string();
    assert!(
        msg.contains("test.behavior.forge") && msg.contains(ORIGIN),
        "{msg}"
    );
}

#[test]
#[ignore = "WI-8"]
fn given_a_null_descriptor_when_loaded_then_null_descriptor_names_the_origin() {
    let entry = forged_null!();
    let mut host = PluginHost::new(HostOptions::default());

    let err = host.load_static(ORIGIN, entry).expect_err("null");

    assert!(matches!(err, PluginError::NullDescriptor { .. }), "{err:?}");
    assert!(err.to_string().contains(ORIGIN));
}

#[test]
#[ignore = "WI-8"]
fn given_an_entry_returning_another_pointer_second_time_then_unstable_entry() {
    let f = forged!(parts(), alt = parts());
    let mut host = PluginHost::new(HostOptions::default());

    let err = host.load_static(ORIGIN, f.entry).expect_err("unstable");

    assert!(matches!(err, PluginError::UnstableEntry { .. }), "{err:?}");
    assert_eq!(
        f.entry_calls.load(Ordering::SeqCst),
        2,
        "exactly two entry calls"
    );
}

#[test]
#[ignore = "WI-8"]
fn given_init_returns_an_error_when_loaded_then_init_error_and_no_shutdown() {
    let f = forged!(parts());
    f.init_code
        .store(umbra_plugin_api::UMBRA_ERR_INTERNAL, Ordering::SeqCst);
    let err = {
        let mut host = PluginHost::new(HostOptions::default());
        host.load_static(ORIGIN, f.entry).expect_err("init fails")
    };

    assert!(matches!(err, PluginError::Init { .. }), "{err:?}");
    assert_eq!(f.init_calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        f.shutdown_calls.load(Ordering::SeqCst),
        0,
        "no shutdown without a successful init"
    );
}

#[test]
#[ignore = "WI-8"]
fn given_a_loaded_plugin_when_the_host_drops_then_entry_twice_init_once_shutdown_once() {
    let f = forged!(parts());
    {
        let mut host = PluginHost::new(HostOptions::default());
        host.load_static(ORIGIN, f.entry).expect("valid");
    }

    assert_eq!(f.entry_calls.load(Ordering::SeqCst), 2);
    assert_eq!(f.init_calls.load(Ordering::SeqCst), 1);
    assert_eq!(f.shutdown_calls.load(Ordering::SeqCst), 1);
    let _ = UMBRA_CAPABILITY_BEHAVIOR;
}
