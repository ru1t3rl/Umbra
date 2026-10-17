//! STUB (WI-11): the fixed phase pipeline types (REQ-K-2, REQ-K-8).
#![expect(missing_docs, reason = "stub until WI-11: documented when implemented")]

use core::time::Duration;

use cgf_core::{Seconds, SimTime, TerrainProvider};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Phase {
    Ingest,
    Perception,
    Behavior,
    Motion,
    Events,
    Publish,
}

impl Phase {
    pub const ALL: [Phase; 6] = [
        Phase::Ingest,
        Phase::Perception,
        Phase::Behavior,
        Phase::Motion,
        Phase::Events,
        Phase::Publish,
    ];
}

pub struct TickContext<'a> {
    pub now: SimTime,
    pub dt: Seconds,
    pub terrain: &'a dyn TerrainProvider,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TickReport {
    pub time: SimTime,
    pub phase_wall: [Duration; 6],
    pub overrun: Option<Duration>,
}
