//! STUB (WI-13): the hard-coded M1 demo scenario (ASM-028).
//!
//! Signatures only.
#![expect(
    missing_docs,
    clippy::todo,
    reason = "stub until WI-13: signatures only, documented when implemented"
)]

use cgf_core::{EntityId, Geodetic};
use cgf_motion::GroundKinematicParams;

use crate::engine::Engine;
use crate::error::EngineError;

pub const ORIGIN_DEG: (f64, f64) = (52.0, 4.0);

pub fn ground_params() -> GroundKinematicParams {
    todo!("WI-13")
}

pub fn spawn_walking_skeleton(_engine: &mut Engine) -> Result<Vec<EntityId>, EngineError> {
    todo!("WI-13")
}

pub fn route_of(_index: usize) -> Vec<Geodetic> {
    todo!("WI-13")
}
