//! `umbra-motion`: built-in motion models for Umbra.
//!
//! Responsibility: implementations of the `MotionModel` trait
//! from `umbra-core` (REQ-M-1, REQ-M-2), starting with `builtin.ground.kinematic`.
//!
//! Dependency rule (REQ-A-2): depends on `umbra-core` only among workspace crates; never on
//! `umbra-kernel`. It contains no `unsafe` code.

#![forbid(unsafe_code)]

pub mod air;
pub mod factory;
pub mod ground;
pub mod sea;

pub use air::{AIR_KINEMATIC_ID, AirKinematic, AirKinematicParams};
pub use factory::{
    AirKinematicFactory, GroundKinematicFactory, SeaKinematicFactory, register_builtins,
};
pub use ground::{GROUND_KINEMATIC_ID, GroundKinematic, GroundKinematicParams, MotionParamError};
pub use sea::{SEA_KINEMATIC_ID, SeaKinematic, SeaKinematicParams};
