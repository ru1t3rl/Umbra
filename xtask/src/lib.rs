//! Repository tooling library for `cargo xtask` (ASM-002).
//!
//! Provides the contract documents and their drift check (`api`, REQ-X-4, gate 10), the dependency-rule check (`deps`, REQ-A-1..3, NFR-2), the C header generation and
//! drift check (`header`, REQ-SDK-2, gate 11) and the plugin package build (`plugins`, gate 12).

#![forbid(unsafe_code)]

pub mod api;
pub mod deps;
pub mod header;
pub mod plugins;

/// The cargo program: `CARGO` (set by cargo for `cargo xtask` and tests), else `cargo` on `PATH`.
#[must_use]
pub fn cargo_program() -> std::ffi::OsString {
    std::env::var_os("CARGO").unwrap_or_else(|| std::ffi::OsString::from("cargo"))
}
