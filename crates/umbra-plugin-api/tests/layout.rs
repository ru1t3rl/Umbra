//! AT-1 (M4 WI-3): layout pins of ABI v0 on 64-bit targets (REQ-P-2, ADR 0019 R-B1, R-B2, R-M7).
//!
//! The literal numbers are the layout table of `docs/plans/M4.md`; changing one is an ABI change (the
//! 0.0 layout is final as of WI-14 and frozen at the M4 close-out, WI-16, ASM-117; any later change is
//! append-only with a minor bump, and the plan table, these pins, the header and
//! `docs/plugin-abi-changelog.md` change together). `support/abi_layout.rs` holds the same structs as
//! Rust computes them; the C side checks those values against the header (`umbra-plugin-host`
//! `tests/c_header.rs`).

#![forbid(unsafe_code)]

#[path = "support/abi_layout.rs"]
mod abi_layout;

use core::mem::offset_of;

use abi_layout::StructLayout;
use umbra_plugin_api::*;

/// Asserts the literal size, alignment 8 and every literal field offset of one struct, and returns
/// its name and pinned field names (to cross-check the support table).
macro_rules! pin {
    ($t:ident, size $size:literal; $($f:ident $off:literal),* $(,)?) => {{
        assert_eq!(size_of::<$t>(), $size, concat!("size_of ", stringify!($t)));
        assert_eq!(align_of::<$t>(), 8, concat!("align_of ", stringify!($t)));
        $(assert_eq!(
            offset_of!($t, $f),
            $off,
            concat!("offset_of ", stringify!($t), ".", stringify!($f))
        );)*
        (stringify!($t), vec![$(stringify!($f)),*])
    }};
}

/// The structs that carry the `abi_version`/`struct_size` header (all but the value primitives).
const HEADED: [&str; 17] = [
    "UmbraErrorSink",
    "UmbraHostApi",
    "UmbraPluginDescriptor",
    "UmbraCapability",
    "UmbraBehaviorVTable",
    "UmbraBehaviorBatch",
    "UmbraBehaviorItem",
    "UmbraEntityView",
    "UmbraContact",
    "UmbraIntent",
    "UmbraIntentSink",
    "UmbraValue",
    "UmbraValueSink",
    "UmbraNetMessage",
    "UmbraNetBatch",
    "UmbraNetSink",
    "UmbraNetworkVTable",
];

/// The frozen value primitives without header (ADR 0019).
const PRIMITIVES: [&str; 3] = ["UmbraStr", "UmbraVec3", "UmbraQuat"];

fn find<'a>(all: &'a [StructLayout], name: &str) -> &'a StructLayout {
    all.iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("{name} missing from support/abi_layout.rs"))
}

