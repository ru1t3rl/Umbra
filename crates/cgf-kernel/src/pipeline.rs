//! The fixed phase pipeline types (REQ-K-2, REQ-K-8).

use core::time::Duration;

use cgf_core::{Seconds, SimTime, TerrainProvider};

/// The six tick phases, in execution order (REQ-K-2). [`Phase::ALL`] is the single source of
/// that order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Phase {
    /// Network and command input (empty in M1).
    Ingest,
    /// Sensing (empty in M1).
    Perception,
    /// Behaviors write intents (the route stand-in in M1, ASM-029).
    Behavior,
    /// Motion models turn intents into new kinematic state.
    Motion,
    /// Event dispatch (empty in M1).
    Events,
    /// Output to observers (empty in M1).
    Publish,
}

impl Phase {
    /// Every phase in execution order.
    pub const ALL: [Phase; 6] = [
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
pub struct TickContext<'a> {
    /// The time of the tick being run (before the clock advances).
    pub now: SimTime,
    /// The tick period.
    pub dt: Seconds,
    /// Terrain for motion models.
    pub terrain: &'a dyn TerrainProvider,
}

/// What one tick reports.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct TickReport {
    /// Simulation time after the tick (ASM-036).
    pub time: SimTime,
    /// Wall time spent per phase, indexed like [`Phase::ALL`]. Diagnostics only: never hashed.
    pub phase_wall: [Duration; 6],
    /// In real-time and scaled modes: how far behind schedule the clock is when it exceeds one
    /// tick period (REQ-K-8). The tick still ran; none are skipped.
    pub overrun: Option<Duration>,
}
