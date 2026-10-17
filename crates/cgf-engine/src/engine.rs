//! STUB (WI-13): the embeddable synchronous facade (REQ-S-2 minimal, ASM-026).
//!
//! Signatures only. Acceptance tests: `tests/determinism.rs`, `tests/waypoints.rs`.
#![expect(
    missing_docs,
    clippy::todo,
    reason = "stub until WI-13: signatures only, documented when implemented"
)]

use std::sync::Arc;

use cgf_core::{
    EntityId, Meters, MetersPerSecond, MotionModel, Ownership, Position, Radians, SimTime,
    StateHash, TickRate,
};
use cgf_kernel::{ClockMode, Kernel, Snapshot, TickReport, WallClock};

use crate::error::EngineError;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EngineConfig {
    pub seed: u64,
    pub tick_rate: TickRate,
    pub clock: ClockMode,
    pub terrain_height: Meters,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunUntil {
    /// Total ticks since start.
    Ticks(u64),
    /// Until the clock reports idle.
    Idle,
}

/// Engine-level placement (ASM-044). Heading is clockwise from true north.
#[derive(Clone)]
pub struct SpawnSpec {
    pub position: Position,
    pub heading: Radians,
    pub speed: MetersPerSecond,
    pub ownership: Ownership,
    pub motion: Arc<dyn MotionModel>,
    pub route: Option<RouteSpec>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RouteSpec {
    pub waypoints: Vec<Position>,
    pub arrival_radius: Meters,
    pub cruise_speed: MetersPerSecond,
}

pub struct TickView<'a> {
    pub report: &'a TickReport,
    kernel: &'a Kernel,
}

impl TickView<'_> {
    pub fn state_hash(&self) -> StateHash {
        self.kernel.state_hash()
    }
    pub fn snapshot(&self) -> Snapshot {
        self.kernel.snapshot()
    }
}

/// Summary of one `run` or `step` call. Fields are not specified by the plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct RunSummary {}

pub struct Engine {
    kernel: Kernel,
}

impl Engine {
    pub fn new(_config: EngineConfig) -> Self {
        todo!("WI-13")
    }
    pub fn with_wall_clock(_config: EngineConfig, _wall: Box<dyn WallClock>) -> Self {
        todo!("WI-13")
    }
    pub fn spawn(&mut self, _spec: SpawnSpec) -> Result<EntityId, EngineError> {
        todo!("WI-13")
    }
    pub fn run(
        &mut self,
        _until: RunUntil,
        _on_tick: impl FnMut(&TickView<'_>),
    ) -> Result<RunSummary, EngineError> {
        todo!("WI-13")
    }
    pub fn step(
        &mut self,
        _n: u64,
        _on_tick: impl FnMut(&TickView<'_>),
    ) -> Result<RunSummary, EngineError> {
        todo!("WI-13")
    }
    pub fn pause(&mut self) {
        todo!("WI-13")
    }
    pub fn resume(&mut self) {
        todo!("WI-13")
    }
    pub fn set_clock(&mut self, _m: ClockMode) {
        todo!("WI-13")
    }
    pub fn now(&self) -> SimTime {
        self.kernel.now()
    }
    pub fn state_hash(&self) -> StateHash {
        self.kernel.state_hash()
    }
    pub fn snapshot(&self) -> Snapshot {
        self.kernel.snapshot()
    }
}