/// The plan's layout table, field by field (offsets in bytes).
#[test]
fn layouts_are_pinned_on_64_bit() {
    let pinned = vec![
        pin!(UmbraStr, size 16; ptr 0, len 8),
        pin!(UmbraVec3, size 24; x 0, y 8, z 16),
        pin!(UmbraQuat, size 32; w 0, x 8, y 16, z 24),
        pin!(UmbraErrorSink, size 24; abi_version 0, struct_size 4, ctx 8, set 16),
        pin!(UmbraHostApi, size 40;
            abi_version 0, struct_size 4, host_ctx 8, log 16, rng_next_u64 24, rng_next_f64 32),
        pin!(UmbraPluginDescriptor, size 88;
            abi_version 0, struct_size 4, id 8, name 24, version 40, capabilities 56,
            capability_count 64, init 72, shutdown 80),
        pin!(UmbraCapability, size 88;
            abi_version 0, struct_size 4, kind 8, reserved_0 12, id 16, version 32,
            param_schema 48, metadata 64, vtable 80),
        pin!(UmbraBehaviorVTable, size 80;
            abi_version 0, struct_size 4, configure 8, destroy_behavior 16, instantiate 24,
            destroy_instance 32, tick_batch 40, halt 48, halt_running 56, set_input 64, output 72),
        pin!(UmbraBehaviorBatch, size 56;
            abi_version 0, struct_size 4, tick 8, rate_hz 16, reserved_0 20, items 24, count 32,
            statuses 40, intents 48),
        pin!(UmbraBehaviorItem, size 40;
            abi_version 0, struct_size 4, behavior 8, instance 16, entity 24, rng 32),
        pin!(UmbraEntityView, size 160;
            abi_version 0, struct_size 4, id 8, position 16, velocity 40, orientation 64, force 96,
            has_route 112, route_complete 113, reserved_0 114, route_next 120, route 128,
            route_len 136, contacts 144, contact_count 152),
        pin!(UmbraContact, size 88;
            abi_version 0, struct_size 4, entity 8, position 16, velocity 40, force 64, range_m 80),
        pin!(UmbraIntent, size 88;
            abi_version 0, struct_size 4, kind 8, reserved_0 12, position 16, value 40, entity 48,
            id 56, payload 72),
        pin!(UmbraIntentSink, size 24; abi_version 0, struct_size 4, ctx 8, push 16),
        pin!(UmbraValue, size 80;
            abi_version 0, struct_size 4, ty 8, boolean 12, reserved_0 13, number 16, integer 24,
            string 32, entity 48, position 56),
        pin!(UmbraValueSink, size 24; abi_version 0, struct_size 4, ctx 8, set 16),
        // ABI 0.1 (M6 WI-10, ADR 0033): appended structs; every 0.0 row above is unchanged.
        pin!(UmbraNetMessage, size 224;
            abi_version 0, struct_size 4, kind 8, dead_reckoning 12, key 16, other_key 24,
            position 32, velocity 56, acceleration 80, angular_velocity 104, orientation 128,
            entity_type 152, force 168, name 184, weapon 200, control 216, reserved_0 220),
        pin!(UmbraNetBatch, size 40;
            abi_version 0, struct_size 4, tick 8, rate_hz 16, reserved_0 20, messages 24, count 32),
        pin!(UmbraNetSink, size 24; abi_version 0, struct_size 4, ctx 8, push 16),
        pin!(UmbraNetworkVTable, size 72;
            abi_version 0, struct_size 4, configure 8, destroy 16, key_space 24, start 32,
            publish 40, poll 48, stop 56, counters 64),
    ];

    // The reserved arrays have the documented widths.
    assert_eq!(
        offset_of!(UmbraEntityView, route_next) - offset_of!(UmbraEntityView, reserved_0),
        6
    );
    assert_eq!(
        offset_of!(UmbraValue, number) - offset_of!(UmbraValue, reserved_0),
        3
    );
    // Opaque handles are zero-sized in Rust and incomplete in C; only pointers to them cross.
    assert_eq!(size_of::<UmbraBehavior>(), 0);
    assert_eq!(size_of::<UmbraInstance>(), 0);
    assert_eq!(size_of::<UmbraNetAdapter>(), 0);

    // The support table (used for the C asserts) lists exactly these structs and fields, in order.
    let support = abi_layout::structs();
    let support_names: Vec<(&str, Vec<&str>)> = support
        .iter()
        .map(|s| (s.name, s.fields.iter().map(|f| f.name).collect()))
        .collect();
    assert_eq!(support_names, pinned);
}

/// R-B1: every byte of every struct belongs to a named field: fields are contiguous from offset 0 and
/// end exactly at `size_of`. This also proves the support table lists every field.
#[test]
fn no_struct_has_padding() {
    for s in abi_layout::structs() {
        let mut fields: Vec<_> = s.fields.iter().collect();
        fields.sort_by_key(|f| f.offset);
        let mut end = 0;
        for f in fields {
            assert_eq!(
                f.offset, end,
                "{}: gap or overlap before {}",
                s.name, f.name
            );
            end = f.offset + f.size;
        }
        assert_eq!(end, s.size, "{}: trailing padding", s.name);
        let sum: usize = s.fields.iter().map(|f| f.size).sum();
        assert_eq!(sum, s.size, "{}: sum of field sizes", s.name);
    }
}

/// REQ-P-2: every growable struct starts with `abi_version: u32` at 0 and `struct_size: u32` at 4;
/// only the frozen value primitives have no header.
#[test]
fn every_headed_struct_starts_with_abi_version_and_struct_size() {
    let all = abi_layout::structs();
    assert_eq!(all.len(), HEADED.len() + PRIMITIVES.len());
    for name in HEADED {
        let s = find(&all, name);
        let head: Vec<(&str, usize, usize)> = s
            .fields
            .iter()
            .take(2)
            .map(|f| (f.name, f.offset, f.size))
            .collect();
        assert_eq!(
            head,
            [("abi_version", 0, 4), ("struct_size", 4, 4)],
            "{name}"
        );
    }
    for name in PRIMITIVES {
        let s = find(&all, name);
        assert!(
            s.fields.iter().all(|f| f.name != "abi_version"),
            "{name} is a frozen primitive without header"
        );
    }
}

/// R-B1: headed structs have alignment 8 and a size that is a multiple of 8, so a field appended in a
/// later minor never lands in former padding.
#[test]
fn headed_sizes_are_multiples_of_8_with_align_8() {
    let all = abi_layout::structs();
    for name in HEADED.iter().chain(PRIMITIVES.iter()) {
        let s = find(&all, name);
        assert_eq!(s.align, 8, "{name}");
        assert_eq!(s.size % 8, 0, "{name}");
    }
}

