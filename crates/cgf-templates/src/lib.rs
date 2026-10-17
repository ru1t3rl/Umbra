//! `cgf-templates`: template and scenario files for the CGF Engine.
//!
//! Responsibility (M2): read TOML templates and scenarios with source locations, resolve
//! inheritance, validate component parameters against the schemas registered in
//! `cgf_core::Registry`, and report every error with `path:line:col` (REQ-T-1..5, REQ-T-3).
//! Components are named by ID only; this crate never sees a concrete motion model or
//! terrain type.
//!
//! Dependency rule (REQ-A-1..3): depends on `cgf-core` only among workspace crates; never on
//! `cgf-kernel` or `cgf-motion`. It contains no `unsafe` code.
//!
//! This is the WI-1 scaffold; the modules arrive in later M2 work items.

#![forbid(unsafe_code)]
