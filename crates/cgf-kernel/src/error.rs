//! Kernel errors (STUB shape from the M1 plan; thiserror per CLAUDE.md rule 6).

/// Errors from the clock and the kernel.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum KernelError {
    /// The tick counter would pass `u64::MAX`.
    #[error("simulation time overflow at tick {0}")]
    TimeOverflow(u64),
    /// Entity ids are exhausted.
    #[error("entity ids exhausted")]
    IdsExhausted,
    /// A route needs at least one waypoint.
    #[error("route has no waypoints")]
    EmptyRoute,
    /// A route parameter (arrival radius or cruise speed) is invalid.
    #[error("invalid route parameter: {0}")]
    InvalidRoute(&'static str),
}