/// ADR 0019: `UMBRA_ABI_VERSION = major << 16 | minor`; ABI 0.1 since M6 (ADR 0033).
#[test]
fn abi_version_packs_major_and_minor() {
    assert_eq!(UMBRA_ABI_VERSION >> 16, UMBRA_ABI_VERSION_MAJOR);
    assert_eq!(UMBRA_ABI_VERSION & 0xffff, UMBRA_ABI_VERSION_MINOR);
    assert_eq!(
        (
            UMBRA_ABI_VERSION_MAJOR,
            UMBRA_ABI_VERSION_MINOR,
            UMBRA_ABI_VERSION
        ),
        (0, 1, 1)
    );
}

/// ADR 0019, ADR 0020, ADR 0021, ASM-119: every constant has its documented value.
#[test]
fn constants_and_caps_have_documented_values() {
    assert_eq!(UMBRA_INTENT_SET_VERSION, 1);
    // Caps (R-M2).
    assert_eq!(UMBRA_MAX_CAPABILITIES, 256);
    assert_eq!(UMBRA_MAX_ID_LEN, 128);
    assert_eq!(UMBRA_MAX_SCHEMA_LEN, 1_048_576);
    assert_eq!(UMBRA_MAX_METADATA_LEN, 65_536);
    assert_eq!(UMBRA_MAX_ERROR_LEN, 4096);
    assert_eq!(UMBRA_MAX_INTENTS_PER_ITEM, 64);
    // Result codes.
    assert_eq!(
        [
            UMBRA_OK,
            UMBRA_ERR_PANIC,
            UMBRA_ERR_INVALID_ARGUMENT,
            UMBRA_ERR_PARAMS,
            UMBRA_ERR_UNSUPPORTED,
            UMBRA_ERR_INTERNAL,
            UMBRA_ERR_STATE,
        ],
        [0, 1, 2, 3, 4, 5, 6]
    );
    // Capability kinds (REQ-P-4): behavior plus the reserved v1 kinds.
    assert_eq!(
        [
            UMBRA_CAPABILITY_BEHAVIOR,
            UMBRA_CAPABILITY_MOTION_MODEL,
            UMBRA_CAPABILITY_NETWORK_ADAPTER,
            UMBRA_CAPABILITY_SENSOR,
            UMBRA_CAPABILITY_COMPONENT,
            UMBRA_CAPABILITY_CONDITION,
            UMBRA_CAPABILITY_ACTION,
        ],
        [1, 2, 3, 4, 5, 6, 7]
    );
    // Statuses (R-B2).
    assert_eq!(
        [
            UMBRA_STATUS_UNSET,
            UMBRA_STATUS_SUCCESS,
            UMBRA_STATUS_FAILURE,
            UMBRA_STATUS_RUNNING,
        ],
        [0, 1, 2, 3]
    );
    // Intent kinds (ADR 0016 set v1).
    assert_eq!(
        [
            UMBRA_INTENT_MOVE_TO,
            UMBRA_INTENT_FOLLOW_ROUTE,
            UMBRA_INTENT_SET_SPEED,
            UMBRA_INTENT_SET_HEADING,
            UMBRA_INTENT_HOLD,
            UMBRA_INTENT_FIRE,
            UMBRA_INTENT_SPAWN,
            UMBRA_INTENT_DESPAWN,
            UMBRA_INTENT_CUSTOM,
        ],
        [1, 2, 3, 4, 5, 6, 7, 8, 9]
    );
    // Value types (BbValue).
    assert_eq!(
        [
            UMBRA_VALUE_BOOL,
            UMBRA_VALUE_NUMBER,
            UMBRA_VALUE_INTEGER,
            UMBRA_VALUE_STRING,
            UMBRA_VALUE_ENTITY,
            UMBRA_VALUE_POSITION,
        ],
        [1, 2, 3, 4, 5, 6]
    );
    // Log levels.
    assert_eq!(
        [
            UMBRA_LOG_ERROR,
            UMBRA_LOG_WARN,
            UMBRA_LOG_INFO,
            UMBRA_LOG_DEBUG,
            UMBRA_LOG_TRACE,
        ],
        [1, 2, 3, 4, 5]
    );
    // The support table used for the C asserts lists every constant above once.
    let names: std::collections::BTreeSet<&str> =
        abi_layout::constants().iter().map(|(n, _)| *n).collect();
    assert_eq!(names.len(), 68);
    assert_eq!(abi_layout::constants().len(), 68);
}

