//! The engine handle (REQ-X-1, ADR 0024): one thread owns the `Engine`; every other thread talks to
//! it through a bounded command channel and reads frames from the [snapshot bus](crate::bus).
//!
//! WI-6 declares the types and the signatures. The data types, [`Reply`], [`HandleConfig`],
//! [`Command`] and the error enums are final; the bodies of [`EngineHandle`] and [`EngineThread`]
//! are `todo!` until WI-8 (thread, state machine, load, priming, shutdown) and WI-9 (steps and
//! queries). Bodies name the work item that implements them.

#![expect(
    clippy::todo,
    clippy::needless_pass_by_value,
    unused_variables,
    reason = "WI-6 signature stubs; the bodies arrive in WI-8 and WI-9"
)]

use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use umbra_core::{
    Catalog, ComponentId, ComponentKind, EntityId, ParamSchema, Provenance, Registry, SimTime,
    StateHash, TickRate, Version,
};
use umbra_kernel::{ClockMode, EntitySnapshot, StdWallClock, WallClock};
use umbra_templates::{ResolvedTemplate, TickRateSource};

use crate::bus::{Bus, RunState, RunStatus, SubscribeOptions, Subscription};
use crate::error::EngineError;
use crate::plugins::{PluginFault, PluginInfo};
use crate::scenario::{EntityInfo, LoadSettings};
use crate::session::{SessionError, TemplateSpawn};
use crate::validate::GraphValidation;

/// The smallest stack the engine thread accepts: deep graphs recurse (ADR 0024).
pub const MIN_STACK_SIZE: usize = 4 * 1024 * 1024;

/// Why a command did not complete.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum HandleError {
    /// The command queue is full.
    #[error("the engine command queue is full")]
    Busy,
    /// The engine thread has ended, or the reply was dropped without an answer.
    #[error("the engine is not running")]
    Closed,
    /// No scenario is loaded.
    #[error("no scenario is loaded")]
    NoSession,
    /// The command is not valid in this run state.
    #[error("{command} is not valid while the run is {state:?}")]
    InvalidState {
        /// The state the run was in.
        state: RunState,
        /// The command.
        command: &'static str,
    },
    /// `step` needs the stepped clock.
    #[error("the clock is not stepped")]
    NotStepped,
    /// A step is already pending.
    #[error("a step is already in progress")]
    StepInProgress,
    /// The step was cancelled by a pause, unload, clock change or plugin fault.
    #[error("the step was cancelled at {reached:?}")]
    StepCancelled {
        /// The time reached when it was cancelled.
        reached: SimTime,
    },
    /// Loading, spawning or removing failed.
    #[error(transparent)]
    Session(#[from] SessionError),
    /// The engine failed.
    #[error(transparent)]
    Engine(#[from] EngineError),
}

/// Why [`EngineHandle::spawn`] failed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SpawnError {
    /// The stack is smaller than [`MIN_STACK_SIZE`].
    #[error("engine thread stack of {requested} bytes is below the minimum of {minimum}")]
    StackTooSmall {
        /// The configured size.
        requested: usize,
        /// The minimum.
        minimum: usize,
    },
    /// The command queue capacity is zero.
    #[error("the command queue capacity must be at least 1")]
    ZeroCapacity,
    /// The thread could not be started.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// How to start the engine thread.
#[non_exhaustive]
pub struct HandleConfig {
    /// The registry scenarios load against (built-ins; plugins are added per session).
    pub base_registry: Registry,
    /// Tick rate default, extra plugin directories and confinement.
    pub load: LoadSettings,
    /// Commands queued before `Busy`; default 64; 0 is [`SpawnError::ZeroCapacity`].
    pub command_capacity: usize,
    /// Stack of the engine thread; default 8 MiB; below [`MIN_STACK_SIZE`] is refused.
    pub stack_size: usize,
    /// The longest one wait slice of the thread; default 10 ms.
    pub max_wait: Duration,
    /// Makes the wall clock of each new session.
    pub wall: Arc<dyn Fn() -> Box<dyn WallClock> + Send + Sync>,
}

impl HandleConfig {
    /// The defaults of ADR 0024 over `base_registry`, with a standard wall clock per session.
    #[must_use]
    pub fn new(base_registry: Registry) -> Self {
        Self {
            base_registry,
            load: LoadSettings::default(),
            command_capacity: 64,
            stack_size: 8 * 1024 * 1024,
            max_wait: Duration::from_millis(10),
            wall: Arc::new(|| -> Box<dyn WallClock> { Box::new(StdWallClock::new()) }),
        }
    }
}

/// The callback side of a command: completes at most once. If it is dropped uncompleted, the
/// callback receives [`HandleError::Closed`] (so a dead engine thread never leaves a caller waiting).
pub struct Reply<T>(Option<Callback<T>>);

type Callback<T> = Box<dyn FnOnce(Result<T, HandleError>) + Send>;

impl<T> Reply<T> {
    /// A reply that calls `callback` with the outcome.
    pub fn new(callback: impl FnOnce(Result<T, HandleError>) + Send + 'static) -> Self {
        Self(Some(Box::new(callback)))
    }

    /// Completes the reply.
    pub fn complete(mut self, result: Result<T, HandleError>) {
        if let Some(callback) = self.0.take() {
            callback(result);
        }
    }
}

impl<T> Drop for Reply<T> {
    fn drop(&mut self) {
        if let Some(callback) = self.0.take() {
            callback(Err(HandleError::Closed));
        }
    }
}

impl<T> fmt::Debug for Reply<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Reply")
            .field(&if self.0.is_some() { "pending" } else { "done" })
            .finish()
    }
}

