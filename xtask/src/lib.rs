//! Repository tooling library for `cargo xtask` (ASM-002).
//!
//! Provides the dependency-rule check (`deps`, REQ-A-1..3, NFR-2) and the C header generation and
//! drift check (`header`, REQ-SDK-2, gate 11).

#![forbid(unsafe_code)]

pub mod deps;
pub mod header;
