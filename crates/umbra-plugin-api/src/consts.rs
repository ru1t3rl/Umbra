//! Version constants, caps and the integer "enum" typedefs of ABI v0 (ADR 0019, ADR 0020, ADR 0021).
//!
//! Every enum-like value crossing the boundary is a fixed-width integer with `UMBRA_*` constants, never a
//! Rust `enum` or `bool`: an unknown value is an error for the reader, never undefined behavior
//! (ADR 0019). `0` is never a valid kind, intent kind, value type or log level; for statuses `0` is
//! [`UMBRA_STATUS_UNSET`] ("not written", R-B2).

// ---------------------------------------------------------------- versions

/// ABI major version. A reader requires the major in every struct it reads to equal its own; major 0 is
/// a real major (REQ-P-2, REQ-P-9, ADR 0019). The ABI 0.0 layout is final as of M4 WI-14 and frozen at
/// the M4 close-out (WI-16, ASM-117).
pub const UMBRA_ABI_VERSION_MAJOR: u32 = 0;
/// ABI minor version: bumped for every appended field, vtable entry or kind (append-only, REQ-P-2).
pub const UMBRA_ABI_VERSION_MINOR: u32 = 0;
/// Packed ABI version written into every `abi_version` field: `major << 16 | minor`.
pub const UMBRA_ABI_VERSION: u32 = (UMBRA_ABI_VERSION_MAJOR << 16) | UMBRA_ABI_VERSION_MINOR;
/// Version of the intent set the intent kinds below describe (ADR 0016); equals the host's
/// `umbra_core::INTENT_SET_VERSION` (checked by a host test).
pub const UMBRA_INTENT_SET_VERSION: u32 = 1;

// ---------------------------------------------------------------- caps (R-M2, ASM-119)

/// At most this many capabilities per plugin descriptor.
pub const UMBRA_MAX_CAPABILITIES: usize = 256;
/// At most this many bytes in a plugin or capability ID (equals `umbra_core::MAX_ID_LEN`).
pub const UMBRA_MAX_ID_LEN: usize = 128;
/// At most this many bytes of JSON in a capability's `param_schema` (1 MiB).
pub const UMBRA_MAX_SCHEMA_LEN: usize = 1 << 20;
/// At most this many bytes of JSON in a capability's `metadata` (64 KiB).
pub const UMBRA_MAX_METADATA_LEN: usize = 1 << 16;
/// Error messages are truncated to this many bytes, at a character boundary, by the host.
pub const UMBRA_MAX_ERROR_LEN: usize = 4096;
/// At most this many intents per batch item per call; the next one is refused and faults the capability.
pub const UMBRA_MAX_INTENTS_PER_ITEM: usize = 64;

// ---------------------------------------------------------------- tokens

/// Opaque host token (`host_ctx`, sink contexts, RNG tokens; ADR 0019 R-B4). Written by the host, only
/// passed back by the plugin, never interpreted by it. The host checks every token against the calling
/// thread's active call record and never dereferences it; a stale, foreign or out-of-call token is an
/// error code and faults the capability. Layout (host-internal, not a plugin contract): `u32` generation
/// in the high half, `u32` index in the low half; `0` is never a valid token.
pub type UmbraToken = u64;

// ---------------------------------------------------------------- result codes

/// Result of a fallible call (ADR 0019). Written by the callee; [`UMBRA_OK`] or one of `UMBRA_ERR_*`.
/// Any other value is treated by the host as [`UMBRA_ERR_INTERNAL`].
pub type UmbraCode = i32;
/// The call succeeded.
pub const UMBRA_OK: UmbraCode = 0;
/// A panic was caught at the boundary (REQ-P-7).
pub const UMBRA_ERR_PANIC: UmbraCode = 1;
/// A pointer, size, version, alignment, UTF-8 or enum value was malformed.
pub const UMBRA_ERR_INVALID_ARGUMENT: UmbraCode = 2;
/// The params JSON was rejected by the capability.
pub const UMBRA_ERR_PARAMS: UmbraCode = 3;
/// The request is valid but not supported by this side.
pub const UMBRA_ERR_UNSUPPORTED: UmbraCode = 4;
/// Any other failure of the callee.
pub const UMBRA_ERR_INTERNAL: UmbraCode = 5;
/// The call happened outside its lifecycle or call scope (before `init`, after `shutdown`, stale token).
pub const UMBRA_ERR_STATE: UmbraCode = 6;

// ---------------------------------------------------------------- capability kinds (REQ-P-4)

