//! STUB (WI-10, WI-11): the kernel facade, spawning, state hash and snapshot.
#![expect(
    missing_docs,
    clippy::todo,
    reason = "stub until WI-10/WI-11: signatures only, documented when implemented"
)]

use std::sync::Arc;

use cgf_core::{
    Ecef, EntityId, KinematicState, Meters, MetersPerSecond, MotionModel, MotionModelId, Ownership,
    SimRng, SimTime, StateHash, TerrainProvider, TickRate,
};

use crate::clock::{Clock, ClockMode, WallClock};
use crate::error::KernelError;
use crate::pipeline::TickReport;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KernelConfig {
    pub tick_rate: TickRate,
    pub seed: u64,
    pub clock: ClockMode,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RouteInit {
    pub waypoints: Vec<Ecef>,
    pub arrival_radius: Meters,
    pub cruise_speed: MetersPerSecond,
}

#[derive(Clone)]
pub struct EntityInit {
    pub state: KinematicState,
    pub ownership: Ownership,
    pub motion: Arc<dyn MotionModel>,
    pub route: Option<RouteInit>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    pub time: SimTime,
    pub entities: Vec<EntitySnapshot>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntitySnapshot {
    pub id: EntityId,
    pub ownership: Ownership,
    pub motion_model: MotionModelId,
    pub state: KinematicState,
    /// `(next, len)` of the route, if any.
    pub route_next: Option<(usize, usize)>,
}

impl EntitySnapshot {
    /// Position converted for display (ASM-044).
    pub fn geodetic(&self) -> cgf_core::Geodetic {
        self.state.position.to_geodetic()
    }
}

pub struct Kernel {
    clock: Clock,
    rng: SimRng,
    _terrain: Box<dyn TerrainProvider>,
}

impl Kernel {
    pub fn new(
        _config: KernelConfig,
        _terrain: Box<dyn TerrainProvider>,
        _wall: Box<dyn WallClock>,
    ) -> Self {
        todo!("WI-10")
    }
    pub fn spawn(&mut self, _init: EntityInit) -> Result<EntityId, KernelError> {
        todo!("WI-10")
    }
    pub fn tick(&mut self) -> Result<TickReport, KernelError> {
        todo!("WI-11")
    }
    pub fn clock(&self) -> &Clock {
        &self.clock
    }
    pub fn clock_mut(&mut self) -> &mut Clock {
        &mut self.clock
    }
    pub fn now(&self) -> SimTime {
        self.clock.now()
    }
    pub fn rng(&self) -> SimRng {
        self.rng
    }
    pub fn entity_count(&self) -> usize {
        todo!("WI-10")
    }
    pub fn state_hash(&self) -> StateHash {
        todo!("WI-10")
    }
    pub fn snapshot(&self) -> Snapshot {
        todo!("WI-10")
    }
}
