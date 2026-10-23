//! Per-call host services of ABI 0.2 (ADR 0050, human decision H2): terrain queries for motion models
//! and sensors, and blackboard access for conditions and actions.
//!
//! A service is a struct of a host token plus function pointers (the sink pattern of ADR 0019), passed
//! as a pointer inside the call record of the calls that have it: terrain in `UmbraMotionStep` and
//! `UmbraSenseCall`, the blackboard in `UmbraLeafCall`. It is written by the host in the plugin's
//! declared minor and valid only during that call, like its token (REQ-P-11, ADR 0021 RR-M2): a token
//! used after the call, or in another call, is refused with an error code and faults the capability.
//! Callbacks are leaf calls: they never call into any plugin.

use crate::behavior::{UmbraValue, UmbraValueSink};
use crate::consts::{UmbraCode, UmbraToken};
use crate::types::{UmbraStr, UmbraVec3};

/// Terrain queries of one motion `step` or sensor `sense` call (ADR 0050). Answers come from the
/// `TerrainProvider` the core call received, so a plugin sees exactly what a built-in sees (LSP).
/// Every out-pointer is written only on `UMBRA_OK`.
#[repr(C)]
pub struct UmbraTerrain {
    /// ABI version the host was compiled against.
    pub abi_version: u32,
    /// `sizeof` this struct in the layout of the plugin's declared minor.
    pub struct_size: u32,
    /// Opaque host token of this service in this call, passed back to every entry.
    pub ctx: UmbraToken,
    /// Writes the terrain height (m above the ellipsoid) under the ECEF point `*ecef` into `*out_m`.
    /// Required. A non-finite input is refused (`UMBRA_ERR_INVALID_ARGUMENT`).
    pub height_at: Option<
        unsafe extern "C" fn(ctx: UmbraToken, ecef: *const UmbraVec3, out_m: *mut f64) -> UmbraCode,
    >,
    /// Writes the unit terrain normal (local east-north-up) under the ECEF point `*ecef` into
    /// `*out_enu`. Required.
    pub normal_at: Option<
        unsafe extern "C" fn(
            ctx: UmbraToken,
            ecef: *const UmbraVec3,
            out_enu: *mut UmbraVec3,
        ) -> UmbraCode,
    >,
    /// Writes 1 into `*out` if the terrain does not block the line from `*from` to `*to` (ECEF), else
    /// 0. Required.
    pub line_of_sight: Option<
        unsafe extern "C" fn(
            ctx: UmbraToken,
            from: *const UmbraVec3,
            to: *const UmbraVec3,
            out: *mut u8,
        ) -> UmbraCode,
    >,
}

/// Blackboard access of one condition `check` or action `tick` call (ADR 0050, ASM-618). The host
/// grants exactly the keys named by the leaf's declared `blackboard_params` (resolved from its params
/// at configure), with their declared type and access; any other key, a wrong type, a non-finite value
/// or a write to a read-only key is refused with an error code and faults the capability.
#[repr(C)]
pub struct UmbraBlackboard {
    /// ABI version the host was compiled against.
    pub abi_version: u32,
    /// `sizeof` this struct in the layout of the plugin's declared minor.
    pub struct_size: u32,
    /// Opaque host token of this service in this call, passed back to every entry.
    pub ctx: UmbraToken,
    /// Reports the value of `key` (UTF-8, borrowed during the call) through `sink`: the value, or
    /// "unset" (a null value) when the key holds none. Required.
    pub get: Option<
        unsafe extern "C" fn(
            ctx: UmbraToken,
            key: UmbraStr,
            sink: *const UmbraValueSink,
        ) -> UmbraCode,
    >,
    /// Sets `key` to `*value`, or unsets it when `value` is null; both borrowed during the call (the
    /// host copies the value). Required.
    pub set: Option<
        unsafe extern "C" fn(ctx: UmbraToken, key: UmbraStr, value: *const UmbraValue) -> UmbraCode,
    >,
}
