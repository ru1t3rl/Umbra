//! `umbra-plugin-api`: the Umbra plugin C ABI, version 0 (D-6, REQ-P-1..6, ADR 0019).
//!
//! Single responsibility: the `#[repr(C)]` types, function-pointer tables and version constants that
//! the host (`umbra-plugin-host`), the Rust SDK (`umbra-sdk`) and C plugins (through the generated
//! `include/umbra_plugin_api.h`, REQ-SDK-2) share. It contains no logic and has no dependencies at all
//! (section 4, ASM-009, REQ-A-3).
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
