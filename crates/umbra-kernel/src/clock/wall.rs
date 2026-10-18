//! Wall-clock sources (ASM-035, REQ-K-4).
//!
//! This file is the only place in the workspace (outside tests and `xtask`) that reads real time
//! or sleeps. The simulation never sees it: only the [`Clock`](super::Clock) pacing logic and the
//! pipeline's diagnostics timing go through [`WallClock`].

use core::time::Duration;
use std::sync::{Arc, Mutex, PoisonError};

pub use real::StdWallClock;

/// Monotonic wall time since an arbitrary epoch.
///
/// Implementations must never go backwards. Systems never receive a `WallClock`; only the clock
/// and the pipeline timing do.
pub trait WallClock: Send {
    /// Wall time elapsed since this clock's epoch.
    fn now(&self) -> Duration;

    /// Blocks (or jumps, for manual clocks) until [`now`](Self::now) reaches `deadline`. Returns
    /// immediately when the deadline has already passed.
    fn sleep_until(&self, deadline: Duration);
}

mod real {
    #![expect(
        clippy::disallowed_methods,
        reason = "REQ-K-4: the single sanctioned wall-clock site (ASM-035)"
    )]

    use core::time::Duration;
    use std::time::Instant;

    use super::WallClock;

    /// The real monotonic clock, measured from its construction.
    #[derive(Debug, Clone, Copy)]
    pub struct StdWallClock {
        epoch: Instant,
    }

    impl StdWallClock {
        /// Starts a clock whose epoch is now.
        pub fn new() -> Self {
            Self {
                epoch: Instant::now(),
            }
        }
    }

    impl Default for StdWallClock {
        fn default() -> Self {
            Self::new()
        }
    }

    impl WallClock for StdWallClock {
        fn now(&self) -> Duration {
            self.epoch.elapsed()
        }

        fn sleep_until(&self, deadline: Duration) {
            let remaining = deadline.saturating_sub(self.now());
            if !remaining.is_zero() {
                std::thread::sleep(remaining);
            }
        }
    }
}

/// A hand-driven wall clock for tests and embedding. Clones share the same time.
///
/// Time starts at zero and moves only through [`advance`](Self::advance),
/// [`sleep_until`](WallClock::sleep_until) and the optional auto-advance, so paced runs are
/// instantaneous and reproducible.
#[derive(Debug, Clone)]
pub struct ManualWallClock {
    time: Arc<Mutex<Duration>>,
    auto_advance: Duration,
}

impl ManualWallClock {
    /// A clock at time zero that only moves when told to.
    pub fn new() -> Self {
        Self::with_auto_advance(Duration::ZERO)
    }

    /// A clock where every read of [`now`](WallClock::now) moves the time forward by `step`
    /// afterwards (the read returns the time before the move). Models work that takes wall time:
    /// elapsed time scales with the number of reads, so a run that reads more often (for
    /// example with phase timing) sees more elapsed time.
    pub fn with_auto_advance(step: Duration) -> Self {
        Self {
            time: Arc::new(Mutex::new(Duration::ZERO)),
            auto_advance: step,
        }
    }

    /// Moves the (shared) time forward by `d`, saturating at `Duration::MAX`.
    pub fn advance(&self, d: Duration) {
        let mut time = self.time.lock().unwrap_or_else(PoisonError::into_inner);
        *time = time.saturating_add(d);
    }
}

impl Default for ManualWallClock {
    fn default() -> Self {
        Self::new()
    }
}

impl WallClock for ManualWallClock {
    fn now(&self) -> Duration {
        let mut time = self.time.lock().unwrap_or_else(PoisonError::into_inner);
        let read = *time;
        *time = time.saturating_add(self.auto_advance);
        read
    }

    fn sleep_until(&self, deadline: Duration) {
        let mut time = self.time.lock().unwrap_or_else(PoisonError::into_inner);
        *time = (*time).max(deadline);
    }
}
