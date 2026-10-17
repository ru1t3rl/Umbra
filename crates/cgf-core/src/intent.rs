//! The M1 subset of behavior intents (REQ-B-2, ASM-030).
//!
//! Behaviors express what an entity wants; motion models decide how (REQ-B-1, REQ-M-1). The enum
//! is `#[non_exhaustive]`: later milestones add variants, so consumers must ignore unknown ones.

use crate::coords::Ecef;
use crate::units::MetersPerSecond;

/// What an entity wants to do this tick.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// Go to `destination` (ECEF position, D-15) and stop there.
    MoveTo {
        /// Target position, ECEF meters.
        destination: Ecef,
    },
    /// Cruise at this speed (clamped by the model's limits).
    SetSpeed(MetersPerSecond),
    /// Stop and keep the current heading; wins over every other intent.
    Hold,
}