/// Kind of a capability in the descriptor. Written by the plugin. Only [`UMBRA_CAPABILITY_BEHAVIOR`] is
/// supported in v0; the other numbers are reserved and rejected as unsupported; `0` and unknown values
/// are invalid.
pub type UmbraCapabilityKind = u32;
/// A behavior (vtable `UmbraBehaviorVTable`, ADR 0020).
pub const UMBRA_CAPABILITY_BEHAVIOR: UmbraCapabilityKind = 1;
/// Reserved: motion model (M7).
pub const UMBRA_CAPABILITY_MOTION_MODEL: UmbraCapabilityKind = 2;
/// Reserved: network adapter (M6).
pub const UMBRA_CAPABILITY_NETWORK_ADAPTER: UmbraCapabilityKind = 3;
/// Reserved: sensor (M7).
pub const UMBRA_CAPABILITY_SENSOR: UmbraCapabilityKind = 4;
/// Reserved: component (M7).
pub const UMBRA_CAPABILITY_COMPONENT: UmbraCapabilityKind = 5;
/// Reserved: behavior-graph condition (M7).
pub const UMBRA_CAPABILITY_CONDITION: UmbraCapabilityKind = 6;
/// Reserved: behavior-graph action (M7).
pub const UMBRA_CAPABILITY_ACTION: UmbraCapabilityKind = 7;

// ---------------------------------------------------------------- statuses (R-B2)

/// Status of one batch item after `tick_batch`, written by the plugin into the host-owned `statuses`
/// array. The host pre-fills [`UMBRA_STATUS_UNSET`]; a value left unset or out of range faults the
/// capability.
pub type UmbraStatus = u32;
/// Not written by the plugin (pre-filled by the host).
pub const UMBRA_STATUS_UNSET: UmbraStatus = 0;
/// The behavior succeeded.
pub const UMBRA_STATUS_SUCCESS: UmbraStatus = 1;
/// The behavior failed.
pub const UMBRA_STATUS_FAILURE: UmbraStatus = 2;
/// The behavior is still running.
pub const UMBRA_STATUS_RUNNING: UmbraStatus = 3;

// ---------------------------------------------------------------- intent kinds (ADR 0016, set v1)

/// Kind of a `UmbraIntent`, written by the plugin; `0` and unknown values are refused by the
/// intent sink.
pub type UmbraIntentKind = u32;
/// Move to `position` (ECEF).
pub const UMBRA_INTENT_MOVE_TO: UmbraIntentKind = 1;
/// Follow the entity's route.
pub const UMBRA_INTENT_FOLLOW_ROUTE: UmbraIntentKind = 2;
/// Set the speed to `value` (m/s).
pub const UMBRA_INTENT_SET_SPEED: UmbraIntentKind = 3;
/// Set the true heading to `value` (rad).
pub const UMBRA_INTENT_SET_HEADING: UmbraIntentKind = 4;
/// Hold position.
pub const UMBRA_INTENT_HOLD: UmbraIntentKind = 5;
/// Fire weapon `id` at `entity` (non-zero).
pub const UMBRA_INTENT_FIRE: UmbraIntentKind = 6;
/// Spawn template `id` at `position` with heading `value` (rad).
pub const UMBRA_INTENT_SPAWN: UmbraIntentKind = 7;
/// Despawn this entity.
pub const UMBRA_INTENT_DESPAWN: UmbraIntentKind = 8;
/// Custom intent of component `id` with JSON `payload`.
pub const UMBRA_INTENT_CUSTOM: UmbraIntentKind = 9;

// ---------------------------------------------------------------- value types (BbValue)

/// Type tag of a `UmbraValue`, written by its producer; `0` and unknown values are invalid.
pub type UmbraValueType = u32;
/// `boolean` holds 0 or 1.
pub const UMBRA_VALUE_BOOL: UmbraValueType = 1;
/// `number` holds a finite `f64`.
pub const UMBRA_VALUE_NUMBER: UmbraValueType = 2;
/// `integer` holds an `i64`.
pub const UMBRA_VALUE_INTEGER: UmbraValueType = 3;
/// `string` holds UTF-8.
pub const UMBRA_VALUE_STRING: UmbraValueType = 4;
/// `entity` holds a non-zero entity ID.
pub const UMBRA_VALUE_ENTITY: UmbraValueType = 5;
/// `position` holds a finite ECEF position.
pub const UMBRA_VALUE_POSITION: UmbraValueType = 6;

// ---------------------------------------------------------------- log levels (ADR 0021)

/// Level of a message passed to `UmbraHostApi::log`, written by the plugin; `0` and unknown values are
/// refused with [`UMBRA_ERR_INVALID_ARGUMENT`].
pub type UmbraLogLevel = u32;
/// Error.
pub const UMBRA_LOG_ERROR: UmbraLogLevel = 1;
/// Warning.
pub const UMBRA_LOG_WARN: UmbraLogLevel = 2;
/// Information.
pub const UMBRA_LOG_INFO: UmbraLogLevel = 3;
/// Debug.
pub const UMBRA_LOG_DEBUG: UmbraLogLevel = 4;
/// Trace.
pub const UMBRA_LOG_TRACE: UmbraLogLevel = 5;
