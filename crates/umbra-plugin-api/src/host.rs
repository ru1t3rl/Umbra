//! The host API, the plugin descriptor, capabilities and the entry point type (REQ-P-1, REQ-P-3, REQ-P-5,
//! ADR 0019, ADR 0021).

use core::ffi::c_void;

use crate::consts::{UmbraCapabilityKind, UmbraCode, UmbraLogLevel, UmbraToken};
use crate::types::{UmbraErrorSink, UmbraStr};

/// Host API v0 (ADR 0021): logging and RNG; time arrives in every call, registration is the descriptor.
/// Written by the host in its own minor-0 layout (not the plugin's declared minor, which is known only
/// from the descriptor the entry returns, ASM-134 a) and passed to `umbra_plugin_entry`; it is a
/// process-lifetime allocation (one per image lifecycle, never freed), so the pointer stays valid
/// forever, but calls through it outside the plugin's lifecycle (before `init`, after `shutdown`) return
/// `UMBRA_ERR_STATE`. Callbacks are leaf calls: they never call into any plugin. Terrain queries and
/// event emission are appended in M7.
#[repr(C)]
pub struct UmbraHostApi {
    /// ABI version the host was compiled against (`UMBRA_ABI_VERSION`).
    pub abi_version: u32,
    /// `sizeof` this struct in the minor-0 layout (ASM-134 a); a later host minor keeps passing the
    /// minor-0 size unless the ABI gains a way to declare the plugin's minor before the entry call.
    pub struct_size: u32,
    /// Opaque host token of this image lifecycle, passed back to every callback.
    pub host_ctx: UmbraToken,
    /// Logs `message` (UTF-8, borrowed during the call) at `level`, attributed to the plugin. Required.
    pub log: Option<
        unsafe extern "C" fn(
            host_ctx: UmbraToken,
            level: UmbraLogLevel,
            message: UmbraStr,
        ) -> UmbraCode,
    >,
    /// Draws the next `u64` from the entity stream named by `rng` (the batch item's RNG token, valid only
    /// during that `tick_batch` call) into `*out`. Required.
    pub rng_next_u64: Option<
        unsafe extern "C" fn(host_ctx: UmbraToken, rng: UmbraToken, out: *mut u64) -> UmbraCode,
    >,
    /// Draws the next `f64` in `[0, 1)` from the entity stream named by `rng` into `*out`. Required.
    pub rng_next_f64: Option<
        unsafe extern "C" fn(host_ctx: UmbraToken, rng: UmbraToken, out: *mut f64) -> UmbraCode,
    >,
}

/// The plugin descriptor returned by `umbra_plugin_entry` (REQ-P-3). Written by the plugin as a static:
/// it and everything it points to must stay valid and unchanged until `shutdown` returns; the host copies
/// it at load and never re-reads it. Every call of the entry in one lifecycle returns the same pointer.
#[repr(C)]
pub struct UmbraPluginDescriptor {
    /// The ABI version the plugin requires (REQ-P-3): its major must equal the host's, its minor must not
    /// be newer (REQ-P-9).
    pub abi_version: u32,
    /// `sizeof` this struct as the plugin knows it.
    pub struct_size: u32,
    /// Plugin ID (`PluginId` grammar, ADR 0002), at most `UMBRA_MAX_ID_LEN` bytes.
    pub id: UmbraStr,
    /// Human-readable plugin name.
    pub name: UmbraStr,
    /// Plugin version (semantic version text).
    pub version: UmbraStr,
    /// Array of `capability_count` capabilities, 8-byte aligned; null only when the count is 0. The
    /// stride is the first element's `struct_size`.
    pub capabilities: *const UmbraCapability,
    /// Number of capabilities, at most `UMBRA_MAX_CAPABILITIES`.
    pub capability_count: usize,
    /// Called once per image lifecycle after the descriptor was validated, before any other call; a
    /// non-`UMBRA_OK` result fails the load (a message may go to `err`). `host` is the same pointer the
    /// entry received. Optional (null = nothing to do).
    pub init: Option<
        unsafe extern "C" fn(host: *const UmbraHostApi, err: *const UmbraErrorSink) -> UmbraCode,
    >,
    /// Called once when the image is released, only if `init` succeeded, after every handle was
    /// destroyed. Optional (null = nothing to do).
    pub shutdown: Option<unsafe extern "C" fn()>,
}

/// One capability in the descriptor (REQ-P-3, REQ-P-4). Written by the plugin, same lifetime as the
/// descriptor; copied by the host at load.
#[repr(C)]
pub struct UmbraCapability {
    /// ABI version the plugin was compiled against.
    pub abi_version: u32,
    /// `sizeof` this struct as the plugin knows it; also the array stride.
    pub struct_size: u32,
    /// The capability kind; `vtable` points at the matching vtable type.
    pub kind: UmbraCapabilityKind,
    /// Reserved: written as 0, ignored by readers.
    pub reserved_0: u32,
    /// Component ID (ADR 0002), at most `UMBRA_MAX_ID_LEN` bytes; `builtin.*` is refused.
    pub id: UmbraStr,
    /// Capability version (semantic version text); empty = the plugin version.
    pub version: UmbraStr,
    /// JSON Schema of the params (ADR 0009 subset), at most `UMBRA_MAX_SCHEMA_LEN` bytes.
    pub param_schema: UmbraStr,
    /// Kind-specific JSON metadata (behavior: ADR 0020), at most `UMBRA_MAX_METADATA_LEN` bytes; may be
    /// empty.
    pub metadata: UmbraStr,
    /// The kind's vtable (behavior: `UmbraBehaviorVTable`), 8-byte aligned, non-null.
    pub vtable: *const c_void,
}

/// Type of the one exported symbol `umbra_plugin_entry` (REQ-P-1). Called by the host with a pointer to
/// the image's [`UmbraHostApi`]; returns the plugin's static descriptor, or null on failure. It may be
/// called more than once in a lifecycle and must return the same pointer each time.
pub type UmbraPluginEntry =
    unsafe extern "C" fn(host: *const UmbraHostApi) -> *const UmbraPluginDescriptor;
