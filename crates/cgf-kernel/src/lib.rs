//! `cgf-kernel`: the deterministic simulation kernel of the CGF Engine.
//!
//! Responsibility: the simulation clock and its modes
//! (REQ-K-3), the ECS world (REQ-K-1, ADR 0005), the fixed six-phase tick pipeline (REQ-K-2),
//! per-tick timings (REQ-K-8) and the state hash (REQ-K-4, ADR 0007).
//!
//! In M1 the Behavior phase hosts a route-following stand-in (ASM-029) and the Motion phase runs
//! each entity's motion model; both run on `Owned` entities only (REQ-K-5). The kernel owns the
//! master `SimRng` and gives each entity its own stream. `Kernel::snapshot` is synchronous and
//! sorted by entity id (ASM-039); the observer buffer arrives in M5.
//!
//! Dependency rule (REQ-A-2): depends on `cgf-core` only among workspace crates; never on
//! `cgf-motion`, `cgf-engine` or `cgf-cli`. It contains no `unsafe` code.

#![forbid(unsafe_code)]

pub mod clock;
pub mod error;
pub mod kernel;
pub mod pipeline;
mod state;
mod systems;
mod world;

pub use clock::{
    Clock, ClockError, ClockMode, ManualWallClock, Pace, StdWallClock, TimeScale, WallClock,
};
pub use error::{KernelError, RouteParam};
pub use kernel::{EntityInit, EntitySnapshot, Kernel, KernelConfig, RouteInit, Snapshot};
pub use pipeline::{Phase, TickContext, TickReport};
