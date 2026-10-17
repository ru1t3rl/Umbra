//! Engine errors.

use cgf_kernel::KernelError;
use cgf_motion::MotionParamError;

/// Errors from the engine facade.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum EngineError {
    /// A kernel error.
    #[error(transparent)]
    Kernel(#[from] KernelError),
    /// Invalid motion model parameters.
    #[error(transparent)]
    Motion(#[from] MotionParamError),
}
