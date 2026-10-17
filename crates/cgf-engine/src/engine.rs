//! The embeddable synchronous facade (REQ-S-2 minimal, ASM-026, ASM-044).
//!
//! `Engine` wraps a [`Kernel`], converts user-facing placement (ECEF or lat/lon/alt) to the
//! canonical ECEF state exactly once, and drives the kernel according to its clock.

use std::ops::ControlFlow;
use std::sync::Arc;

use cgf_core::Enu;
use cgf_core::{
    EntityId, Euler, KinematicState, LocalFrame, Meters, MetersPerSecond, MotionModel, Ownership,
    Position, Radians, SimRng, SimTime, StateHash, TerrainProvider, TickRate, Vec3, math,
};
use cgf_kernel::{
    ClockMode, EntityInit, Kernel, KernelConfig, Pace, RouteInit, Snapshot, StdWallClock,
    TickReport, WallClock,
};

use crate::error::{EngineError, SpawnParam};

/// Engine construction parameters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EngineConfig {
    /// Master seed of the simulation's random streams.
    pub seed: u64,
    /// Ticks per second.
    pub tick_rate: TickRate,
    /// Initial clock mode.
    pub clock: ClockMode,
}

/// When [`Engine::run`] returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunUntil {
    /// Total ticks since start: runs until the clock has reached this tick.
    Ticks(u64),
    /// Until the clock reports idle (paused, or stepped with no pending steps). In a free-running
    /// mode this does not return until the tick counter overflows.
    Idle,
}

/// Engine-level placement of one entity (ASM-044). Heading is clockwise from true north in the
/// local ENU frame at the entity's position.
#[derive(Clone)]
pub struct SpawnSpec {
    /// Where the entity starts, as ECEF or lat/lon/alt (height above the ellipsoid).
    pub position: Position,
    /// Initial heading.
    pub heading: Radians,
    /// Initial speed along the heading, horizontal.
    pub speed: MetersPerSecond,
    /// Whether this engine simulates the entity.
    pub ownership: Ownership,
    /// The entity's motion model instance.
    pub motion: Arc<dyn MotionModel>,
    /// Optional waypoint route.
    pub route: Option<RouteSpec>,
}

/// A waypoint route with positions in either form (ASM-044).
#[derive(Debug, Clone, PartialEq)]
pub struct RouteSpec {
    /// Waypoints in visiting order; at least one.
    pub waypoints: Vec<Position>,
    /// A waypoint counts as reached within this horizontal distance.
    pub arrival_radius: Meters,
    /// Speed requested while following the route.
    pub cruise_speed: MetersPerSecond,
}

/// What a per-tick callback sees: the tick's report, and on demand the hash and snapshot of the
/// state after the tick.
pub struct TickView<'a> {
    /// The report of the tick that just ran.
    pub report: &'a TickReport,
    kernel: &'a Kernel,
}

impl TickView<'_> {
    /// The state hash after this tick.
    pub fn state_hash(&self) -> StateHash {
        self.kernel.state_hash()
    }

    /// A snapshot after this tick.
    pub fn snapshot(&self) -> Snapshot {
        self.kernel.snapshot()
    }
}

/// Summary of one [`Engine::run`] or [`Engine::step`] call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct RunSummary {
    /// Ticks run during the call.
    pub ticks_run: u64,
    /// Whether the callback stopped the run with [`ControlFlow::Break`].
    pub stopped_early: bool,
}

/// The embeddable, synchronous engine.
pub struct Engine {
    kernel: Kernel,
}

impl Engine {
    /// An engine paced by the real wall clock, on the given terrain (REQ-C-6).
    ///
    /// Terrain supplied by an embedder is not validated; a provider reporting non-finite heights
    /// leaves entity heights unchanged (the ground model keeps the moved height). Terrain built
    /// from a scenario is validated by its schema (ASM-069).
    pub fn new(config: EngineConfig, terrain: Arc<dyn TerrainProvider>) -> Self {
        Self::with_wall_clock(config, terrain, Box::new(StdWallClock::new()))
    }

    /// An engine paced by the given wall clock (tests and embedding use a manual one).
    pub fn with_wall_clock(
        config: EngineConfig,
        terrain: Arc<dyn TerrainProvider>,
        wall: Box<dyn WallClock>,
    ) -> Self {
        let kernel_config = KernelConfig {
            tick_rate: config.tick_rate,
            seed: config.seed,
            clock: config.clock,
        };
        Self {
            kernel: Kernel::new(kernel_config, Box::new(terrain), wall),
        }
    }

    /// The master random source, for scenario-level randomness (ADR 0006).
    ///
    /// Streams are pure functions of (seed, key): deriving the same key again restarts that
    /// stream from its beginning. The scenario layer therefore owns the `SCENARIO` stream indices
    /// and draws each index once.
    pub fn rng(&self) -> SimRng {
        self.kernel.rng()
    }

