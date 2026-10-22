//! `builtin.air.kinematic`: a kinematic aircraft (REQ-M-2, REQ-M-1, REQ-M-4, ADR 0051,
//! ASM-607..609, ASM-611).
//!
//! Stateless like every motion model. Speed changes at `max_accel` within
//! `[min_speed, max_speed]`; the turn rate is `g * tan(max_bank) / max(v, 1 m/s)` capped by
//! `max_turn_rate`; the target height comes from the `MoveTo` destination, with climb and descent
//! limited by their rates and never below terrain plus `min_altitude_agl`. Pitch is the
//! flight-path angle and roll the bank used in the step.
//!
//! Status: M7 WI-1 stub. The signatures are final; the behavior is implemented in M7 WI-6 and the
//! acceptance tests (`tests/air.rs`, `tests/air_prop.rs`) are ignored until then.

use umbra_core::{
    Intent, KinematicState, Meters, MetersPerSecond, MetersPerSecondSquared, MotionModel,
    MotionModelId, Radians, RadiansPerSecond, Seconds, TerrainProvider,
};

use crate::ground::MotionParamError;

/// The namespaced id of this model (ADR 0002).
pub const AIR_KINEMATIC_ID: &str = "builtin.air.kinematic";

/// Limits of an aircraft.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AirKinematicParams {
    /// Maximum speed, m/s (> 0).
    pub max_speed: MetersPerSecond,
    /// Minimum speed, m/s (0 <= min < max).
    pub min_speed: MetersPerSecond,
    /// Maximum speed change rate, m/s^2 (> 0).
    pub max_accel: MetersPerSecondSquared,
    /// Maximum climb rate, m/s (> 0).
    pub max_climb_rate: MetersPerSecond,
    /// Maximum descent rate, m/s (> 0).
    pub max_descent_rate: MetersPerSecond,
    /// Maximum bank angle (0 < bank < pi/2).
    pub max_bank: Radians,
    /// Optional cap on the turn rate, rad/s.
    pub max_turn_rate: Option<RadiansPerSecond>,
    /// Lowest allowed height above the terrain, meters (>= 0).
    pub min_altitude_agl: Meters,
}

/// The `builtin.air.kinematic` motion model.
#[derive(Debug, Clone, PartialEq)]
pub struct AirKinematic {
    id: MotionModelId,
    params: AirKinematicParams,
}

impl AirKinematic {
    /// Builds the model.
    ///
    /// # Errors
    /// [`MotionParamError`] naming the first invalid limit.
    pub fn new(params: AirKinematicParams) -> Result<Self, MotionParamError> {
        let _ = params;
        unimplemented!("M7 WI-6")
    }

    /// The limits this model was built with.
    pub fn params(&self) -> AirKinematicParams {
        self.params
    }
}

impl MotionModel for AirKinematic {
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
        unimplemented!("M7 WI-6")
    }
}
