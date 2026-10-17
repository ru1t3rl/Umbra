//! STUB (WI-9): the simulation clock and its modes (REQ-K-3, REQ-K-8, REQ-C-1, D-5, ASM-035).
//!
//! Signatures only. Acceptance tests: `tests/clock.rs`, `tests/clock_prop.rs` (AT-7).
#![expect(
    missing_docs,
    clippy::todo,
    reason = "stub until WI-9: signatures only, documented when implemented"
)]

mod wall;

use core::time::Duration;

use cgf_core::{SimTime, TickRate};

use crate::error::KernelError;

pub use wall::{ManualWallClock, StdWallClock, WallClock};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimeScale(f64);

impl TimeScale {
    pub fn new(_s: f64) -> Result<Self, ClockError> {
        todo!("WI-9")
    }
    pub fn get(self) -> f64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ClockError {
    #[error("time scale must be finite and > 0")]
    InvalidScale,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ClockMode {
    RealTime,
    Scaled(TimeScale),
    AsFastAsPossible,
    Stepped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pace {
    Tick,
    WaitUntil(Duration),
    Idle,
}

pub struct Clock {
    mode: ClockMode,
    now: SimTime,
    wall: Box<dyn WallClock>,
}

impl Clock {
    pub fn new(_rate: TickRate, _mode: ClockMode, _wall: Box<dyn WallClock>) -> Self {
        todo!("WI-9")
    }
    /// Test-driven addition (not in the plan): start at an arbitrary time, so the overflow path
    /// of `advance` is reachable without 2^64 ticks.
    pub fn starting_at(_time: SimTime, _mode: ClockMode, _wall: Box<dyn WallClock>) -> Self {
        todo!("WI-9")
    }
    pub fn now(&self) -> SimTime {
        self.now
    }
    pub fn mode(&self) -> ClockMode {
        self.mode
    }
    pub fn set_mode(&mut self, _m: ClockMode) {
        let _ = &self.wall;
        todo!("WI-9")
    }
    pub fn pause(&mut self) {
        todo!("WI-9")
    }
    pub fn resume(&mut self) {
        todo!("WI-9")
    }
    pub fn is_paused(&self) -> bool {
        todo!("WI-9")
    }
    pub fn request_steps(&mut self, _n: u64) {
        todo!("WI-9")
    }
    pub fn poll(&mut self) -> Pace {
        todo!("WI-9")
    }
    pub fn advance(&mut self) -> Result<SimTime, KernelError> {
        todo!("WI-9")
    }
    pub fn behind(&self) -> Option<Duration> {
        todo!("WI-9")
    }
    pub fn wall_now(&self) -> Duration {
        todo!("WI-9")
    }
    pub fn wait_until(&self, _d: Duration) {
        todo!("WI-9")
    }
}
