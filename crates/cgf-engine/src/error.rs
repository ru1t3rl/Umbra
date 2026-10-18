//! Engine errors.

use cgf_core::{BuildError, ComponentId, ComponentKind};
use cgf_kernel::{ClockError, KernelError};
use cgf_motion::MotionParamError;

/// Which spawn parameter was invalid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SpawnParam {
    /// The heading is not finite.
    Heading,
    /// The speed is not finite or is negative.
    Speed,
    /// The position has a non-finite coordinate.
    Position,
}

/// Errors from the engine facade.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum EngineError {
    /// A kernel error.
    #[error(transparent)]
    Kernel(#[from] KernelError),
    /// A spawn parameter was invalid.
    #[error("invalid spawn parameter: {0:?}")]
    InvalidSpawn(SpawnParam),
    /// Invalid motion model parameters.
    #[error(transparent)]
    Motion(#[from] MotionParamError),
    /// A scenario clock setting the kernel rejects (not expected after validation).
    #[error(transparent)]
    Clock(#[from] ClockError),
    /// A component named by the scenario is not in the registry (not expected after validation).
    #[error("{kind} {id} is not registered")]
    UnknownComponent {
        /// Kind of the missing component.
        kind: ComponentKind,
        /// Its ID.
        id: ComponentId,
    },
    /// A motion model factory rejected the parameters of a placement (not expected after
    /// validation).
    #[error("entity {entity}: {source}")]
    Build {
        /// Index of the placement in file order, 0-based.
        entity: usize,
        /// The factory's error.
        #[source]
        source: BuildError,
    },
    /// The terrain factory rejected the scenario's terrain parameters (not expected after
    /// validation).
    #[error("terrain: {0}")]
    TerrainBuild(BuildError),
    /// A behavior factory rejected the parameters of a placement (not expected after validation).
    #[error("entity {entity}: behavior: {source}")]
    BehaviorBuild {
        /// Index of the placement in file order, 0-based.
        entity: usize,
        /// The factory's error.
        #[source]
        source: BuildError,
    },
    /// A sensor factory rejected the parameters of a placement (not expected after validation).
    #[error("entity {entity}: sensor {index}: {source}")]
    SensorBuild {
        /// Index of the placement in file order, 0-based.
        entity: usize,
        /// Index of the sensor in the placement's sensor list, 0-based.
        index: usize,
        /// The factory's error.
        #[source]
        source: BuildError,
    },
    /// Registering the scenario's graphs failed (not expected after validation).
    #[error(transparent)]
    Graphs(#[from] cgf_behavior::GraphRegistrationError),
}
