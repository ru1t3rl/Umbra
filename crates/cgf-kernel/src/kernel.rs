//! The kernel facade: spawning, ticking, state hash and snapshot (REQ-K-1, REQ-K-2, REQ-K-4,
//! REQ-K-5, REQ-K-6 partial, REQ-K-8).

use core::time::Duration;
use std::sync::Arc;

use cgf_core::{
    Ecef, EntityId, KinematicState, Meters, MetersPerSecond, MotionModel, MotionModelId, Ownership,
    SimRng, SimTime, StateHash, TerrainProvider, TickRate,
};

use crate::clock::{Clock, ClockMode, WallClock};
use crate::error::KernelError;
use crate::pipeline::{Phase, TickContext, TickReport};
use crate::state;
use crate::systems::{motion, route};
use crate::world::World;

/// Kernel construction parameters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KernelConfig {
    /// Ticks per second.
    pub tick_rate: TickRate,
    /// Master seed of the simulation's random streams (ADR 0006).
    pub seed: u64,
    /// Initial clock mode.
    pub clock: ClockMode,
}

/// A waypoint route for the M1 route-following stand-in (ASM-029).
#[derive(Debug, Clone, PartialEq)]
pub struct RouteInit {
    /// Waypoints in visiting order, ECEF; at least one.
    pub waypoints: Vec<Ecef>,
    /// A waypoint counts as reached within this horizontal distance; finite and > 0.
    pub arrival_radius: Meters,
    /// Speed requested while following the route; finite and > 0.
    pub cruise_speed: MetersPerSecond,
}

/// Everything needed to spawn one entity. Positions are ECEF (D-15).
#[derive(Clone)]
pub struct EntityInit {
    /// Initial kinematic state.
    pub state: KinematicState,
    /// Whether this kernel simulates the entity (REQ-K-5).
    pub ownership: Ownership,
    /// The motion model instance for this entity (REQ-M-4); may be shared.
    pub motion: Arc<dyn MotionModel>,
    /// Optional route for the stand-in behavior.
    pub route: Option<RouteInit>,
}

/// An immutable copy of the world, entities sorted by id (REQ-K-6 partial, ASM-039).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Snapshot {
    /// Simulation time after the last completed tick.
    pub time: SimTime,
    /// All entities in ascending id order.
    pub entities: Vec<EntitySnapshot>,
}

/// One entity in a [`Snapshot`].
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct EntitySnapshot {
    /// The entity id.
    pub id: EntityId,
    /// Owned or remote.
    pub ownership: Ownership,
    /// Id of the entity's motion model.
    pub motion_model: MotionModelId,
    /// Kinematic state, ECEF.
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

/// The deterministic simulation kernel: clock, world, terrain and the fixed tick pipeline.
///
/// The kernel never paces itself: a driver polls [`Clock::poll`] and calls [`Kernel::tick`] when
/// told to (the engine facade does this). Entities are spawned between ticks only (ASM-036).
pub struct Kernel {
    clock: Clock,
    rng: SimRng,
    terrain: Box<dyn TerrainProvider>,
    world: World,
}

impl Kernel {
    /// Creates an empty kernel at tick 0.
    pub fn new(
        config: KernelConfig,
        terrain: Box<dyn TerrainProvider>,
        wall: Box<dyn WallClock>,
    ) -> Self {
        Self {
            clock: Clock::new(config.tick_rate, config.clock, wall),
            rng: SimRng::new(config.seed),
            terrain,
            world: World::new(),
        }
    }

    /// Adds an entity under the next id (from 1, never reused; REQ-C-2).
    ///
    /// # Errors
    /// [`KernelError::EmptyRoute`] or [`KernelError::InvalidRoute`] for a bad route,
    /// [`KernelError::IdsExhausted`] when ids run out. The kernel is unchanged on error.
    pub fn spawn(&mut self, init: EntityInit) -> Result<EntityId, KernelError> {
        self.world.spawn(init, self.rng)
    }

    /// Runs one tick through the six phases in [`Phase::ALL`] order and advances the clock by
    /// exactly one tick. Pacing is the caller's business; this never waits.
    ///
    /// # Errors
    /// [`KernelError::TimeOverflow`] if the tick counter is exhausted; nothing runs then.
    pub fn tick(&mut self) -> Result<TickReport, KernelError> {
        let now = self.clock.now();
        if now.checked_next().is_none() {
            return Err(KernelError::TimeOverflow(now.tick()));
        }
        let ctx = TickContext {
            now,
            dt: now.dt(),
            terrain: self.terrain.as_ref(),
        };
        let mut phase_wall = [Duration::ZERO; 6];
        for (slot, phase) in phase_wall.iter_mut().zip(Phase::ALL) {
            let _span = tracing::trace_span!("phase", phase = phase.name()).entered();
            let started = self.clock.wall_now();
            run_phase(phase, &mut self.world, &ctx);
            *slot = self.clock.wall_now().saturating_sub(started);
        }
        let time = self.clock.advance()?;
        let overrun = self.clock.behind();
        if let Some(lag) = overrun {
            tracing::warn!(
                tick = time.tick(),
                ?lag,
                ?phase_wall,
                "tick overran its period"
            );
        }
        Ok(TickReport {
            time,
            phase_wall,
            overrun,
        })
    }

    /// The clock.
    pub fn clock(&self) -> &Clock {
        &self.clock
    }

    /// The clock, mutably (pause, mode, steps, polling).
    pub fn clock_mut(&mut self) -> &mut Clock {
        &mut self.clock
    }

    /// The current simulation time.
    pub fn now(&self) -> SimTime {
        self.clock.now()
    }

    /// The master random source.
    pub fn rng(&self) -> SimRng {
        self.rng
    }

    /// Number of entities.
    pub fn entity_count(&self) -> usize {
        self.world.len()
    }

    /// The canonical state hash (ADR 0007), entities walked in id order.
    pub fn state_hash(&self) -> StateHash {
        state::state_hash(self.clock.now(), &self.world)
    }

    /// An immutable copy of the current state, sorted by id.
    pub fn snapshot(&self) -> Snapshot {
        state::snapshot(self.clock.now(), &self.world)
    }
}

/// Runs the systems of one phase. Behavior and Motion touch only `Owned` entities (REQ-K-5);
/// the other phases have no systems in M1.
fn run_phase(phase: Phase, world: &mut World, ctx: &TickContext<'_>) {
    match phase {
        Phase::Behavior => world.for_each_mut(|e| {
            e.intents.0.clear();
            if e.ownership.is_owned()
                && let Some(r) = e.route
            {
                route::steer(r, e.state.position, &mut e.intents.0);
            }
        }),
        Phase::Motion => world.for_each_mut(|e| {
            if e.ownership.is_owned() {
                motion::step(e.motion, e.state, &e.intents.0, ctx);
            }
        }),
        Phase::Ingest | Phase::Perception | Phase::Events | Phase::Publish => {}
    }
}