/// The loaded scenario as the engine reports it.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct SessionInfo {
    /// The session number.
    pub session: u64,
    /// The scenario name.
    pub name: String,
    /// The scenario file in display form (relative to the scenario root when confined).
    pub path: String,
    /// The seed in force.
    pub seed: u64,
    /// Ticks per second.
    pub tick_rate: TickRate,
    /// Where the tick rate came from.
    pub tick_rate_source: TickRateSource,
    /// The run length in ticks.
    pub duration_ticks: u64,
    /// Entities at load time.
    pub entity_count: usize,
}

/// The run status plus the scenario, if any.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct EngineStatus {
    /// The run status.
    pub run: RunStatus,
    /// The loaded scenario.
    pub scenario: Option<SessionInfo>,
}

/// The outcome of a `step` (ADR 0026, review M10).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct StepOutcome {
    /// The ticks asked for.
    pub requested: u64,
    /// Whether all of them ran; false when the duration or a `stop` ended the run first.
    pub completed: bool,
    /// The time when the step was applied.
    pub applied_at: SimTime,
    /// The time reached.
    pub reached: SimTime,
    /// The state hash at `reached`.
    pub state_hash: StateHash,
    /// The run status afterwards.
    pub status: RunStatus,
}

/// A command applied between ticks (spawn, remove).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct Applied {
    /// The time at which it was applied.
    pub applied_at: SimTime,
}

/// An entity as the engine reports it: its directory entry and its state in the last tick.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct EntityState {
    /// Name, template and group.
    pub info: EntityInfo,
    /// Kinematics, behavior, route, contacts and intents.
    pub snapshot: EntitySnapshot,
}

/// A bounding box in degrees; `min_lon_deg > max_lon_deg` crosses the antimeridian.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GeoBox {
    /// Southern edge.
    pub min_lat_deg: f64,
    /// Western edge.
    pub min_lon_deg: f64,
    /// Northern edge.
    pub max_lat_deg: f64,
    /// Eastern edge.
    pub max_lon_deg: f64,
}

/// Which side owns the entities to list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnershipFilter {
    /// Simulated by this engine.
    Owned,
    /// Mirrored from elsewhere.
    Remote,
}

