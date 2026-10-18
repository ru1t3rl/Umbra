//! `umbra-sdk`: write Umbra plugins in safe Rust (REQ-SDK-1, D-7).
//!
//! Single responsibility: safe traits, conversions and panic guards over the C ABI of
//! [`umbra-plugin-api`](abi), and the `export_plugin!` macro that emits the one exported symbol
//! `umbra_plugin_entry` (REQ-P-1). Plugin crates built with the SDK keep `#![forbid(unsafe_code)]`
//! (ADR 0023, amended R-M5).
//!
//! Dependency rule (REQ-A-3): the only workspace dependency is `umbra-plugin-api`.
//!
//! This is one of the three crates allowed to contain `unsafe` (CLAUDE.md rule 5, NFR-2). Every
//! `unsafe` block carries a `// SAFETY:` comment naming the invariant it relies on and is exercised by
//! a test. The traits and the macro land in M4 WI-7.

#![deny(unsafe_op_in_unsafe_fn)]
#![warn(clippy::undocumented_unsafe_blocks)]

/// The raw C ABI types the SDK is built on (`umbra-plugin-api`), re-exported so that code generated
/// by `export_plugin!` can name them as `$crate::abi::..` without its own dependency.
pub use umbra_plugin_api as abi;
