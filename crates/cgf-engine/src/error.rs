//! Engine errors.

use cgf_kernel::KernelError;
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
    /// The configured terrain height is not finite.
    #[error("terrain height must be finite")]
    InvalidTerrainHeight,
    /// The demo scenario has no entity with this index.
    #[error("the demo scenario has no entity {0}")]
    DemoLayout(usize),
    /// Invalid motion model parameters.
    #[error(transparent)]
    Motion(#[from] MotionParamError),
}