/// Which entities to list, and from where (keyset paging by id, ADR 0026).
///
/// Filters that name something unknown (a template, a force) simply match nothing.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct EntityQuery {
    /// Entities with an id above this one.
    pub after_id: Option<EntityId>,
    /// The page size, at most [`EntityQuery::MAX_LIMIT`].
    pub limit: usize,
    /// Only these ids; empty means all.
    pub ids: Vec<EntityId>,
    /// Only the entity with this name.
    pub name: Option<String>,
    /// Only this force.
    pub force: Option<String>,
    /// Only this group.
    pub group: Option<String>,
    /// Only this template.
    pub template: Option<String>,
    /// Only this ownership.
    pub ownership: Option<OwnershipFilter>,
    /// Only inside this box.
    pub bbox: Option<GeoBox>,
}

impl EntityQuery {
    /// The page size when none is given.
    pub const DEFAULT_LIMIT: usize = 100;
    /// The largest page size.
    pub const MAX_LIMIT: usize = 1000;
}

impl Default for EntityQuery {
    fn default() -> Self {
        Self {
            after_id: None,
            limit: Self::DEFAULT_LIMIT,
            ids: Vec::new(),
            name: None,
            force: None,
            group: None,
            template: None,
            ownership: None,
            bbox: None,
        }
    }
}

/// One page of entities.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct EntityPage {
    /// The tick the page was read at.
    pub tick: u64,
    /// Entities matching the filter, over all pages.
    pub total: usize,
    /// The page, ascending by id.
    pub items: Vec<EntityState>,
    /// The cursor of the next page; `None` on the last one.
    pub next_after_id: Option<EntityId>,
}

/// A registered component with its schema.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct ComponentEntry {
    /// The kind.
    pub kind: ComponentKind,
    /// The component id.
    pub id: ComponentId,
    /// Its version.
    pub version: Version,
    /// A description.
    pub description: String,
    /// Where it came from.
    pub provenance: Provenance,
    /// Its parameter schema.
    pub params_schema: ParamSchema,
}

/// The plugins of the session and the fault that stopped it.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct PluginReport {
    /// The loaded plugins in load order; empty without a session.
    pub plugins: Vec<PluginInfo>,
    /// The fault that stopped the run.
    pub fault: Option<PluginFault>,
}

