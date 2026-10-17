//! `cgf-kernel`: the deterministic simulation kernel of the CGF Engine.
//!
//! Responsibility (filled in by later M1 work items): the simulation clock and its modes
//! (REQ-K-3), the ECS world (REQ-K-1, ADR 0005), the fixed six-phase tick pipeline (REQ-K-2),
//! per-tick timings (REQ-K-8) and the state hash (REQ-K-4, ADR 0007).
//!
//! Dependency rule (REQ-A-2): depends on `cgf-core` only among workspace crates; never on
//! `cgf-motion`, `cgf-engine` or `cgf-cli`. It contains no `unsafe` code.

#![forbid(unsafe_code)]
