//! `umbra-plugin-api`: the Umbra plugin C ABI, version 0 (D-6, REQ-P-1..6, ADR 0019).
//!
//! Single responsibility: the `#[repr(C)]` types, function-pointer tables and version constants that
//! the host (`umbra-plugin-host`), the Rust SDK (`umbra-sdk`) and C plugins (through the generated
//! `include/umbra_plugin_api.h`, REQ-SDK-2) share. It contains no logic and has no dependencies at all
//! (section 4, ASM-009, REQ-A-3).
//!
//! Conventions (ADR 0019): every growable struct starts with `abi_version` and `struct_size`, has no
//! implicit padding (gaps are `reserved_N` fields that writers zero and readers ignore) and a size that
//! is a multiple of 8; enums are `u32` typedefs with `UMBRA_*` constants; booleans are `u8`; every
//! function-pointer field is nullable (`Option`); strings are UTF-8 [`UmbraStr`]. Host contexts are
//! opaque [`UmbraToken`]s. Layouts are pinned by `tests/layout.rs` and checked from C by the generated
//! asserts of `umbra-plugin-host/tests/c_header.rs`. The ABI 0.0 layout is final as of M4 WI-14 and
//! frozen at the M4 close-out (WI-16); after that it changes only append-only, by a minor bump (REQ-P-2,
//! ASM-117). The plugin author's reference is `docs/plugin-guide.md`.
//!
//! The ABI supports 64-bit targets only (ADR 0019, ASM-120); the build fails elsewhere.
//!
//! This crate defines function-pointer *types* (`unsafe extern "C" fn`) but contains no `unsafe`
//! block, function or impl, so it forbids `unsafe_code` (NFR-2).

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

// ADR 0019 (amended R-M7), ASM-120: every layout in this crate assumes 8-byte pointers and `usize`.
const _: () = assert!(
    size_of::<usize>() == 8,
    "Umbra plugin ABI v0 supports 64-bit targets only"
);

mod behavior;
mod consts;
mod host;
mod types;

pub use behavior::{
    UmbraBehavior, UmbraBehaviorBatch, UmbraBehaviorItem, UmbraBehaviorVTable, UmbraContact,
    UmbraEntityView, UmbraInstance, UmbraIntent, UmbraIntentSink, UmbraValue, UmbraValueSink,
};
pub use consts::*;
pub use host::{UmbraCapability, UmbraHostApi, UmbraPluginDescriptor, UmbraPluginEntry};
pub use types::{UmbraErrorSink, UmbraQuat, UmbraStr, UmbraVec3};
