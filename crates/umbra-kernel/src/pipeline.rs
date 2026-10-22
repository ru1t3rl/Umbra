//! The fixed phase pipeline types (REQ-K-2, REQ-K-8).

use core::time::Duration;

use umbra_core::{EntityId, Intent, Seconds, SimEvent, SimTime, TerrainProvider};

/// The number of tick phases.
pub const PHASE_COUNT: usize = 6;

/// The six tick phases, in execution order (REQ-K-2). [`Phase::ALL`] is the single source of
/// that order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Phase {
    /// Dead reckoning of Remote entities (ADR 0032); network input is applied by the engine
    /// between ticks.
    Ingest,
    /// Sensors produce each Owned entity's contacts from pre-tick state (ADR 0015).
    Perception,
    /// Behaviors write intents; entities without one run the M1 route stand-in (ASM-029, ASM-082).
    Behavior,
    /// Motion models turn intents into new kinematic state.
    Motion,
    /// Fire and Custom intents of this tick become canonical events; no state changes (ADR 0028).
    Events,
    /// Output to observers (empty in M1).
    Publish,
}

impl Phase {
    /// Every phase in execution order.
    pub const ALL: [Phase; PHASE_COUNT] = [
        Phase::Ingest,
        Phase::Perception,
        Phase::Behavior,
        Phase::Motion,
        Phase::Events,
        Phase::Publish,
    ];

    /// A static name for tracing spans and logs.
    pub const fn name(self) -> &'static str {
        match self {
            Phase::Ingest => "ingest",
            Phase::Perception => "perception",
            Phase::Behavior => "behavior",
            Phase::Motion => "motion",
            Phase::Events => "events",
            Phase::Publish => "publish",
        }
    }
}

/// What systems receive for one tick: simulation time and step only, never wall time (REQ-C-1).
pub(crate) struct TickContext<'a> {
    /// Simulation time at the start of the tick.
    pub(crate) now: SimTime,
    /// The tick period.
    pub(crate) dt: Seconds,
    /// Terrain for motion models.
    pub(crate) terrain: &'a dyn TerrainProvider,
}

/// What one tick reports.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct TickReport {
    /// Simulation time after the tick (ASM-036).
    pub time: SimTime,
    /// Wall time spent per phase, indexed like [`Phase::ALL`]. Diagnostics only: never hashed.
    pub phase_wall: [Duration; PHASE_COUNT],
    /// In real-time and scaled modes: by how much this tick's own wall duration exceeded its
    /// budget (`Clock::wall_period`), if it did (REQ-K-8, ASM-050). `None` in other modes.
    pub overrun: Option<Duration>,
    /// In real-time and scaled modes: how far behind its schedule the clock is after this tick,
    /// when that exceeds one period (`Clock::behind`). Catch-up ticks after a stall report lag
    /// but, unless they are slow themselves, no overrun.
    pub lag: Option<Duration>,
    /// The canonical events of this tick: Owned entities in id order, each entity's `Fire` and
    /// `Custom` intents in emission order (REQ-C-5, ADR 0028). Never hashed.
    pub events: Vec<SimEvent>,
    /// The `Spawn` and `Despawn` intents of Owned entities, in entity id order and, within an
    /// entity, in emission order, for the engine to apply after the tick (ADR 0052, REQ-B-2). A
    /// bare kernel applies none of them. Never hashed.
    pub lifecycle: Vec<(EntityId, Intent)>,
}
