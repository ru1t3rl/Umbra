//! The kernel facade: spawning, ticking, state hash and snapshot (REQ-K-1, REQ-K-2, REQ-K-4,
//! REQ-K-5, REQ-K-6 partial, REQ-K-8).

use core::time::Duration;
use std::sync::Arc;

use umbra_core::{
    Behavior, BehaviorId, Contact, Ecef, EntityId, Force, Intent, KinematicState, Meters,
    MetersPerSecond, MotionModel, MotionModelId, Ownership, Sensor, SimRng, SimTime, StateHash,
    Status, TerrainProvider, TickRate,
};

use crate::clock::{Clock, ClockMode, WallClock};
use crate::error::KernelError;
use crate::pipeline::{PHASE_COUNT, Phase, TickContext, TickReport};
use crate::state;
use crate::systems::{behavior, motion, perception};
use crate::world::{BehaviorSlot, World};

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

/// Behavior-side parts of an entity (ADR 0011); `AgentInit::default()` spawns exactly like
/// [`Kernel::spawn`] (M3 plan, `umbra-kernel` changes).
#[derive(Clone, Default)]
pub struct AgentInit {
    /// The entity's force; `None` is neutral to everyone (ASM-079).
    pub force: Option<Force>,
    /// The behavior program; the kernel instantiates it at the tick rate on spawn.
    pub behavior: Option<Arc<dyn Behavior>>,
    /// Sensors in template order (first wins when contacts coincide).
    pub sensors: Vec<Arc<dyn Sensor>>,
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
    /// The entity's force, if any.
    pub force: Option<Force>,
    /// The id of the entity's behavior, if it has one.
    pub behavior: Option<BehaviorId>,
    /// The behavior's status in the last tick, if it has run.
    pub status: Option<Status>,
    /// The intents of the last tick (after route expansion).
    pub intents: Vec<Intent>,
    /// The contacts of the last Perception phase, sorted by contact entity id.
    pub contacts: Vec<Contact>,
}

impl EntitySnapshot {
    /// Position converted for display (ASM-044).
    pub fn geodetic(&self) -> umbra_core::Geodetic {
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

    /// Adds an entity with a behavior, sensors and a force (ADR 0011). `spawn(init)` is
    /// `spawn_with(init, AgentInit::default())`.
    ///
    /// # Errors
    /// The errors of [`Kernel::spawn`], and [`KernelError::Instantiate`] when the behavior cannot
    /// be instantiated at the tick rate. The kernel is unchanged on error (no id consumed).
    pub fn spawn_with(
        &mut self,
        init: EntityInit,
        agent: AgentInit,
    ) -> Result<EntityId, KernelError> {
        let rate = self.clock.now().rate();
        let AgentInit {
            force,
            behavior,
            sensors,
        } = agent;
        // Instantiation runs after the entity has been validated (ASM-093 (g)).
        let slot = move || {
            behavior
                .map(|b| {
                    b.instantiate(rate).map(|instance| BehaviorSlot {
                        id: b.id().clone(),
                        instance,
                        last: None,
                    })
                })
                .transpose()
                .map_err(KernelError::from)
        };
        self.world.spawn_with(init, force, sensors, slot, self.rng)
    }

    /// Runs one tick through the six phases in [`Phase::ALL`] order and advances the clock by
    /// exactly one tick. Pacing is the caller's business; this never waits.
    ///
    /// # Stack
    /// Behavior graphs are ticked by recursion, one or two frames per node level (up to 128 levels
    /// per graph, 16 graphs deep). The calling thread needs at least 2 MiB in a debug build
    /// (worst case measured 1.25-1.5 MiB, ASM-098); drive the kernel from a thread with an
    /// explicit stack of 4 MiB or more.
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
        let mut phase_wall = [Duration::ZERO; PHASE_COUNT];
        let tick_started = self.clock.wall_now();
        for (slot, phase) in phase_wall.iter_mut().zip(Phase::ALL) {
            let _span = tracing::trace_span!("phase", phase = phase.name()).entered();
            let started = self.clock.wall_now();
            run_phase(phase, &mut self.world, &ctx);
            *slot = self.clock.wall_now().saturating_sub(started);
        }
        let took = self.clock.wall_now().saturating_sub(tick_started);
        let time = self.clock.advance()?;
        let overrun = self.overrun(took, &phase_wall, time);
        Ok(TickReport {
            time,
            phase_wall,
            overrun,
            lag: self.clock.behind(),
        })
    }

