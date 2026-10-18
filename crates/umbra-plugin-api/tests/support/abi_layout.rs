//! The ABI layout as Rust computes it: every struct with every field (name, `offset_of!`, size), the
//! integer typedefs and the constants (ADR 0019, R-M7).
//!
//! Shared by `umbra-plugin-api/tests/layout.rs` (which pins these values to literals and proves the
//! field lists complete: the field sizes of each struct add up to its size) and
//! `umbra-plugin-host/tests/c_header.rs` (which turns them into C `_Static_assert`s against the
//! generated header), so the C check never relies on hand-copied numbers. Included with `#[path]`.

use core::mem::offset_of;

use umbra_plugin_api::*;

/// One field as Rust lays it out.
pub struct FieldLayout {
    /// Field name (identical in Rust and C).
    pub name: &'static str,
    /// `offset_of!(T, field)`.
    pub offset: usize,
    /// `size_of` the field's type.
    pub size: usize,
    /// `type_name` of the field's type (for example `f64`, `*const u8`, `umbra_plugin_api::..::UmbraStr`).
    pub ty: &'static str,
}

/// One `#[repr(C)]` struct as Rust lays it out.
pub struct StructLayout {
    /// Type name (identical in Rust and C).
    pub name: &'static str,
    /// `size_of::<T>()`.
    pub size: usize,
    /// `align_of::<T>()`.
    pub align: usize,
    /// Every field, in declaration order.
    pub fields: Vec<FieldLayout>,
}

/// `size_of` and `type_name` of the type of the field selected by `select`, inferred from the struct
/// definition (so the table cannot state a wrong type).
fn field_type<T, F>(_select: fn(&T) -> &F) -> (usize, &'static str) {
    (size_of::<F>(), core::any::type_name::<F>())
}

macro_rules! layout {
    ($t:ident { $($f:ident),* $(,)? }) => {
        StructLayout {
            name: stringify!($t),
            size: size_of::<$t>(),
            align: align_of::<$t>(),
            fields: vec![$({
                let (size, ty) = field_type(|s: &$t| &s.$f);
                FieldLayout { name: stringify!($f), offset: offset_of!($t, $f), size, ty }
            }),*],
        }
    };
}

/// Every struct of the ABI except the opaque handles (`UmbraBehavior`, `UmbraInstance`), which are
/// incomplete types in C.
#[expect(
    clippy::too_many_lines,
    reason = "one entry per ABI struct, kept as one table"
)]
pub fn structs() -> Vec<StructLayout> {
    vec![
        layout!(UmbraStr { ptr, len }),
        layout!(UmbraVec3 { x, y, z }),
        layout!(UmbraQuat { w, x, y, z }),
        layout!(UmbraErrorSink {
            abi_version,
            struct_size,
            ctx,
            set
        }),
        layout!(UmbraHostApi {
            abi_version,
            struct_size,
            host_ctx,
            log,
            rng_next_u64,
            rng_next_f64,
        }),
        layout!(UmbraPluginDescriptor {
            abi_version,
            struct_size,
            id,
            name,
            version,
            capabilities,
            capability_count,
            init,
            shutdown,
        }),
        layout!(UmbraCapability {
            abi_version,
            struct_size,
            kind,
            reserved_0,
            id,
            version,
            param_schema,
            metadata,
            vtable,
        }),
        layout!(UmbraBehaviorVTable {
            abi_version,
            struct_size,
            configure,
            destroy_behavior,
            instantiate,
            destroy_instance,
            tick_batch,
            halt,
            halt_running,
            set_input,
            output,
        }),
        layout!(UmbraBehaviorBatch {
            abi_version,
            struct_size,
            tick,
            rate_hz,
            reserved_0,
            items,
            count,
            statuses,
            intents,
        }),
        layout!(UmbraBehaviorItem {
            abi_version,
            struct_size,
            behavior,
            instance,
            entity,
            rng,
        }),
        layout!(UmbraEntityView {
            abi_version,
            struct_size,
            id,
            position,
            velocity,
            orientation,
            force,
            has_route,
            route_complete,
            reserved_0,
            route_next,
            route,
            route_len,
            contacts,
            contact_count,
        }),
        layout!(UmbraContact {
            abi_version,
            struct_size,
            entity,
            position,
            velocity,
            force,
            range_m,
        }),
        layout!(UmbraIntent {
            abi_version,
            struct_size,
            kind,
            reserved_0,
            position,
            value,
            entity,
            id,
            payload,
        }),
        layout!(UmbraIntentSink {
            abi_version,
            struct_size,
            ctx,
            push
        }),
        layout!(UmbraValue {
            abi_version,
            struct_size,
            ty,
            boolean,
            reserved_0,
            number,
            integer,
            string,
            entity,
            position,
        }),
        layout!(UmbraValueSink {
            abi_version,
            struct_size,
            ctx,
            set
        }),
    ]
}

