//! `cgf-motion`: built-in motion models for the CGF Engine.
//!
//! Responsibility (filled in by later M1 work items): implementations of the `MotionModel` trait
//! from `cgf-core` (REQ-M-1, REQ-M-2), starting with `builtin.ground.kinematic`.
//!
//! Dependency rule (REQ-A-2): depends on `cgf-core` only among workspace crates; never on
//! `cgf-kernel`. It contains no `unsafe` code.

#![forbid(unsafe_code)]

pub mod ground;

pub use ground::{GROUND_KINEMATIC_ID, GroundKinematic, GroundKinematicParams, MotionParamError};
