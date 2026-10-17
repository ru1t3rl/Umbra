//! STUB (WI-7): `KinematicState`, `MotionModel` and contract checks (REQ-M-1, ASM-031).
#![expect(
    missing_docs,
    clippy::todo,
    reason = "stub until WI-7: documented when implemented"
)]

use crate::coords::Ecef;
use crate::geo::{Quaternion, Vec3};
use crate::id::MotionModelId;
use crate::intent::Intent;
use crate::terrain::TerrainProvider;
use crate::units::Seconds;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KinematicState {
    pub position: Ecef,
    pub velocity: Vec3,
    pub orientation: Quaternion,
}

pub trait MotionModel: Send + Sync {
    fn id(&self) -> &MotionModelId;
    fn step(
        &self,
        state: &KinematicState,
        intents: &[Intent],
        dt: Seconds,
        terrain: &dyn TerrainProvider,
    ) -> KinematicState;
}

pub mod contract {
    //! LSP contract checks for motion models.
    use super::{Intent, KinematicState, MotionModel, Seconds, TerrainProvider};

    #[derive(Debug, Clone, PartialEq, thiserror::Error)]
    #[non_exhaustive]
    pub enum ContractViolation {
        #[error("case {case}: step is not deterministic")]
        NotDeterministic { case: usize },
        #[error("case {case}: step produced a non-finite value")]
        NonFinite { case: usize },
        #[error("case {case}: orientation is not a unit quaternion")]
        NotUnitQuaternion { case: usize },
        #[error("case {case}: a stationary entity moved")]
        StationaryMoved { case: usize },
    }

    pub fn check_motion_model(
        _model: &dyn MotionModel,
        _cases: &[(KinematicState, Vec<Intent>)],
        _dt: Seconds,
        _terrain: &dyn TerrainProvider,
    ) -> Result<(), ContractViolation> {
        todo!("WI-7")
    }
}