    /// REQ-K-8 (ASM-050): by how much this tick's own duration exceeded the wall budget, with one
    /// warning for that tick. `None` when the clock has no wall budget or the tick fit.
    fn overrun(
        &self,
        took: Duration,
        phase_wall: &[Duration; PHASE_COUNT],
        time: SimTime,
    ) -> Option<Duration> {
        let budget = self.clock.wall_period()?;
        let over = took.checked_sub(budget).filter(|d| !d.is_zero())?;
        tracing::warn!(
            tick_run = time.tick().saturating_sub(1),
            ?took,
            ?budget,
            ?phase_wall,
            "tick exceeded its time budget"
        );
        Some(over)
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

    /// Total capacity, in elements, of the scratch vectors the grouped Behavior phase keeps in the
    /// world and reuses between ticks (ADR 0020). Diagnostic probe for the "no growth after warm-up"
    /// test (M4 AT-6); it never changes behavior or state.
    ///
    /// M4 WI-6 stub: WI-6 sums the capacities of the group and item scratch vectors.
    #[doc(hidden)]
    #[must_use]
    #[expect(clippy::todo, reason = "M4 WI-6 stub")]
    pub fn behavior_scratch_capacity(&self) -> usize {
        todo!("M4 WI-6")
    }
}

/// Runs the systems of one phase. Perception, Behavior and Motion act for `Owned` entities only
/// (REQ-K-5); Ingest, Events and Publish have no systems yet.
fn run_phase(phase: Phase, world: &mut World, ctx: &TickContext<'_>) {
    match phase {
        Phase::Perception => perception::run(world, ctx),
        Phase::Behavior => world.for_each_mut(|e| behavior::run(e, ctx)),
        Phase::Motion => world.for_each_mut(|e| {
            if e.ownership.is_owned() {
                motion::step(e.motion, e.state, &e.intents.0, ctx);
            }
        }),
        Phase::Ingest | Phase::Events | Phase::Publish => {}
    }
}

#[cfg(test)]
mod tests {
    use core::sync::atomic::{AtomicUsize, Ordering};

    use umbra_core::{FlatTerrain, Intent, Quaternion, Seconds, Vec3};

    use super::*;
    use crate::clock::ManualWallClock;

    struct Counting {
        id: MotionModelId,
        calls: AtomicUsize,
    }

    impl MotionModel for Counting {
        fn id(&self) -> &MotionModelId {
            &self.id
        }
        fn step(
            &self,
            state: &KinematicState,
            _intents: &[Intent],
            _dt: Seconds,
            _terrain: &dyn TerrainProvider,
        ) -> KinematicState {
            self.calls.fetch_add(1, Ordering::SeqCst);
            *state
        }
    }

    /// REQ-C-1: at `u64::MAX` the precheck returns `TimeOverflow` before any phase runs.
    #[test]
    fn tick_at_u64_max_overflows_before_running_any_phase() {
        let Some(rate) = TickRate::new(30) else {
            panic!("valid rate");
        };
        let Ok(id) = MotionModelId::new("test.stub.counting") else {
            panic!("valid id");
        };
        let model = Arc::new(Counting {
            id,
            calls: AtomicUsize::new(0),
        });
        let mut kernel = Kernel {
            clock: Clock::starting_at(
                SimTime::at_tick(rate, u64::MAX),
                ClockMode::AsFastAsPossible,
                Box::new(ManualWallClock::new()),
            ),
            rng: SimRng::new(1),
            terrain: Box::new(FlatTerrain {
                height: Meters(0.0),
            }),
            world: World::new(),
        };
        let init = EntityInit {
            state: KinematicState {
                position: Ecef(Vec3::new(6_378_137.0, 0.0, 0.0)),
                velocity: Vec3::ZERO,
                orientation: Quaternion::IDENTITY,
            },
            ownership: Ownership::Owned,
            motion: model.clone(),
            route: None,
        };
        assert!(kernel.spawn(init).is_ok());

        let got = kernel.tick();

        assert_eq!(
            got.map(|r| r.time),
            Err(KernelError::TimeOverflow(u64::MAX))
        );
        assert_eq!(model.calls.load(Ordering::SeqCst), 0);
        assert_eq!(kernel.now().tick(), u64::MAX);
    }
}
