//! The embeddable synchronous facade (REQ-S-2 minimal, ASM-026, ASM-044).
//!
//! `Engine` wraps a [`Kernel`], converts user-facing placement (ECEF or lat/lon/alt) to the
//! canonical ECEF state exactly once, and drives the kernel according to its clock.

use std::ops::ControlFlow;
use std::sync::Arc;
use std::time::Duration;

use umbra_core::Enu;
use umbra_core::{
    EntityId, Euler, KinematicState, LocalFrame, Meters, MetersPerSecond, MotionModel, Ownership,
    Position, Radians, SimRng, SimTime, StateHash, StateHashVersion, TerrainProvider, TickRate,
    Vec3, math,
};
pub use umbra_kernel::AgentInit;
use umbra_kernel::{
    ClockMode, EntityInit, Kernel, KernelConfig, Pace, RouteInit, Snapshot, StdWallClock,
    TickReport, WallClock,
};

use umbra_plugin_host::{PluginFault, PluginHost};

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

    /// The state hash after this tick in the given encoding (ADR 0055; v3 is implemented in M7
    /// WI-15, until then it panics).
    pub fn state_hash_with(&self, version: StateHashVersion) -> StateHash {
        self.kernel.state_hash_with(version)
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
    pub(crate) kernel: Kernel,
    /// The network link, if the scenario has adapters (M6, ADR 0030). Declared before `plugins`, so
    /// it drops first and stops every adapter before the plugin host releases its images.
    pub(crate) network: Option<crate::net::NetLink>,
    /// The plugin host bound by `attach_plugins` (M4, ADR 0021); checked after every tick.
    pub(crate) plugins: Option<Arc<PluginHost>>,
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
            network: None,
            plugins: None,
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
        self.spawn_with(spec, AgentInit::default())
    }

    /// Places an entity with a behavior, sensors and a force (ADR 0011). `spawn(spec)` is
    /// `spawn_with(spec, AgentInit::default())`.
    ///
    /// # Errors
    /// The errors of [`Engine::spawn`], and [`EngineError::Kernel`] wrapping
    /// `KernelError::Instantiate` when the behavior cannot be instantiated at the tick rate.
    pub fn spawn_with(
        &mut self,
        spec: SpawnSpec,
        agent: AgentInit,
    ) -> Result<EntityId, EngineError> {
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
        Ok(self.kernel.spawn_with(init, agent)?)
    }

    /// Drives the kernel by its clock: polls, ticks or waits, and calls `on_tick` after every
    /// tick, until `until` is reached, the clock is idle, or the callback returns
    /// [`ControlFlow::Break`] (then [`RunSummary::stopped_early`] is set).
    ///
    /// # Stack
    /// Behavior graphs are ticked by recursion, one or two frames per node level (up to 128 levels
    /// per graph, 16 graphs deep). The calling thread needs at least 2 MiB in a debug build
    /// (worst case measured 1.25-1.5 MiB, ASM-098); drive the engine from a thread with an
    /// explicit stack of 4 MiB or more.
    ///
    /// # Errors
    /// [`EngineError::Kernel`] if a tick fails (time overflow); [`EngineError::PluginFault`] if an
    /// attached plugin host reports a fault under the `abort` policy after a tick: the run stops
    /// right after that tick, once `on_tick` has seen it (ASM-145 a, ADR 0021).
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
                    let Ticked {
                        report,
                        plugin_fault,
                        net_fault,
                    } = self.tick_once()?;
                    ticks_run += 1;
                    let view = TickView {
                        report: &report,
                        kernel: &self.kernel,
                    };
                    let flow = on_tick(&view);
                    if let Some(fault) = plugin_fault {
                        return Err(EngineError::PluginFault(fault));
                    }
                    if let Some(fault) = net_fault {
                        return Err(EngineError::NetworkFault(fault));
                    }
                    if flow.is_break() {
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
    /// # Stack
    /// Behavior graphs are ticked by recursion, one or two frames per node level (up to 128 levels
    /// per graph, 16 graphs deep). The calling thread needs at least 2 MiB in a debug build
    /// (worst case measured 1.25-1.5 MiB, ASM-098); drive the engine from a thread with an
    /// explicit stack of 4 MiB or more.
    ///
    /// # Errors
    /// The errors of [`Engine::run`].
    pub fn step(
        &mut self,
        n: u64,
        on_tick: impl FnMut(&TickView<'_>) -> ControlFlow<()>,
    ) -> Result<RunSummary, EngineError> {
        if self.kernel.clock().mode() == ClockMode::Stepped {
            self.kernel.clock_mut().request_steps(n);
        }
        let target = self.kernel.now().tick().saturating_add(n);
        let result = self.run(RunUntil::Ticks(target), on_tick);
        let stopped = !matches!(&result, Ok(s) if !s.stopped_early);
        if stopped {
            // Steps requested above but not run must not fire in a later `run`.
            self.kernel.clock_mut().cancel_steps();
        }
        result
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

    /// The state hash in the given encoding (ADR 0055; v3 is implemented in M7 WI-15, until then it
    /// panics).
    pub fn state_hash_with(&self, version: StateHashVersion) -> StateHash {
        self.kernel.state_hash_with(version)
    }

    /// An immutable snapshot of all entities, sorted by id.
    pub fn snapshot(&self) -> Snapshot {
        self.kernel.snapshot()
    }
}

/// What one tick produced, with the faults that end the run.
struct Ticked {
    report: TickReport,
    plugin_fault: Option<PluginFault>,
    net_fault: Option<crate::net::NetFault>,
}

/// Checks the user-supplied numbers of a spawn (kernel checks cover the route and the state).
fn validate_spawn(spec: &SpawnSpec, position: umbra_core::Ecef) -> Result<(), EngineError> {
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

/// What one [`Engine::pump`] call did (ADR 0024).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Pumped {
    /// One tick ran.
    Ticked(TickReport),
    /// One tick ran and a plugin faulted under the `abort` policy in it (ADR 0021, ASM-145 a). The
    /// tick's report, with its events, is kept; the driver stops the run after publishing it.
    Aborted {
        /// The report of the faulting tick.
        report: TickReport,
        /// The fault.
        fault: PluginFault,
    },
    /// One tick ran and a network adapter faulted under the `abort` policy in it (M6, ASM-252).
    /// Like [`Pumped::Aborted`]: the report is kept and the driver stops the run.
    NetAborted {
        /// The report of the faulting tick.
        report: TickReport,
        /// The fault.
        fault: crate::net::NetFault,
    },
    /// No tick was due; the call waited, at most `max_wait` and never past the next deadline.
    Waited,
    /// Nothing to do: the clock is paused, or stepped with no pending steps.
    Idle,
}

impl Engine {
    /// One pacing decision, for a driver that must stay responsive (the engine thread, ADR 0024):
    /// runs a tick if one is due, otherwise waits for the clock at most `max_wait`, or says there is
    /// nothing to do. Never runs more than one tick. After a tick it checks the plugin host like
    /// [`Engine::run`] does, but reports the fault as [`Pumped::Aborted`] with the tick's report
    /// instead of an error.
    ///
    /// # Stack
    /// As [`Engine::run`].
    ///
    /// # Errors
    /// [`EngineError::Kernel`] when the tick counter is exhausted; no tick ran then.
    pub fn pump(&mut self, max_wait: Duration) -> Result<Pumped, EngineError> {
        match self.kernel.clock_mut().poll() {
            Pace::Tick => {
                let Ticked {
                    report,
                    plugin_fault,
                    net_fault,
                } = self.tick_once()?;
                Ok(match (plugin_fault, net_fault) {
                    (Some(fault), _) => Pumped::Aborted { report, fault },
                    (None, Some(fault)) => Pumped::NetAborted { report, fault },
                    (None, None) => Pumped::Ticked(report),
                })
            }
            Pace::WaitUntil(deadline) => {
                let clock = self.kernel.clock();
                clock.wait_until(deadline.min(clock.wall_now().saturating_add(max_wait)));
                Ok(Pumped::Waited)
            }
            Pace::Idle => Ok(Pumped::Idle),
        }
    }

    /// Requests `n` more steps (consumed in stepped mode).
    pub fn request_steps(&mut self, n: u64) {
        self.kernel.clock_mut().request_steps(n);
    }

    /// Drops the steps requested and not yet run.
    pub fn cancel_steps(&mut self) {
        self.kernel.clock_mut().cancel_steps();
    }

    /// The steps requested and not yet run.
    pub fn pending_steps(&self) -> u64 {
        self.kernel.clock().pending_steps()
    }

    /// Whether the clock is paused.
    pub fn is_paused(&self) -> bool {
        self.kernel.clock().is_paused()
    }

    /// The current clock mode.
    pub fn clock_mode(&self) -> ClockMode {
        self.kernel.clock().mode()
    }

    /// Removes an entity between ticks (REQ-C-2, ADR 0028): its behavior instance is dropped, so a
    /// plugin instance is destroyed here. Ids are not reused.
    ///
    /// # Errors
    /// [`EngineError::Kernel`] wrapping `KernelError::UnknownEntity`; nothing changes then.
    pub fn remove(&mut self, id: EntityId) -> Result<(), EngineError> {
        self.kernel.remove(id)?;
        if let Some(link) = self.network.as_mut() {
            link.entity_removed(id);
        }
        Ok(())
    }

    /// One tick with the network around it (ASM-224): ingest, tick, publish, then the abort checks.
    /// The ghost events are prepended to the report's (ASM-248). Without a link it is a plain tick.
    fn tick_once(&mut self) -> Result<Ticked, EngineError> {
        let ingested = match self.network.as_mut() {
            Some(link) => link.ingest(&mut self.kernel),
            None => Vec::new(),
        };
        // If the tick fails (the tick counter is exhausted), the ghost events of this ingest are
        // lost with it: the ghosts exist in the kernel but no report announces them. That is the
        // end of the run in any case, so nothing is kept for a retry.
        let mut report = self.kernel.tick()?;
        let mut net_fault = None;
        if let Some(link) = self.network.as_mut() {
            report.events.splice(0..0, ingested);
            link.publish(&self.kernel, &report.events);
            net_fault = link.take_abort();
        }
        let plugin_fault = self.plugins.as_ref().and_then(|h| h.take_abort());
        Ok(Ticked {
            report,
            plugin_fault,
            net_fault,
        })
    }
}
