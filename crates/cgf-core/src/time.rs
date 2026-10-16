//! Simulation time (REQ-C-1, D-5, ASM-016).
//!
//! Time is an exact integer tick count plus a whole-hertz [`TickRate`]. Seconds are always
//! computed as `tick / hz` (two correctly rounded IEEE-754 operations), never by accumulating
//! `dt`, so there is no drift and results are deterministic (ADR 0003).

use core::cmp::Ordering;
use core::num::NonZeroU32;

use crate::units::Seconds;

/// Simulation tick rate in whole hertz. Never zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TickRate(NonZeroU32);

impl TickRate {
    /// Creates a rate from hertz; returns `None` for zero.
    pub const fn new(hz: u32) -> Option<Self> {
        match NonZeroU32::new(hz) {
            Some(n) => Some(Self(n)),
            None => None,
        }
    }

    /// The rate in hertz (always at least 1).
    pub const fn hz(self) -> u32 {
        self.0.get()
    }

    /// The duration of one tick, `1 / hz`.
    pub fn dt(self) -> Seconds {
        Seconds(1.0 / f64::from(self.hz()))
    }
}

/// A point in simulation time: a tick count at a fixed [`TickRate`].
///
/// This is simulated time, not wall-clock time (REQ-K-4). There is deliberately no `Ord`:
/// times with different rates are unordered ([`PartialOrd`] returns `None`). Key a `BTreeMap`
/// on [`SimTime::tick`] instead. `SimTime` is `Copy`; advancing returns a new value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SimTime {
    tick: u64,
    rate: TickRate,
}

impl SimTime {
    /// Time zero (tick 0) at `rate`.
    pub const fn start(rate: TickRate) -> Self {
        Self { tick: 0, rate }
    }

    /// Time at an explicit tick.
    pub const fn at_tick(rate: TickRate, tick: u64) -> Self {
        Self { tick, rate }
    }

    /// The tick count.
    pub const fn tick(self) -> u64 {
        self.tick
    }

    /// The tick rate.
    pub const fn rate(self) -> TickRate {
        self.rate
    }

    /// The duration of one tick at this time's rate.
    pub fn dt(self) -> Seconds {
        self.rate.dt()
    }

    /// Elapsed simulation seconds, `tick / hz`. Ticks above 2^53 lose precision in `f64`
    /// (about 9e15 ticks, far beyond any realistic run).
    #[expect(clippy::cast_precision_loss, reason = "documented: exact below 2^53")]
    pub fn as_seconds(self) -> Seconds {
        Seconds(self.tick as f64 / f64::from(self.rate.hz()))
    }

    /// The next tick, or `None` on overflow.
    #[must_use = "returns the advanced time; SimTime is Copy and is not modified"]
    pub fn checked_next(self) -> Option<Self> {
        self.checked_advance(1)
    }

    /// Advances by `ticks`, or `None` on overflow.
    #[must_use = "returns the advanced time; SimTime is Copy and is not modified"]
    pub fn checked_advance(self, ticks: u64) -> Option<Self> {
        let tick = self.tick.checked_add(ticks)?;
        Some(Self { tick, ..self })
    }

    /// Ticks elapsed since `earlier`.
    ///
    /// # Errors
    /// [`TimeError::RateMismatch`] if the rates differ (`left` is `self`'s rate), or
    /// [`TimeError::NegativeInterval`] if `earlier` is after `self`.
    pub fn ticks_since(self, earlier: SimTime) -> Result<u64, TimeError> {
        if self.rate != earlier.rate {
            return Err(TimeError::RateMismatch {
                left: self.rate.hz(),
                right: earlier.rate.hz(),
            });
        }
        self.tick
            .checked_sub(earlier.tick)
            .ok_or(TimeError::NegativeInterval {
                from: earlier.tick,
                to: self.tick,
            })
    }
}

impl PartialOrd for SimTime {
    /// Orders by tick; `None` when the rates differ (ASM-016).
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        (self.rate == other.rate).then(|| self.tick.cmp(&other.tick))
    }
}

/// Errors from time arithmetic.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum TimeError {
    /// Two times with different tick rates were combined.
    #[error("tick rates differ: {left} Hz vs {right} Hz")]
    RateMismatch {
        /// Rate of the left operand in hertz.
        left: u32,
        /// Rate of the right operand in hertz.
        right: u32,
    },
    /// An interval was requested from a later time to an earlier one.
    #[error("time {to} is before {from}")]
    NegativeInterval {
        /// Tick of the reference time passed to `ticks_since` (the start of the interval).
        from: u64,
        /// Tick of the receiver (the end of the interval), which is before `from`.
        to: u64,
    },
}