/// A request to the engine thread. Each carries a [`Reply`].
#[derive(Debug)]
#[non_exhaustive]
pub enum Command {
    /// Replaces the session with the scenario at `path`.
    Load {
        /// The scenario file.
        path: PathBuf,
        /// Replaces the scenario's seed.
        seed: Option<u64>,
        /// Replaces the scenario's clock.
        clock: Option<ClockMode>,
        /// The answer.
        reply: Reply<SessionInfo>,
    },
    /// Drops the session.
    Unload {
        /// The answer.
        reply: Reply<()>,
    },
    /// `ready` -> `running`.
    Start {
        /// The answer.
        reply: Reply<RunStatus>,
    },
    /// `running` -> `paused`.
    Pause {
        /// The answer.
        reply: Reply<RunStatus>,
    },
    /// `paused` -> `running`.
    Resume {
        /// The answer.
        reply: Reply<RunStatus>,
    },
    /// Any loaded state but `stopped` -> `stopped`.
    Stop {
        /// The answer.
        reply: Reply<RunStatus>,
    },
    /// Runs `count` ticks of a stepped clock, replying when done.
    Step {
        /// The ticks.
        count: u64,
        /// The answer.
        reply: Reply<StepOutcome>,
    },
    /// Changes the clock.
    SetClock {
        /// The new clock.
        mode: ClockMode,
        /// The answer.
        reply: Reply<RunStatus>,
    },
    /// Spawns an entity from a template.
    Spawn {
        /// What to spawn.
        spec: TemplateSpawn,
        /// The answer.
        reply: Reply<EntityState>,
    },
    /// Removes an entity.
    Remove {
        /// The entity.
        id: EntityId,
        /// The answer.
        reply: Reply<Applied>,
    },
    /// Reads the status.
    Status {
        /// The answer.
        reply: Reply<EngineStatus>,
    },
    /// Lists entities.
    Entities {
        /// The query.
        query: EntityQuery,
        /// The answer.
        reply: Reply<EntityPage>,
    },
    /// Reads one entity.
    Entity {
        /// The entity.
        id: EntityId,
        /// The answer.
        reply: Reply<EntityState>,
    },
    /// Lists the session's templates.
    Templates {
        /// The answer.
        reply: Reply<Vec<ResolvedTemplate>>,
    },
    /// Lists registered components, optionally of one kind.
    Components {
        /// The kind filter.
        kind: Option<ComponentKind>,
        /// The answer.
        reply: Reply<Vec<ComponentEntry>>,
    },
    /// Reads the behavior catalog (the session's registry, or the base one without a session).
    Catalog {
        /// The answer.
        reply: Reply<Catalog>,
    },
    /// Reads the plugins.
    Plugins {
        /// The answer.
        reply: Reply<PluginReport>,
    },
    /// Validates a graph document (REQ-X-11).
    ValidateGraph {
        /// The name diagnostics use for the document.
        name: String,
        /// The document text.
        document: String,
        /// The answer.
        reply: Reply<GraphValidation>,
    },
    /// Internal: serve the subscriptions marked "needs prime".
    #[doc(hidden)]
    #[non_exhaustive]
    Prime,
    /// Internal: wake the thread for shutdown.
    #[doc(hidden)]
    #[non_exhaustive]
    Shutdown,
}

/// A cloneable, thread-safe handle to the engine thread.
#[derive(Clone)]
pub struct EngineHandle {
    #[expect(dead_code, reason = "read by the WI-8 bodies")]
    bus: Arc<Bus>,
}

/// Owns the engine thread: [`shutdown`](Self::shutdown) joins it. (Joining on drop arrives with the
/// thread, in WI-8.)
pub struct EngineThread {
    _private: (),
}

impl EngineHandle {
    /// Starts the engine thread.
    ///
    /// # Errors
    /// [`SpawnError::ZeroCapacity`], [`SpawnError::StackTooSmall`], or [`SpawnError::Io`].
    pub fn spawn(config: HandleConfig) -> Result<(EngineHandle, EngineThread), SpawnError> {
        todo!("WI-8")
    }

    /// Queues a command without waiting.
    ///
    /// # Errors
    /// [`HandleError::Busy`] if the queue is full; [`HandleError::Closed`] if the thread has ended.
    pub fn submit(&self, command: Command) -> Result<(), HandleError> {
        todo!("WI-8")
    }

    /// Registers a subscription and has it primed with a `Control` frame of the current state.
    #[must_use]
    pub fn subscribe(&self, options: SubscribeOptions) -> Subscription {
        todo!("WI-8")
    }

    /// How many tick and control frames the thread has built (probe for the stride test, AT-10).
    #[doc(hidden)]
    #[must_use]
    pub fn snapshot_builds(&self) -> u64 {
        todo!("WI-8")
    }

    /// Blocking form of [`Command::Load`].
    ///
    /// # Errors
    /// As the command.
    pub fn load(
        &self,
        path: PathBuf,
        seed: Option<u64>,
        clock: Option<ClockMode>,
    ) -> Result<SessionInfo, HandleError> {
        todo!("WI-8")
    }

    /// Blocking form of [`Command::Unload`].
    ///
    /// # Errors
    /// As the command.
    pub fn unload(&self) -> Result<(), HandleError> {
        todo!("WI-8")
    }

    /// Blocking form of [`Command::Start`].
    ///
    /// # Errors
    /// As the command.
    pub fn start(&self) -> Result<RunStatus, HandleError> {
        todo!("WI-8")
    }

