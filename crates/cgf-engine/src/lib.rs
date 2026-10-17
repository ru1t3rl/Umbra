//! `cgf-engine`: the embeddable, synchronous facade over kernel and motion models.
//!
//! Responsibility: `Engine` construction, spawning with placement by ECEF or lat/lon/alt
//! (ASM-044), running and stepping the kernel by its clock, and the hard-coded M1 demo scenario
//! (REQ-S-2 minimal, ASM-026, ASM-028).
//!
//! This crate re-exports the core, kernel and motion types that `cgf-cli` needs, because the
//! dependency rules let the CLI depend on `cgf-engine` only (ASM-026).
//!
//! Dependency rule (REQ-A-2): depends on `cgf-core`, `cgf-kernel` and `cgf-motion`. It contains
//! no `unsafe` code.

#![forbid(unsafe_code)]

pub mod builtins;
pub mod demo;
pub mod display;
pub mod engine;
pub mod error;
pub mod scenario;

pub use cgf_core::{
    ComponentId, ComponentKind, EntityId, FlatTerrain, Geodetic, Meters, ParamSchema, ParamValue,
    Position, Radians, Registry, SimTime, StateHash, TerrainProvider, TickRate, TimeError,
};
pub use cgf_kernel::{
    ClockError, ClockMode, EntitySnapshot, ManualWallClock, Snapshot, TickReport, TimeScale,
    WallClock,
};
pub use cgf_templates::{SCENARIO_FILE_SCHEMA, TEMPLATE_FILE_SCHEMA};
pub use display::EntityDisplay;
pub use engine::{Engine, EngineConfig, RouteSpec, RunSummary, RunUntil, SpawnSpec, TickView};
pub use error::{EngineError, SpawnParam};
