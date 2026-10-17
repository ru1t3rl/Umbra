//! `cgf-behavior`: the behavior graph interpreter for the CGF Engine.
//!
//! Responsibility (M3): compile validated behavior graphs into runnable instances and tick them
//! (composites, decorators, finite state machines, subgraphs, built-in conditions and actions),
//! serving REQ-B-1, REQ-B-5 to REQ-B-15 and REQ-B-17. This crate is empty until WI-5.
//!
//! Dependency rule (spec section 4, REQ-A-1..3): depends on `cgf-core` only among workspace
//! crates; never on `cgf-kernel`, `cgf-templates` or `cgf-engine`. It contains no `unsafe` code.

#![forbid(unsafe_code)]