    /// Blocking form of [`Command::Pause`].
    ///
    /// # Errors
    /// As the command.
    pub fn pause(&self) -> Result<RunStatus, HandleError> {
        todo!("WI-8")
    }

    /// Blocking form of [`Command::Resume`].
    ///
    /// # Errors
    /// As the command.
    pub fn resume(&self) -> Result<RunStatus, HandleError> {
        todo!("WI-8")
    }

    /// Blocking form of [`Command::Stop`].
    ///
    /// # Errors
    /// As the command.
    pub fn stop(&self) -> Result<RunStatus, HandleError> {
        todo!("WI-8")
    }

    /// Blocking form of [`Command::Step`].
    ///
    /// # Errors
    /// As the command.
    pub fn step(&self, count: u64) -> Result<StepOutcome, HandleError> {
        todo!("WI-9")
    }

    /// Blocking form of [`Command::SetClock`].
    ///
    /// # Errors
    /// As the command.
    pub fn set_clock(&self, mode: ClockMode) -> Result<RunStatus, HandleError> {
        todo!("WI-8")
    }

    /// Blocking form of [`Command::Spawn`].
    ///
    /// # Errors
    /// As the command.
    pub fn spawn_entity(&self, spec: TemplateSpawn) -> Result<EntityState, HandleError> {
        todo!("WI-9")
    }

    /// Blocking form of [`Command::Remove`].
    ///
    /// # Errors
    /// As the command.
    pub fn remove_entity(&self, id: EntityId) -> Result<Applied, HandleError> {
        todo!("WI-9")
    }

    /// Blocking form of [`Command::Status`].
    ///
    /// # Errors
    /// As the command.
    pub fn status(&self) -> Result<EngineStatus, HandleError> {
        todo!("WI-8")
    }

    /// Blocking form of [`Command::Entities`].
    ///
    /// # Errors
    /// As the command.
    pub fn entities(&self, query: EntityQuery) -> Result<EntityPage, HandleError> {
        todo!("WI-9")
    }

    /// Blocking form of [`Command::Entity`].
    ///
    /// # Errors
    /// As the command.
    pub fn entity(&self, id: EntityId) -> Result<EntityState, HandleError> {
        todo!("WI-9")
    }

    /// Blocking form of [`Command::Templates`].
    ///
    /// # Errors
    /// As the command.
    pub fn templates(&self) -> Result<Vec<ResolvedTemplate>, HandleError> {
        todo!("WI-9")
    }

    /// Blocking form of [`Command::Components`].
    ///
    /// # Errors
    /// As the command.
    pub fn components(
        &self,
        kind: Option<ComponentKind>,
    ) -> Result<Vec<ComponentEntry>, HandleError> {
        todo!("WI-9")
    }

    /// Blocking form of [`Command::Catalog`].
    ///
    /// # Errors
    /// As the command.
    pub fn catalog(&self) -> Result<Catalog, HandleError> {
        todo!("WI-9")
    }

    /// Blocking form of [`Command::Plugins`].
    ///
    /// # Errors
    /// As the command.
    pub fn plugins(&self) -> Result<PluginReport, HandleError> {
        todo!("WI-9")
    }

    /// Blocking form of [`Command::ValidateGraph`].
    ///
    /// # Errors
    /// As the command.
    pub fn validate_graph(
        &self,
        name: String,
        document: String,
    ) -> Result<GraphValidation, HandleError> {
        todo!("WI-9")
    }
}

impl EngineThread {
    /// Sets the shutdown flag, wakes the thread without blocking on the queue, and joins it. A step
    /// in progress is answered [`HandleError::Closed`].
    ///
    /// # Errors
    /// [`HandleError::Closed`] if the thread had already ended by a panic.
    pub fn shutdown(self) -> Result<(), HandleError> {
        todo!("WI-8")
    }
}
