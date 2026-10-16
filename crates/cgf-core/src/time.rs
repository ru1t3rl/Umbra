//! Simulation time (REQ-C-1, ASM-016). STUB (M0 step 4): signatures only, bodies are `todo!()` until WI-6.
#![expect(clippy::todo, missing_docs, reason = "stub until WI-6")]

use crate::units::Seconds;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TickRate(core::num::NonZeroU32);
impl TickRate {
    pub const fn new(hz: u32) -> Option<Self> {
        let _ = hz;
        todo!()
    }
    pub const fn hz(self) -> u32 {
        todo!()
    }
    pub fn dt(self) -> Seconds {
        todo!()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SimTime {
    tick: u64,
    rate: TickRate,
}
impl SimTime {
    pub const fn start(rate: TickRate) -> Self {
        let _ = rate;
        todo!()
    }
    pub const fn at_tick(rate: TickRate, tick: u64) -> Self {
        let _ = (rate, tick);
        todo!()
    }
    pub const fn tick(self) -> u64 {
        todo!()
    }
    pub const fn rate(self) -> TickRate {
        todo!()
    }
    pub fn dt(self) -> Seconds {
        todo!()
    }
    pub fn as_seconds(self) -> Seconds {
        todo!()
    }
    pub fn checked_next(self) -> Option<Self> {
        todo!()
    }
    pub fn checked_advance(self, ticks: u64) -> Option<Self> {
        let _ = ticks;
        todo!()
    }
    pub fn ticks_since(self, earlier: SimTime) -> Result<u64, TimeError> {
        let _ = earlier;
        todo!()
    }
}
impl PartialOrd for SimTime {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        let _ = other;
        todo!()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TimeError {
    #[error("tick rates differ: {left} Hz vs {right} Hz")]
    RateMismatch { left: u32, right: u32 },
    #[error("time {later} is before {earlier}")]
    NegativeInterval { earlier: u64, later: u64 },
}
