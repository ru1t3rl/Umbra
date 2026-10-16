//! Repository tooling library for `cargo xtask` (ASM-002).
//!
//! Currently provides the dependency-rule check (`deps`, REQ-A-1..3, NFR-2).

#![forbid(unsafe_code)]

pub mod deps;
