//! `cgf-behavior`: the behavior graph interpreter for the CGF Engine.
//!
//! Responsibility (M3): compile validated behavior graphs into runnable instances and tick them
//! (composites, decorators, finite state machines, subgraphs, built-in conditions and actions),
//! serving REQ-B-1, REQ-B-2, REQ-B-3 (layer 1) and REQ-B-5 to REQ-B-17, including the determinism
//! rule REQ-B-16. WI-8 to WI-10 deliver the interpreter (composites, decorators, `fsm`, `subgraph`
//! with input bindings and outputs, halt) and `register_graphs`; WI-11 adds the built-in leaves.
//!
//! Dependency rule (spec section 4): depends on `cgf-core` only among workspace
//! crates; never on `cgf-kernel`, `cgf-templates` or `cgf-engine`. It contains no `unsafe` code.

#![forbid(unsafe_code)]

mod factory;
mod instance;
mod leaves;
mod nodes;
mod program;
mod register;

pub use factory::{GraphFactory, GraphRegistrationError};
pub use register::{register_builtins, register_graphs};
