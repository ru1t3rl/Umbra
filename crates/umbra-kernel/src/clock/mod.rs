//! The simulation clock and its modes (REQ-K-3, REQ-K-8, REQ-C-1, D-5, ASM-035).
//!
//! The [`Clock`] owns [`SimTime`] and decides pacing; systems only ever see `now` and `dt`
//! (REQ-C-1). Four modes exist ([`ClockMode`]); pause and resume work in all of them. Pacing is
//! a pure function of the tick count and a [`WallClock`], so the sequence of simulated states is
//! identical in every mode (REQ-K-4, D-5).
//!
//! Real-time and scaled deadlines are computed from integer tick differences since an anchor:
//! `deadline(k) = anchor_wall + (k - anchor_tick) * dt / scale`. The anchor is reset on every mode
//! change and resume, so a pause or a switch from a faster mode never causes a burst of owed
//! ticks. When behind schedule ticks run back to back and are never skipped (REQ-K-8).

mod wall;

use core::time::Duration;

use umbra_core::{SimTime, TickRate};

use crate::error::KernelError;

pub use wall::{ManualWallClock, StdWallClock, WallClock};

/// A wall-time speed-up factor in `[1e-3, 1e6]` (`2.0` = twice as fast as real time).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimeScale(f64);

impl TimeScale {
    /// The smallest accepted factor.
    pub const MIN: f64 = 1e-3;
    /// The largest accepted factor.
    pub const MAX: f64 = 1e6;

    /// Validates the factor.
    ///
    /// # Errors
    /// [`ClockError::InvalidScale`] unless `s` is finite and within `[MIN, MAX]`
    /// (`[1e-3, 1e6]`).
    pub fn new(s: f64) -> Result<Self, ClockError> {
        if (Self::MIN..=Self::MAX).contains(&s) {
            Ok(Self(s))
        } else {
            Err(ClockError::InvalidScale)
        }
    }

    /// The factor.
    pub fn get(self) -> f64 {
        self.0
    }
}

/// Clock configuration errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ClockError {
    /// A time scale was zero, negative, NaN or infinite.
    #[error("time scale must be finite and within [0.001, 1000000]")]
    InvalidScale,
}

/// How the clock paces ticks against wall time (REQ-K-3).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ClockMode {
    /// One tick per tick period of wall time.
    RealTime,
    /// Real time multiplied by a factor.
    Scaled(TimeScale),
    /// Tick whenever asked; no wall time involved.
    AsFastAsPossible,
    /// Tick only when steps were requested with [`Clock::request_steps`].
    Stepped,
}

/// What the driver should do next, as answered by [`Clock::poll`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pace {
    /// Run a tick now, then call [`Clock::advance`].
    Tick,
    /// Wait until this wall-clock time (see [`Clock::wait_until`]), then poll again.
    WaitUntil(Duration),
    /// Nothing to do: paused, or stepped with no pending steps.
    Idle,
}

/// The simulation clock (REQ-C-1, REQ-K-3).
pub struct Clock {
    mode: ClockMode,
    now: SimTime,
    paused: bool,
    pending_steps: u64,
    /// `(wall time, tick)` the real-time schedule is measured from; set lazily by `poll`.
    anchor: Option<(Duration, u64)>,
    wall: Box<dyn WallClock>,
}

impl Clock {
    /// A clock at tick 0 in `mode`, running (not paused).
    pub fn new(rate: TickRate, mode: ClockMode, wall: Box<dyn WallClock>) -> Self {
        Self::starting_at(SimTime::start(rate), mode, wall)
    }

    /// A clock starting at an arbitrary `time` (resuming a run; also makes the overflow path of
    /// [`advance`](Self::advance) reachable in tests).
    pub fn starting_at(time: SimTime, mode: ClockMode, wall: Box<dyn WallClock>) -> Self {
        Self {
            mode,
            now: time,
            paused: false,
            pending_steps: 0,
            anchor: None,
            wall,
        }
    }

    /// The current simulation time (the next tick to run).
    pub fn now(&self) -> SimTime {
        self.now
    }

    /// The current mode.
    pub fn mode(&self) -> ClockMode {
        self.mode
    }

    /// Switches mode and re-anchors the real-time schedule. Setting the current mode again does
    /// nothing. Leaving stepped mode drops its pending steps, so they cannot fire after a later
    /// return to it.
    pub fn set_mode(&mut self, m: ClockMode) {
        if m == self.mode {
            return;
        }
        if self.mode == ClockMode::Stepped {
            self.pending_steps = 0;
        }
        self.mode = m;
        self.anchor = None;
    }

    /// Stops ticking in every mode until [`resume`](Self::resume).
    pub fn pause(&mut self) {
        self.paused = true;
        self.anchor = None;
    }

    /// Resumes after [`pause`](Self::pause), re-anchoring so no ticks are owed for the pause.
    pub fn resume(&mut self) {
        self.paused = false;
        self.anchor = None;
    }

