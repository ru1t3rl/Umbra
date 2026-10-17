//! STUB (WI-9): wall-clock sources (ASM-035). The only place real time may be read.
#![expect(
    missing_docs,
    clippy::todo,
    reason = "stub until WI-9: signatures only, documented when implemented"
)]

use core::time::Duration;
use std::sync::{Arc, Mutex};

/// Monotonic wall time since an arbitrary epoch.
pub trait WallClock: Send {
    fn now(&self) -> Duration;
    fn sleep_until(&self, deadline: Duration);
}

#[derive(Debug)]
pub struct StdWallClock {
    _private: (),
}

impl StdWallClock {
    pub fn new() -> Self {
        todo!("WI-9")
    }
}

impl Default for StdWallClock {
    fn default() -> Self {
        Self::new()
    }
}

impl WallClock for StdWallClock {
    fn now(&self) -> Duration {
        todo!("WI-9")
    }
    fn sleep_until(&self, _deadline: Duration) {
        todo!("WI-9")
    }
}

/// A hand-driven wall clock. Clones share the same time (the plan's `Arc<Mutex<Duration>>`).
#[derive(Debug, Clone)]
pub struct ManualWallClock {
    time: Arc<Mutex<Duration>>,
    auto_advance: Duration,
}

impl ManualWallClock {
    pub fn new() -> Self {
        todo!("WI-9")
    }
    pub fn advance(&self, _d: Duration) {
        let _ = &self.time;
        todo!("WI-9")
    }
    pub fn with_auto_advance(_step: Duration) -> Self {
        let _ = Duration::ZERO;
        todo!("WI-9")
    }
}

impl Default for ManualWallClock {
    fn default() -> Self {
        Self::new()
    }
}

impl WallClock for ManualWallClock {
    fn now(&self) -> Duration {
        let _ = self.auto_advance;
        todo!("WI-9")
    }
    fn sleep_until(&self, _deadline: Duration) {
        todo!("WI-9")
    }
}
