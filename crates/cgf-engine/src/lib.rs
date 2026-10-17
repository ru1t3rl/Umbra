//! `cgf-engine`: the embeddable, synchronous facade over kernel and motion models.
//!
//! Responsibility (filled in by later M1 work items): `Engine` construction, running and stepping
//! the kernel, and the hard-coded M1 demo scenario (REQ-S-2 minimal, ASM-026, ASM-028).
//!
//! Dependency rule (REQ-A-2): depends on `cgf-core`, `cgf-kernel` and `cgf-motion`. It contains
//! no `unsafe` code.

#![forbid(unsafe_code)]

pub mod demo;
pub mod engine;
pub mod error;

pub use engine::{Engine, EngineConfig, RouteSpec, RunSummary, RunUntil, SpawnSpec, TickView};
pub use error::EngineError;
