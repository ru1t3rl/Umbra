//! `builtin.sea.kinematic`: a kinematic surface vessel (REQ-M-2, REQ-M-1, REQ-M-4, ADR 0051,
//! ASM-610, ASM-611).
//!
//! Stateless like every motion model. Speed changes at a limited rate, the heading changes at
//! `v / min_turn_radius`, the vessel never reverses, its height is pinned to `surface_height`
//! (terrain and `MoveTo` heights are ignored) and pitch and roll stay zero.
//!
//! A `MoveTo` is approached at up to `max_speed` and the vessel slows to land on it (the
//! arrival snap of the ground model). Because the turn rate is `v / min_turn_radius`, the path
//! radius is `min_turn_radius` at every speed, so a destination strictly inside the turning circle
//! on its side cannot be reached by turning. The vessel then holds its heading (steers out) until
//! straight travel has carried the destination outside that circle (plus a margin of two step
//! lengths), and only then turns in at the maximum rate. The hold band (ASM-694) keeps a destination
//! that has just left the hold zone from dipping back in, which is what used to leave a lateral
//! miss on the final approach and fly another circle. The path to a destination at distance `d` is
//! at most `d + 2 pi R + 2 R + 2 m + max_speed^2 / max_accel` (`m` = the margin; the last term is
//! the stopping overshoot, out and back) while `max_speed * dt <= R`; a longer step has no circle
//! to bound by (the sweep in ASM-694 still arrives). No pivot turns: the heading
//! changes only while the vessel moves. All trigonometry goes through `umbra_core::math`
//! (ADR 0003).

use core::f64::consts::FRAC_PI_2;

use umbra_core::math;
use umbra_core::{
    Enu, Euler, Geodetic, Intent, KinematicState, LocalFrame, Meters, MetersPerSecond,
    MetersPerSecondSquared, MotionModel, MotionModelId, Radians, Seconds, TerrainProvider,
};

use crate::ground::{MotionParamError, require_positive};
use crate::steer::{self, ARRIVAL_EPSILON_M};

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
        require_positive("max_speed", params.max_speed.0)?;
        require_positive("max_accel", params.max_accel.0)?;
        require_positive("min_turn_radius", params.min_turn_radius.0)?;
        if !params.surface_height.0.is_finite() {
            return Err(MotionParamError::NotFinite {
                name: "surface_height",
                value: params.surface_height.0,
            });
        }
        let id = MotionModelId::new(SEA_KINEMATIC_ID).map_err(|_| MotionParamError::InvalidId)?;
        Ok(Self { id, params })
    }

    /// The limits this model was built with.
    pub fn params(&self) -> SeaKinematicParams {
        self.params
    }
}

impl SeaKinematic {
    /// Cruise speed for the reduced intents: `Hold` stops, `SetSpeed` is clamped to
    /// `[0, max_speed]`, a `MoveTo` alone means `max_speed`, nothing means stop.
    fn cruise(&self, s: &steer::Steering) -> f64 {
        let max = self.params.max_speed.0;
        if s.hold {
            0.0
        } else if let Some(v) = s.set_speed {
            v.clamp(0.0, max)
        } else if s.destination.is_some() {
            max
        } else {
            0.0
        }
    }
}

impl MotionModel for SeaKinematic {
    fn id(&self) -> &MotionModelId {
        &self.id
    }

