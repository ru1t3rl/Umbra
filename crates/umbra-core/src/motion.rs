//! Kinematic state, the `MotionModel` trait and its contract checks (REQ-M-1, REQ-M-4, ASM-031).

use crate::coords::Ecef;
use crate::geo::{Quaternion, Vec3};
use crate::id::MotionModelId;
use crate::intent::Intent;
use crate::terrain::TerrainProvider;
use crate::units::Seconds;

/// Kinematic state of one entity. The canonical frame is ECEF (D-15).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KinematicState {
    /// Position, ECEF meters.
    pub position: Ecef,
    /// Velocity, ECEF meters per second.
    pub velocity: Vec3,
    /// Orientation: unit quaternion rotating body FRD axes to ECEF axes.
    pub orientation: Quaternion,
}

/// Turns intents into motion (REQ-M-1).
///
/// A model is stateless across ticks: equal inputs give bit-identical outputs. Its parameters are
/// bound at construction, so one instance can be shared by many entities
/// (`Arc<dyn MotionModel>`, REQ-M-4). The trait is object safe and infallible (ASM-031); use
/// [`contract::check_motion_model`] in every implementation's tests.
///
/// ```
/// use umbra_core::MotionModel;
/// fn _accepts(_: Box<dyn MotionModel>) {}
/// ```
pub trait MotionModel: Send + Sync {
    /// The model's namespaced ID (for example `builtin.ground.kinematic`).
    fn id(&self) -> &MotionModelId;

    /// Advances `state` by `dt` under `intents`, following `terrain` where the model is bound to it.
    ///
    /// Precondition: `dt` is finite and strictly positive. Implementations return `*state`
    /// unchanged when it is not.
    fn step(
        &self,
        state: &KinematicState,
        intents: &[Intent],
        dt: Seconds,
        terrain: &dyn TerrainProvider,
    ) -> KinematicState;
}

pub mod contract {
    //! LSP contract checks for [`MotionModel`] implementations (REQ-M-1).

    use super::{Intent, KinematicState, MotionModel, Seconds, TerrainProvider};

    /// Largest accepted deviation of the orientation quaternion's norm from 1.
    const UNIT_TOLERANCE: f64 = 1e-9;
    /// Largest displacement (meters, 1 mm) a stationary, intent-free entity may show.
    const STATIONARY_TOLERANCE_M: f64 = 1e-3;

    /// A broken [`MotionModel`] contract; `case` is the index of the offending test case.
    #[derive(Debug, Clone, PartialEq, thiserror::Error)]
    #[non_exhaustive]
    pub enum ContractViolation {
        /// Two calls with identical inputs gave different outputs.
        #[error("case {case}: step is not deterministic")]
        NotDeterministic {
            /// Index into the checked cases.
            case: usize,
        },
        /// The output contains NaN or infinity.
        #[error("case {case}: step produced a non-finite value")]
        NonFinite {
            /// Index into the checked cases.
            case: usize,
        },
        /// The output orientation is not a unit quaternion.
        #[error("case {case}: orientation is not a unit quaternion")]
        NotUnitQuaternion {
            /// Index into the checked cases.
            case: usize,
        },
        /// A stationary entity with no motion intent changed position.
        #[error("case {case}: a stationary entity moved")]
        StationaryMoved {
            /// Index into the checked cases.
            case: usize,
        },
    }

    fn components(s: &KinematicState) -> [f64; 10] {
        let (p, v, q) = (s.position.0, s.velocity, s.orientation);
        [p.x, p.y, p.z, v.x, v.y, v.z, q.w, q.x, q.y, q.z]
    }

    fn is_stationary_without_motion_intent(state: &KinematicState, intents: &[Intent]) -> bool {
        state.velocity == crate::geo::Vec3::ZERO
            && intents.iter().all(|i| matches!(i, Intent::Hold))
    }

    /// Checks that `model` is deterministic (two calls agree bit for bit), finite, returns a
    /// unit-quaternion orientation, and does not move (by more than 1 mm) a stationary entity whose
    /// intents are empty or only `Hold`. Cases should place stationary entities on the terrain, since a model may
    /// legitimately snap to it.
    ///
    /// # Errors
    /// The first [`ContractViolation`] found, with the case index.
    pub fn check_motion_model(
        model: &dyn MotionModel,
        cases: &[(KinematicState, Vec<Intent>)],
        dt: Seconds,
        terrain: &dyn TerrainProvider,
    ) -> Result<(), ContractViolation> {
        for (case, (state, intents)) in cases.iter().enumerate() {
            let first = model.step(state, intents, dt, terrain);
            let second = model.step(state, intents, dt, terrain);
            let (a, b) = (components(&first), components(&second));
            if !a.iter().chain(b.iter()).all(|c| c.is_finite()) {
                return Err(ContractViolation::NonFinite { case });
            }
            if a.iter().zip(&b).any(|(x, y)| x.to_bits() != y.to_bits()) {
                return Err(ContractViolation::NotDeterministic { case });
            }
            let q = first.orientation;
            let norm = (q.w * q.w + q.x * q.x + q.y * q.y + q.z * q.z).sqrt();
            if (norm - 1.0).abs() > UNIT_TOLERANCE {
                return Err(ContractViolation::NotUnitQuaternion { case });
            }
            if is_stationary_without_motion_intent(state, intents)
                && (first.position - state.position).norm() > STATIONARY_TOLERANCE_M
            {
                return Err(ContractViolation::StationaryMoved { case });
            }
        }
        Ok(())
    }
}
