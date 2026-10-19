//! `umbra-kernel`: the deterministic simulation kernel of Umbra.
//!
//! Responsibility: the simulation clock and its modes
//! (REQ-K-3), the ECS world (REQ-K-1, ADR 0005), the fixed six-phase tick pipeline (REQ-K-2),
//! per-tick timings (REQ-K-8) and the state hash (REQ-K-4, ADR 0007).
//!
//! The Perception phase runs sensors and the Behavior phase ticks instantiated behaviors (M3,
//! ADR 0013, ADR 0015); entities without a behavior keep the M1 route-following stand-in
//! (ASM-029). A behavior that offers a batch (M4, ADR 0020) is ticked with its group: one
//! `tick_batch` call per group per tick, groups ordered by their first entity id, members in id
//! order; the group buffers live in the world and are reused. The Motion phase runs each entity's motion model. All three run on `Owned`
//! entities only (REQ-K-5). The kernel owns the
//! master `SimRng` and gives each entity its own stream. `Kernel::snapshot` is synchronous and
//! sorted by entity id (ASM-039); the observer buffer arrives in M5.
//!
//! Dependency rule (REQ-A-2): depends on `umbra-core` only among workspace crates; never on
//! `umbra-motion`, `umbra-engine` or `umbra-cli`. It contains no `unsafe` code.

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
pub use kernel::{
    AgentInit, EntityInit, EntitySnapshot, Kernel, KernelConfig, RouteInit, Snapshot,
};
pub use pipeline::{PHASE_COUNT, Phase, TickReport};