    /// One step of the sea model.
    ///
    /// 1. Intents are reduced like the ground model's (`Hold` wins, the last of `MoveTo` and
    ///    `SetHeading` steers, the last `SetSpeed` wins, nothing decelerates to a stop).
    /// 2. Speed moves toward the desired speed (cruise, limited by the stopping speed to a
    ///    destination) by at most `max_accel * dt`; it never goes below zero.
    /// 3. The heading moves toward the bearing (or the commanded heading) by at most
    ///    `new_speed / min_turn_radius * dt`, so it does not change at rest (no pivot turns).
    ///    Exception: while the destination is inside the turn-side circle of radius
    ///    `min_turn_radius` (plus a margin of two step lengths behind the vessel) the heading is held, so straight
    ///    travel carries the destination out of the circle; the vessel then turns in at the
    ///    maximum rate. The rule is geometric and stateless and holds for any speed and any
    ///    destination. A destination directly behind is inside only while it is closer than
    ///    `sqrt(2 R m + m^2)` (about 8 m at the defaults); farther ones are turned toward at once.
    /// 4. Arrival snap (the ground model's): a destination within this step's travel (or 1 mm),
    ///    ahead (or within 1 mm in any direction), with both speeds at most `2 * max_accel * dt`,
    ///    is landed on and the vessel stops. There is no capture of a pass at a distance: a
    ///    lateral miss is prevented by the hold band of step 3, not by moving the vessel
    ///    sideways. The 1 mm rule absorbs the sub-millimeter horizontal residual that
    ///    the pinned height leaves between the landing point and a destination at another height
    ///    (without it the vessel could land "behind" the destination and sail away).
    /// 5. The height is `surface_height` (terrain and `MoveTo` heights ignored); pitch and roll
    ///    are zero.
    ///
    /// Precondition: `dt` is finite and > 0; otherwise the state is returned unchanged.
    fn step(
        &self,
        state: &KinematicState,
        intents: &[Intent],
        dt: Seconds,
        _terrain: &dyn TerrainProvider,
    ) -> KinematicState {
        if !(dt.0.is_finite() && dt.0 > 0.0) {
            return *state;
        }
        let p = &self.params;
        let dt = dt.0;
        let frame = LocalFrame::at(state.position.to_geodetic());
        let heading = frame.euler_from_orientation(state.orientation).heading.0;
        let speed = state.velocity.norm();
        let reduced = steer::reduce(intents);
        let cruise = self.cruise(&reduced);
        let toward = reduced
            .destination
            .map(|d| steer::approach(&frame, d, heading));
        let desired = toward.as_ref().map_or(cruise, |a| {
            cruise.min(steer::stopping_speed(p.max_accel.0, dt, a.distance))
        });
        let accel_step = p.max_accel.0 * dt;
        let mut new_speed = (speed + (desired - speed).clamp(-accel_step, accel_step)).max(0.0);
        let turn_step = new_speed / p.min_turn_radius.0 * dt;
        let margin = 2.0 * speed.max(new_speed) * dt;
        let steer_out = toward
            .as_ref()
            .is_some_and(|a| steer::inside_turn_circle(a, p.min_turn_radius.0, margin));
        // The turn step is proportional to the speed, so a vessel at rest does not pivot.
        let error = if steer_out {
            None
        } else {
            steer::turn_error(toward.as_ref(), reduced.heading, heading)
        };
        let new_heading = match error {
            Some(error) => math::wrap_pi(Radians(heading + error.clamp(-turn_step, turn_step))).0,
            None => heading,
        };

        let travel = new_speed * dt;
        let (east, north) = match &toward {
            Some(a)
                if a.distance <= travel.max(ARRIVAL_EPSILON_M)
                    && speed.max(new_speed) <= 2.0 * accel_step
                    && (a.error.abs() < FRAC_PI_2 || a.distance <= ARRIVAL_EPSILON_M) =>
            {
                new_speed = 0.0;
                (a.east, a.north)
            }
            _ => {
                let (s, c) = math::sin_cos(new_heading);
                (travel * s, travel * c)
            }
        };

        let moved = frame
            .from_enu(Enu {
                east,
                north,
                up: 0.0,
            })
            .to_geodetic();
        // `surface_height` is finite (checked in `new`), so the height is always accepted; the
        // fallback keeps the moved point should that ever change.
        let on_surface: Geodetic = moved.with_height(p.surface_height).unwrap_or(moved);
        let new_frame = LocalFrame::at(on_surface);
        let (s, c) = math::sin_cos(new_heading);
        KinematicState {
            position: on_surface.to_ecef(),
            velocity: new_frame.vector_from_enu(Enu {
                east: new_speed * s,
                north: new_speed * c,
                up: 0.0,
            }),
            orientation: new_frame.orientation_from_euler(Euler {
                heading: Radians(new_heading),
                pitch: Radians(0.0),
                roll: Radians(0.0),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::factory::located;

    /// The factory's key mapping covers the `NotFinite` arm (the sea `surface_height`); an
    /// infinite JSON number cannot reach it through `build` because it serialises to null.
    #[test]
    fn located_points_a_not_finite_error_at_its_key() {
        let error = located(&MotionParamError::NotFinite {
            name: "surface_height",
            value: f64::INFINITY,
        });

        assert_eq!(error.param.as_deref(), Some("/surface_height_m"));
        assert!(
            error
                .to_string()
                .contains("`surface_height_m` must be finite, got inf"),
            "{error}"
        );
    }

    #[test]
    fn located_leaves_an_unknown_name_unlocated() {
        let error = located(&MotionParamError::NotFinite {
            name: "nope",
            value: f64::NAN,
        });

        assert_eq!(error.param, None);
    }
}
