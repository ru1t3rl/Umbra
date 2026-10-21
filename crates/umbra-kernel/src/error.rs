//! Kernel errors.

/// Which route parameter was invalid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RouteParam {
    /// The arrival radius is not finite and > 0.
    ArrivalRadius,
    /// The cruise speed is not finite and > 0.
    CruiseSpeed,
    /// A waypoint has a non-finite coordinate.
    Waypoints,
}

impl core::fmt::Display for RouteParam {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::ArrivalRadius => "arrival radius",
            Self::CruiseSpeed => "cruise speed",
            Self::Waypoints => "waypoints",
        })
    }
}

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
    /// A route parameter is invalid.
    #[error("invalid route parameter: {0}")]
    InvalidRoute(RouteParam),
    /// The initial kinematic state has a non-finite component.
    #[error("initial entity state is not finite")]
    InvalidState,
    /// The behavior could not be instantiated at the tick rate (ADR 0013 amendment).
    #[error(transparent)]
    Instantiate(#[from] umbra_core::InstantiateError),
    /// No entity with this id exists (never spawned, or already removed; ADR 0028).
    #[error("unknown entity {0}")]
    UnknownEntity(umbra_core::EntityId),
    /// `update_remote` was called for an entity that has no remote track, an Owned one (REQ-K-5,
    /// ADR 0032).
    #[error("entity {0} has no remote track")]
    NotRemote(umbra_core::EntityId),
}
