//! STUB (WI-8): `builtin.ground.kinematic` (REQ-M-2, ASM-030, ASM-031).
//!
//! Signatures only. Acceptance tests: `tests/ground_kinematic.rs` (AT-8).
#![expect(
    missing_docs,
    clippy::todo,
    reason = "stub until WI-8: signatures only, documented when implemented"
)]

use cgf_core::{
    Intent, KinematicState, MetersPerSecond, MetersPerSecondSquared, MotionModel, MotionModelId,
    RadiansPerSecond, Seconds, TerrainProvider,
};

pub const GROUND_KINEMATIC_ID: &str = "builtin.ground.kinematic";

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GroundKinematicParams {
    pub max_speed: MetersPerSecond,
    pub max_turn_rate: RadiansPerSecond,
    pub max_accel: MetersPerSecondSquared,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GroundKinematic {
    id: MotionModelId,
    params: GroundKinematicParams,
}

impl GroundKinematic {
    pub fn new(_params: GroundKinematicParams) -> Result<Self, MotionParamError> {
        todo!("WI-8")
    }
    pub fn params(&self) -> GroundKinematicParams {
        self.params
    }
}

impl MotionModel for GroundKinematic {
    fn id(&self) -> &MotionModelId {
        &self.id
    }
    fn step(
        &self,
        _state: &KinematicState,
        _intents: &[Intent],
        _dt: Seconds,
        _terrain: &dyn TerrainProvider,
    ) -> KinematicState {
        todo!("WI-8")
    }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum MotionParamError {
    #[error("{name} must be finite and > 0, got {value}")]
    NotPositive { name: &'static str, value: f64 },
}