/// Typedef widths: tokens 8 bytes, codes `i32`, enums `u32`, the entry a (non-null) function pointer.
#[test]
fn typedefs_have_fixed_widths() {
    let widths: Vec<(&str, usize)> = abi_layout::typedefs();
    assert_eq!(
        widths,
        [
            ("UmbraToken", 8),
            ("UmbraCode", 4),
            ("UmbraCapabilityKind", 4),
            ("UmbraStatus", 4),
            ("UmbraIntentKind", 4),
            ("UmbraValueType", 4),
            ("UmbraLogLevel", 4),
            ("UmbraPluginEntry", 8),
            ("UmbraNetKind", 4),
            ("UmbraDeadReckoning", 4),
            ("UmbraNetControl", 4),
        ]
    );
}

/// R-B2: `0` is "not written" for statuses, so a status the plugin forgot is detectable.
#[test]
fn status_unset_is_zero() {
    assert_eq!(UMBRA_STATUS_UNSET, 0);
    assert_ne!(UMBRA_STATUS_SUCCESS, UMBRA_STATUS_UNSET);
    assert_ne!(UMBRA_STATUS_FAILURE, UMBRA_STATUS_UNSET);
    assert_ne!(UMBRA_STATUS_RUNNING, UMBRA_STATUS_UNSET);
}

/// ADR 0019: every function-pointer field is `Option<unsafe extern "C" fn ..>`, because C may write
/// null. Compile-time check: each struct below is built with `None` in every function-pointer field
/// (which compiles only for `Option`), and the literals list every field (so none is missed). The
/// null-pointer niche keeps `Option` at 8 bytes.
#[test]
fn function_pointer_fields_are_nullable() {
    let s = UmbraStr {
        ptr: core::ptr::null(),
        len: 0,
    };
    let sink = UmbraErrorSink {
        abi_version: 0,
        struct_size: 0,
        ctx: 0,
        set: None,
    };
    let host = UmbraHostApi {
        abi_version: 0,
        struct_size: 0,
        host_ctx: 0,
        log: None,
        rng_next_u64: None,
        rng_next_f64: None,
    };
    let descriptor = UmbraPluginDescriptor {
        abi_version: 0,
        struct_size: 0,
        id: s,
        name: s,
        version: s,
        capabilities: core::ptr::null(),
        capability_count: 0,
        init: None,
        shutdown: None,
    };
    let vtable = UmbraBehaviorVTable {
        abi_version: 0,
        struct_size: 0,
        configure: None,
        destroy_behavior: None,
        instantiate: None,
        destroy_instance: None,
        tick_batch: None,
        halt: None,
        halt_running: None,
        set_input: None,
        output: None,
    };
    let intents = UmbraIntentSink {
        abi_version: 0,
        struct_size: 0,
        ctx: 0,
        push: None,
    };
    let values = UmbraValueSink {
        abi_version: 0,
        struct_size: 0,
        ctx: 0,
        set: None,
    };
    assert!(sink.set.is_none() && intents.push.is_none() && values.set.is_none());
    assert!(host.log.is_none() && host.rng_next_u64.is_none() && host.rng_next_f64.is_none());
    assert!(descriptor.init.is_none() && descriptor.shutdown.is_none());
    assert!(vtable.configure.is_none() && vtable.output.is_none());
    assert_eq!(size_of::<Option<unsafe extern "C" fn()>>(), 8);
}

/// R-B1: every `reserved_N` field is a `u32` or a byte array, so writers can zero it and readers can
/// ignore it without interpreting a type.
#[test]
fn reserved_fields_are_u32_or_byte_arrays() {
    let mut seen = 0;
    for s in abi_layout::structs() {
        for f in s.fields.iter().filter(|f| f.name.starts_with("reserved_")) {
            assert!(
                f.ty == "u32" || f.ty.starts_with("[u8; "),
                "{}.{} is {}",
                s.name,
                f.name,
                f.ty
            );
            seen += 1;
        }
    }
    assert_eq!(
        seen, 7,
        "UmbraCapability, UmbraBehaviorBatch, UmbraEntityView, UmbraIntent, UmbraValue, UmbraNetMessage, UmbraNetBatch"
    );
}