/// Every integer typedef and the entry-point type with its Rust size.
pub fn typedefs() -> Vec<(&'static str, usize)> {
    vec![
        ("UmbraToken", size_of::<UmbraToken>()),
        ("UmbraCode", size_of::<UmbraCode>()),
        ("UmbraCapabilityKind", size_of::<UmbraCapabilityKind>()),
        ("UmbraStatus", size_of::<UmbraStatus>()),
        ("UmbraIntentKind", size_of::<UmbraIntentKind>()),
        ("UmbraValueType", size_of::<UmbraValueType>()),
        ("UmbraLogLevel", size_of::<UmbraLogLevel>()),
        ("UmbraPluginEntry", size_of::<UmbraPluginEntry>()),
    ]
}

macro_rules! constants {
    ($($c:ident),* $(,)?) => {
        vec![$((stringify!($c), $c.to_string())),*]
    };
}

/// Every `UMBRA_*` constant with its Rust value as decimal text.
pub fn constants() -> Vec<(&'static str, String)> {
    constants![
        UMBRA_ABI_VERSION_MAJOR,
        UMBRA_ABI_VERSION_MINOR,
        UMBRA_ABI_VERSION,
        UMBRA_INTENT_SET_VERSION,
        UMBRA_MAX_CAPABILITIES,
        UMBRA_MAX_ID_LEN,
        UMBRA_MAX_SCHEMA_LEN,
        UMBRA_MAX_METADATA_LEN,
        UMBRA_MAX_ERROR_LEN,
        UMBRA_MAX_INTENTS_PER_ITEM,
        UMBRA_OK,
        UMBRA_ERR_PANIC,
        UMBRA_ERR_INVALID_ARGUMENT,
        UMBRA_ERR_PARAMS,
        UMBRA_ERR_UNSUPPORTED,
        UMBRA_ERR_INTERNAL,
        UMBRA_ERR_STATE,
        UMBRA_CAPABILITY_BEHAVIOR,
        UMBRA_CAPABILITY_MOTION_MODEL,
        UMBRA_CAPABILITY_NETWORK_ADAPTER,
        UMBRA_CAPABILITY_SENSOR,
        UMBRA_CAPABILITY_COMPONENT,
        UMBRA_CAPABILITY_CONDITION,
        UMBRA_CAPABILITY_ACTION,
        UMBRA_STATUS_UNSET,
        UMBRA_STATUS_SUCCESS,
        UMBRA_STATUS_FAILURE,
        UMBRA_STATUS_RUNNING,
        UMBRA_INTENT_MOVE_TO,
        UMBRA_INTENT_FOLLOW_ROUTE,
        UMBRA_INTENT_SET_SPEED,
        UMBRA_INTENT_SET_HEADING,
        UMBRA_INTENT_HOLD,
        UMBRA_INTENT_FIRE,
        UMBRA_INTENT_SPAWN,
        UMBRA_INTENT_DESPAWN,
        UMBRA_INTENT_CUSTOM,
        UMBRA_VALUE_BOOL,
        UMBRA_VALUE_NUMBER,
        UMBRA_VALUE_INTEGER,
        UMBRA_VALUE_STRING,
        UMBRA_VALUE_ENTITY,
        UMBRA_VALUE_POSITION,
        UMBRA_LOG_ERROR,
        UMBRA_LOG_WARN,
        UMBRA_LOG_INFO,
        UMBRA_LOG_DEBUG,
        UMBRA_LOG_TRACE,
    ]
}
