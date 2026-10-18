//! `umbra-plugin-host`: loads native Umbra plugins and bridges them to the core traits (D-6, D-8).
//!
//! Single responsibility: discover plugin packages, check `plugin.toml` and the ABI version, open the
//! library, copy and validate the descriptor, run the plugin lifecycle, and register each capability
//! in the one `umbra_core::Registry` with `Provenance::Plugin` (REQ-P-1..10, ADR 0019-0022). Plugin
//! faults follow `on_plugin_fault` per host (REQ-P-7).
//!
//! Dependency rule (section 4): `umbra-core` and `umbra-plugin-api` only.
//!
//! This is one of the three crates allowed to contain `unsafe` (CLAUDE.md rule 5, NFR-2). Every
//! `unsafe` block carries a `// SAFETY:` comment naming the invariant it relies on and is exercised by
//! a test. Loading lands in M4 WI-8, the behavior bridge in WI-9.

#![deny(unsafe_op_in_unsafe_fn)]
#![warn(clippy::undocumented_unsafe_blocks)]