/// The hand-kept type lists agree: every `pub struct` and `pub type` in `src/*.rs` is in the support
/// table (structs, the two opaque handles, typedefs) and vice versa (the C side checks the header
/// against the same table in `umbra-plugin-host/tests/c_header.rs`).
#[test]
fn source_types_match_the_support_table() {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut structs = std::collections::BTreeSet::new();
    let mut types = std::collections::BTreeSet::new();
    let entries = std::fs::read_dir(&src).unwrap_or_else(|e| panic!("{}: {e}", src.display()));
    for entry in entries {
        let path = entry.unwrap_or_else(|e| panic!("{e}")).path();
        if path.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        let text =
            std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        for line in text.lines().map(str::trim_start) {
            let ident = |rest: &str| {
                rest.split(|c: char| !(c.is_alphanumeric() || c == '_'))
                    .next()
                    .unwrap_or("")
                    .to_string()
            };
            if let Some(rest) = line.strip_prefix("pub struct ") {
                structs.insert(ident(rest));
            } else if let Some(rest) = line.strip_prefix("pub type ") {
                types.insert(ident(rest));
            }
        }
    }
    let mut expected_structs: std::collections::BTreeSet<String> = abi_layout::structs()
        .iter()
        .map(|s| s.name.to_string())
        .collect();
    expected_structs.extend(["UmbraBehavior".to_string(), "UmbraInstance".to_string()]);
    expected_structs.insert("UmbraNetAdapter".to_string());
    let expected_types: std::collections::BTreeSet<String> = abi_layout::typedefs()
        .iter()
        .map(|(n, _)| (*n).to_string())
        .collect();

    assert_eq!(structs, expected_structs);
    assert_eq!(types, expected_types);
}

/// ADR 0033: the ABI 0.1 constants of the network adapter kind have their documented values; `0` is
/// invalid for every network enum (none of the constants is 0).
#[test]
fn network_constants_have_documented_values() {
    assert_eq!(UMBRA_CAPABILITY_NETWORK_ADAPTER, 3);
    assert_eq!(UMBRA_MAX_NET_POLL, 4096);
    assert_eq!(UMBRA_MAX_NET_COUNTERS, 8);
    assert_eq!(
        [
            UMBRA_NET_ENTITY_STATE,
            UMBRA_NET_ENTITY_REMOVED,
            UMBRA_NET_FIRE,
            UMBRA_NET_DETONATION,
            UMBRA_NET_CONTROL,
        ],
        [1, 2, 3, 4, 5]
    );
    // ADR 0032's model order (`umbra_core::dead_reckoning::DeadReckoning`).
    assert_eq!(
        [
            UMBRA_DR_STATIC,
            UMBRA_DR_VELOCITY,
            UMBRA_DR_VELOCITY_ROTATION,
            UMBRA_DR_ACCELERATION_ROTATION,
            UMBRA_DR_ACCELERATION,
            UMBRA_DR_BODY_VELOCITY,
            UMBRA_DR_BODY_VELOCITY_ROTATION,
            UMBRA_DR_BODY_ACCELERATION_ROTATION,
            UMBRA_DR_BODY_ACCELERATION,
        ],
        [1, 2, 3, 4, 5, 6, 7, 8, 9]
    );
    assert_eq!(
        [
            UMBRA_NET_CONTROL_START,
            UMBRA_NET_CONTROL_PAUSE,
            UMBRA_NET_CONTROL_RESUME,
            UMBRA_NET_CONTROL_STOP,
        ],
        [1, 2, 3, 4]
    );
}

/// ADR 0019 for the ABI 0.1 structs: every function-pointer field is nullable (compiles only for
/// `Option`), and the literals list every field.
#[test]
fn network_function_pointer_fields_are_nullable() {
    let sink = UmbraNetSink {
        abi_version: 0,
        struct_size: 0,
        ctx: 0,
        push: None,
    };
    let vtable = UmbraNetworkVTable {
        abi_version: 0,
        struct_size: 0,
        configure: None,
        destroy: None,
        key_space: None,
        start: None,
        publish: None,
        poll: None,
        stop: None,
        counters: None,
    };
    assert!(sink.push.is_none());
    assert!(vtable.configure.is_none() && vtable.destroy.is_none() && vtable.key_space.is_none());
    assert!(vtable.start.is_none() && vtable.publish.is_none() && vtable.poll.is_none());
    assert!(vtable.stop.is_none() && vtable.counters.is_none());
}

/// R-B1 for the ABI 0.1 reserved fields: their widths fill exactly the documented gaps.
#[test]
fn network_reserved_fields_fill_their_gaps() {
    assert_eq!(
        offset_of!(UmbraNetMessage, reserved_0) - offset_of!(UmbraNetMessage, control),
        4
    );
    assert_eq!(
        size_of::<UmbraNetMessage>() - offset_of!(UmbraNetMessage, reserved_0),
        4
    );
    assert_eq!(
        offset_of!(UmbraNetBatch, messages) - offset_of!(UmbraNetBatch, reserved_0),
        4
    );
}