    /// Whether the clock is paused.
    pub fn is_paused(&self) -> bool {
        self.paused
    }

    /// Adds `n` ticks to the pending steps (consumed in [`ClockMode::Stepped`]).
    pub fn request_steps(&mut self, n: u64) {
        self.pending_steps = self.pending_steps.saturating_add(n);
    }

    /// Drops the steps that were requested but not yet run.
    pub fn cancel_steps(&mut self) {
        self.pending_steps = 0;
    }

    /// Says whether a tick is due. Never advances simulation time.
    pub fn poll(&mut self) -> Pace {
        if self.paused {
            return Pace::Idle;
        }
        match self.mode {
            ClockMode::AsFastAsPossible => Pace::Tick,
            ClockMode::Stepped if self.pending_steps > 0 => Pace::Tick,
            ClockMode::Stepped => Pace::Idle,
            ClockMode::RealTime | ClockMode::Scaled(_) => {
                let deadline = self.next_deadline();
                if self.wall.now() >= deadline {
                    Pace::Tick
                } else {
                    Pace::WaitUntil(deadline)
                }
            }
        }
    }

    /// Moves time forward by exactly one tick and returns the new time. In stepped mode this
    /// consumes one pending step.
    ///
    /// # Errors
    /// [`KernelError::TimeOverflow`] when the tick counter would pass `u64::MAX`; time and
    /// pending steps are left unchanged.
    pub fn advance(&mut self) -> Result<SimTime, KernelError> {
        let next = self
            .now
            .checked_next()
            .ok_or(KernelError::TimeOverflow(self.now.tick()))?;
        if self.scale().is_some() && !self.paused {
            self.anchor_now();
        }
        if self.mode == ClockMode::Stepped {
            self.pending_steps = self.pending_steps.saturating_sub(1);
        }
        self.now = next;
        Ok(next)
    }

    /// In real-time and scaled modes: how far the wall clock is past the deadline of the tick
    /// just run, when that lag exceeds one (wall) tick period (REQ-K-8). Call it after
    /// [`advance`](Self::advance). `None` otherwise, also while paused or before any tick since
    /// the last anchor.
    pub fn behind(&self) -> Option<Duration> {
        let (anchor_wall, anchor_tick) = self.anchor?;
        let scale = self.scale()?;
        let hz = self.now.rate().hz();
        let last_run = self.now.tick().checked_sub(1)?;
        let ticks = last_run.checked_sub(anchor_tick)?;
        let due = anchor_wall.saturating_add(wall_offset(ticks, hz, scale));
        let lag = self.wall.now().saturating_sub(due);
        (lag > wall_offset(1, hz, scale)).then_some(lag)
    }

    /// The wall-time budget of one tick: one tick period divided by the time scale. `None` in
    /// modes without a wall schedule (as fast as possible, stepped).
    pub fn wall_period(&self) -> Option<Duration> {
        let scale = self.scale()?;
        Some(wall_offset(1, self.now.rate().hz(), scale))
    }

    /// The wall clock's current time.
    pub fn wall_now(&self) -> Duration {
        self.wall.now()
    }

    /// Waits on the wall clock until `d`.
    pub fn wait_until(&self, d: Duration) {
        self.wall.sleep_until(d);
    }

    /// The wall-time factor of a paced mode, `None` for modes without a wall schedule.
    fn scale(&self) -> Option<f64> {
        match self.mode {
            ClockMode::RealTime => Some(1.0),
            ClockMode::Scaled(s) => Some(s.get()),
            ClockMode::AsFastAsPossible | ClockMode::Stepped => None,
        }
    }

    /// Sets the anchor to (wall now, current tick) unless one exists.
    fn anchor_now(&mut self) -> (Duration, u64) {
        *self
            .anchor
            .get_or_insert_with(|| (self.wall.now(), self.now.tick()))
    }

    /// Wall time at which the tick `now` is due, anchoring the schedule first if necessary.
    fn next_deadline(&mut self) -> Duration {
        let (anchor_wall, anchor_tick) = self.anchor_now();
        let scale = self.scale().unwrap_or(1.0);
        let ticks = self.now.tick().saturating_sub(anchor_tick);
        anchor_wall.saturating_add(wall_offset(ticks, self.now.rate().hz(), scale))
    }
}

/// Wall time `ticks` tick periods take at `scale`: `ticks / (hz * scale)` seconds, from the
/// integer tick count (saturating at `Duration::MAX`).
fn wall_offset(ticks: u64, hz: u32, scale: f64) -> Duration {
    // u64 -> f64 is exact below 2^53 ticks; beyond that only the sub-tick rounding of an
    // astronomically distant deadline changes.
    #[expect(clippy::cast_precision_loss, reason = "exact below 2^53 ticks")]
    let secs = ticks as f64 / (f64::from(hz) * scale);
    Duration::try_from_secs_f64(secs).unwrap_or(Duration::MAX)
}
