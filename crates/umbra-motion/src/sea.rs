//! `builtin.sea.kinematic`: a kinematic surface vessel (REQ-M-2, REQ-M-1, REQ-M-4, ADR 0051,
//! ASM-610, ASM-611).
//!
//! Stateless like every motion model. Speed changes at a limited rate, the heading changes at
//! `v / min_turn_radius`, the vessel never reverses, its height is pinned to `surface_height`
//! (terrain and `MoveTo` heights are ignored) and pitch and roll stay zero.
//!
//! Status: M7 WI-1 stub. The signatures are final; the behavior is implemented in M7 WI-5 and the
//! acceptance tests (`tests/sea.rs`, `tests/sea_prop.rs`) are ignored until then.

use umbra_core::{
    Intent, KinematicState, Meters, MetersPerSecond, MetersPerSecondSquared, MotionModel,
    MotionModelId, Seconds, TerrainProvider,
};

use crate::ground::MotionParamError;

/// The namespaced id of this model (ADR 0002).
pub const SEA_KINEMATIC_ID: &str = "builtin.sea.kinematic";

/// Limits of a surface vessel. Speed, acceleration and turn radius must be finite and > 0; the
/// surface height must be finite.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SeaKinematicParams {
    /// Maximum speed, m/s.
    pub max_speed: MetersPerSecond,
    /// Maximum speed change rate, m/s^2.
    pub max_accel: MetersPerSecondSquared,
    /// Smallest turn radius, meters (the turn rate is `v / min_turn_radius`).
    pub min_turn_radius: Meters,
    /// Height of the sea surface above the ellipsoid, meters.
    pub surface_height: Meters,
}

/// The `builtin.sea.kinematic` motion model.
#[derive(Debug, Clone, PartialEq)]
pub struct SeaKinematic {
    id: MotionModelId,
    params: SeaKinematicParams,
}

impl SeaKinematic {
    /// Builds the model.
    ///
    /// # Errors
    /// [`MotionParamError`] naming the first invalid limit.
    pub fn new(params: SeaKinematicParams) -> Result<Self, MotionParamError> {
        let _ = params;
        unimplemented!("M7 WI-5")
    }

    /// The limits this model was built with.
    pub fn params(&self) -> SeaKinematicParams {
        self.params
    }
}

impl MotionModel for SeaKinematic {
    fn id(&self) -> &MotionModelId {
        &self.id
    }

    fn step(
        &self,
        state: &KinematicState,
        intents: &[Intent],
        dt: Seconds,
        terrain: &dyn TerrainProvider,
    ) -> KinematicState {
        let _ = (state, intents, dt, terrain);
        unimplemented!("M7 WI-5")
    }
}