    /// Places an entity. Positions are converted to ECEF here, once (ASM-044); the orientation
    /// and velocity come from the heading and speed in the local frame at the position.
    ///
    /// # Errors
    /// [`EngineError::InvalidSpawn`] for a non-finite heading or position or a non-finite or
    /// negative speed; [`EngineError::Kernel`] for an invalid route or exhausted ids.
    pub fn spawn(&mut self, spec: SpawnSpec) -> Result<EntityId, EngineError> {
        let position = spec.position.to_ecef();
        validate_spawn(&spec, position)?;
        // The frame is built from the geodetic derived from the ECEF position, never from the
        // geodetic the caller gave, so an ECEF spawn and a geodetic spawn of the same point are
        // bit-identical (AT-15).
        let frame = LocalFrame::at(position.to_geodetic());
        let (sin_h, cos_h) = math::sin_cos(spec.heading.0);
        let velocity: Vec3 = frame.vector_from_enu(Enu {
            east: spec.speed.0 * sin_h,
            north: spec.speed.0 * cos_h,
            up: 0.0,
        });
        let orientation = frame.orientation_from_euler(Euler {
            heading: spec.heading,
            pitch: Radians(0.0),
            roll: Radians(0.0),
        });
        let route = spec.route.map(|r| RouteInit {
            waypoints: r.waypoints.into_iter().map(Position::to_ecef).collect(),
            arrival_radius: r.arrival_radius,
            cruise_speed: r.cruise_speed,
        });
        let init = EntityInit {
            state: KinematicState {
                position,
                velocity,
                orientation,
            },
            ownership: spec.ownership,
            motion: spec.motion,
            route,
        };
        Ok(self.kernel.spawn(init)?)
    }

    /// Drives the kernel by its clock: polls, ticks or waits, and calls `on_tick` after every
    /// tick, until `until` is reached, the clock is idle, or the callback returns
    /// [`ControlFlow::Break`] (then [`RunSummary::stopped_early`] is set).
    ///
    /// # Errors
    /// [`EngineError::Kernel`] if a tick fails (time overflow).
    pub fn run(
        &mut self,
        until: RunUntil,
        mut on_tick: impl FnMut(&TickView<'_>) -> ControlFlow<()>,
    ) -> Result<RunSummary, EngineError> {
        let mut ticks_run = 0_u64;
        let mut stopped_early = false;
        loop {
            if let RunUntil::Ticks(limit) = until
                && self.kernel.now().tick() >= limit
            {
                break;
            }
            match self.kernel.clock_mut().poll() {
                Pace::Tick => {
                    let report = self.kernel.tick()?;
                    ticks_run += 1;
                    let view = TickView {
                        report: &report,
                        kernel: &self.kernel,
                    };
                    if on_tick(&view).is_break() {
                        stopped_early = true;
                        break;
                    }
                }
                Pace::WaitUntil(deadline) => self.kernel.clock().wait_until(deadline),
                Pace::Idle => break,
            }
        }
        Ok(RunSummary {
            ticks_run,
            stopped_early,
        })
    }

    /// Runs `n` more ticks. In stepped mode this requests `n` steps first; in other modes it
    /// runs until the tick count has grown by `n`. While paused nothing runs. When the callback
    /// breaks, the steps not yet run are cancelled.
    ///
    /// # Errors
    /// [`EngineError::Kernel`] if a tick fails (time overflow).
    pub fn step(
        &mut self,
        n: u64,
        on_tick: impl FnMut(&TickView<'_>) -> ControlFlow<()>,
    ) -> Result<RunSummary, EngineError> {
        if self.kernel.clock().mode() == ClockMode::Stepped {
            self.kernel.clock_mut().request_steps(n);
        }
        let target = self.kernel.now().tick().saturating_add(n);
        let summary = self.run(RunUntil::Ticks(target), on_tick)?;
        if summary.stopped_early {
            // Steps requested above but not run must not fire in a later `run`.
            self.kernel.clock_mut().cancel_steps();
        }
        Ok(summary)
    }

    /// Pauses the clock in every mode.
    pub fn pause(&mut self) {
        self.kernel.clock_mut().pause();
    }

    /// Resumes after [`pause`](Self::pause) without a burst of owed ticks.
    pub fn resume(&mut self) {
        self.kernel.clock_mut().resume();
    }

    /// Switches the clock mode.
    pub fn set_clock(&mut self, m: ClockMode) {
        self.kernel.clock_mut().set_mode(m);
    }

    /// The current simulation time.
    pub fn now(&self) -> SimTime {
        self.kernel.now()
    }

    /// The canonical state hash.
    pub fn state_hash(&self) -> StateHash {
        self.kernel.state_hash()
    }

    /// An immutable snapshot of all entities, sorted by id.
    pub fn snapshot(&self) -> Snapshot {
        self.kernel.snapshot()
    }
}

/// Checks the user-supplied numbers of a spawn (kernel checks cover the route and the state).
fn validate_spawn(spec: &SpawnSpec, position: cgf_core::Ecef) -> Result<(), EngineError> {
    let p = position.0;
    if !(p.x.is_finite() && p.y.is_finite() && p.z.is_finite()) {
        return Err(EngineError::InvalidSpawn(SpawnParam::Position));
    }
    if !spec.heading.0.is_finite() {
        return Err(EngineError::InvalidSpawn(SpawnParam::Heading));
    }
    if !(spec.speed.0.is_finite() && spec.speed.0 >= 0.0) {
        return Err(EngineError::InvalidSpawn(SpawnParam::Speed));
    }
    Ok(())
}
