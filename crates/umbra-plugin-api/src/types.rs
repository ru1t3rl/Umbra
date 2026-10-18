//! Frozen value primitives and the error sink (ADR 0019).
//!
//! [`UmbraStr`], [`UmbraVec3`] and [`UmbraQuat`] have no header: they are frozen for the major and never
//! grow. Every other struct starts with `abi_version` and `struct_size` (REQ-P-2).

use crate::consts::UmbraToken;

/// A UTF-8 string slice: pointer plus length in bytes, **not** NUL-terminated (REQ-P-6). Written by
/// whichever side owns the bytes; borrowed by the reader for the duration of the call it was passed in
/// (strings in the descriptor: until the reader has copied them at load). `len == 0` allows a null
/// `ptr`; invalid UTF-8 is an error. Frozen: 16 bytes, no header.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct UmbraStr {
    /// First byte; may be null only when `len == 0`.
    pub ptr: *const u8,
    /// Length in bytes.
    pub len: usize,
}

/// A 3-vector of `f64` (ECEF metres or m/s), written and owned as part of the struct or array that holds
/// it. Frozen: 24 bytes, no header.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct UmbraVec3 {
    /// X component.
    pub x: f64,
    /// Y component.
    pub y: f64,
    /// Z component.
    pub z: f64,
}

/// A unit quaternion `w + xi + yj + zk` (orientation), written and owned as part of the struct that holds
/// it. Frozen: 32 bytes, no header.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct UmbraQuat {
    /// Scalar part.
    pub w: f64,
    /// I component.
    pub x: f64,
    /// J component.
    pub y: f64,
    /// K component.
    pub z: f64,
}

/// Host-provided sink for one error message of a call (ADR 0019). Written by the host, in the plugin's
/// declared minor; borrowed by the plugin only during the call it was passed to. The host copies the
/// message during `set`; the first message of a call wins, later ones are ignored; messages longer than
/// `UMBRA_MAX_ERROR_LEN` are truncated.
#[repr(C)]
pub struct UmbraErrorSink {
    /// ABI version the writer was compiled against (`UMBRA_ABI_VERSION`).
    pub abi_version: u32,
    /// `sizeof` this struct as the writer knows it.
    pub struct_size: u32,
    /// Opaque host token of this sink, passed back to `set`.
    pub ctx: UmbraToken,
    /// Records `message` (UTF-8, borrowed during the call). Required (non-null).
    pub set: Option<unsafe extern "C" fn(ctx: UmbraToken, message: UmbraStr)>,
}
