//! The kernel facade: spawning, ticking, state hash and snapshot (REQ-K-1, REQ-K-2, REQ-K-4,
//! REQ-K-5, REQ-K-6 partial, REQ-K-8).

use core::time::Duration;
use std::sync::Arc;

use umbra_core::dead_reckoning::RemoteState;
pub use umbra_core::net::OwnedView;
use umbra_core::{
    Behavior, BehaviorId, Contact, Ecef, EntityId, Force, Intent, KinematicState, Meters,
    MetersPerSecond, MotionModel, MotionModelId, Ownership, RemoteSource, Seconds, Sensor,
    SimEvent, SimRng, SimTime, StateHash, StateHashVersion, Status, TerrainProvider, TickRate,
};

use crate::clock::{Clock, ClockMode, WallClock};
use crate::error::KernelError;
use crate::pipeline::{PHASE_COUNT, Phase, TickContext, TickReport};
use crate::state;
use crate::systems::{behavior, events, ingest, motion, perception};
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
                        batch: b.batch(),
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
        let mut sim_events = Vec::new();
        let mut lifecycle = Vec::new();
        let tick_started = self.clock.wall_now();
        for (slot, phase) in phase_wall.iter_mut().zip(Phase::ALL) {
            let _span = tracing::trace_span!("phase", phase = phase.name()).entered();
            let started = self.clock.wall_now();
            run_phase(
                phase,
                &mut self.world,
                &ctx,
                &mut sim_events,
                &mut lifecycle,
            );
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
            events: sim_events,
            lifecycle,
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

    /// Removes an entity between ticks (ADR 0028). Its behavior instance is dropped, so a plugin
    /// instance is destroyed here, outside any tick, and it leaves its batch group (the next
    /// grouped Behavior phase builds its groups from the live entities). Ids are never reused
    /// (REQ-C-2); every other entity is unaffected.
    ///
    /// # Errors
    /// [`KernelError::UnknownEntity`] when no such entity exists. The kernel is unchanged then.
    pub fn remove(&mut self, id: EntityId) -> Result<(), KernelError> {
        self.world.remove(id)
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

    /// The canonical state hash in the given encoding (ADR 0055). `V2` equals
    /// [`Kernel::state_hash`]. M7 WI-15 implements `V3`; until then this is a stub.
    pub fn state_hash_with(&self, version: StateHashVersion) -> StateHash {
        let _ = version;
        unimplemented!("M7 WI-15")
    }

    /// An immutable copy of the current state, sorted by id.
    pub fn snapshot(&self) -> Snapshot {
        state::snapshot(self.clock.now(), &self.world)
    }

    /// Total capacity, in elements, of the scratch vectors the grouped Behavior phase keeps in the
    /// world and reuses between ticks (ADR 0020). Diagnostic probe for the "no growth after warm-up"
    /// test (M4 AT-6); it never changes behavior or state.
    #[doc(hidden)]
    #[must_use]
    pub fn behavior_scratch_capacity(&self) -> usize {
        self.world.batch.capacity()
    }
}

/// A remote entity to mirror: who owns it, how its identity is built and its first basis
/// (ADR 0032, REQ-K-5).
#[derive(Clone)]
pub struct RemoteInit {
    /// Where the entity comes from (the adapter's name, ASM-227).
    pub source: RemoteSource,
    /// Motion model for identity only; never stepped for a Remote entity (ASM-241).
    pub motion: Arc<dyn MotionModel>,
    /// Its force, if the peer sent one (ASM-242).
    pub force: Option<Force>,
    /// The first dead-reckoning basis, taken at the kernel's current time.
    pub remote: RemoteState,
}

impl Kernel {
    /// Adds a Remote entity (no behavior, no sensors) under the next id (ADR 0032, ASM-242). Its
    /// state is the basis; the basis is taken at the current time (ASM-267).
    ///
    /// # Errors
    /// [`KernelError::InvalidState`] for a non-finite basis state and
    /// [`KernelError::IdsExhausted`] when ids run out. The kernel is unchanged on error.
    pub fn spawn_remote(&mut self, init: RemoteInit) -> Result<EntityId, KernelError> {
        self.world.spawn_remote(init, self.clock.now(), self.rng)
    }

    /// The dead-reckoning cap of `source`, if one is set.
    #[must_use]
    pub fn remote_timeout(&self, source: &RemoteSource) -> Option<Seconds> {
        self.world.remote_timeout(source)
    }

    /// Replaces the dead-reckoning caps: a Remote entity of a listed source is never extrapolated
    /// further than its source's timeout, the adapter's ghost timeout (ADR 0032, ASM-457). The
    /// previous list is dropped as a whole; a source that is not listed is not capped. Remote
    /// entities exist only with a network link, so scenarios without one are unaffected.
    ///
    /// # Errors
    /// [`KernelError::InvalidRemoteTimeout`] when a timeout is not positive and finite; the
    /// kernel is unchanged.
    pub fn set_remote_timeouts<'a>(
        &mut self,
        timeouts: impl IntoIterator<Item = (&'a RemoteSource, Seconds)>,
    ) -> Result<(), KernelError> {
        let mut map = std::collections::BTreeMap::new();
        for (source, timeout) in timeouts {
            if !(timeout.0.is_finite() && timeout.0 > 0.0) {
                return Err(KernelError::InvalidRemoteTimeout);
            }
            map.insert(source.clone(), timeout);
        }
        self.world.set_remote_timeouts(map);
        Ok(())
    }

    /// Replaces a Remote entity's dead-reckoning basis with `remote`, taken at the current time.
    ///
    /// # Errors
    /// [`KernelError::NotRemote`] for an Owned entity, [`KernelError::UnknownEntity`] for an
    /// unknown id, [`KernelError::InvalidState`] for a non-finite basis state. The kernel is
    /// unchanged on error.
    pub fn update_remote(&mut self, id: EntityId, remote: RemoteState) -> Result<(), KernelError> {
        self.world.update_remote(id, remote, self.clock.now())
    }

    /// Clears `out` and fills it with the Owned entities in id order (ASM-253).
    pub fn owned_views(&self, out: &mut Vec<OwnedView>) {
        out.clear();
        self.world.for_each_owned(|id, state, force| {
            out.push(OwnedView {
                id,
                state: *state,
                force: force.cloned(),
            });
        });
    }
}

/// Runs the systems of one phase. Perception, Behavior and Motion act for `Owned` entities only
/// (REQ-K-5), and so does Events, which turns `Fire` and `Custom` intents into `events` (ADR 0028);
/// Ingest dead-reckons Remote entities (ADR 0032); Publish has no systems yet.
fn run_phase(
    phase: Phase,
    world: &mut World,
    ctx: &TickContext<'_>,
    events: &mut Vec<SimEvent>,
    lifecycle: &mut Vec<(EntityId, Intent)>,
) {
    match phase {
        Phase::Perception => perception::run(world, ctx),
        Phase::Behavior => behavior::run_phase(world, ctx),
        Phase::Motion => world.for_each_mut(|e| {
            if e.ownership.is_owned() {
                motion::step(e.motion, e.state, &e.intents.0, ctx);
            }
        }),
        Phase::Events => {
            events::run(world, events);
            events::collect_lifecycle(world, lifecycle);
        }
        Phase::Ingest => ingest::run(world, ctx),
        Phase::Publish => {}
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
