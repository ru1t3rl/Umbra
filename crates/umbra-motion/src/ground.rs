//! `builtin.ground.kinematic`: a kinematic ground vehicle (REQ-M-2, REQ-M-1, REQ-M-4,
//! ADR 0002, ASM-030, ASM-031, ASM-032).
//!
//! The model is stateless: [`MotionModel::step`] is a pure function of its inputs. Speed and
//! heading change at limited rates, the entity follows the terrain height, and pitch and roll
//! stay zero (ASM-032). All trigonometry goes through `umbra_core::math` (ADR 0003).

use umbra_core::math;
use umbra_core::{
    Ecef, Enu, Euler, Intent, KinematicState, LocalFrame, MetersPerSecond, MetersPerSecondSquared,
    MotionModel, MotionModelId, Radians, RadiansPerSecond, Seconds, TerrainProvider,
};

/// The namespaced id of this model (ADR 0002).
pub const GROUND_KINEMATIC_ID: &str = "builtin.ground.kinematic";

/// Smallest distance (meters) at which the arrival snap applies even for a stopped entity.
const ARRIVAL_EPSILON_M: f64 = 1e-3;

/// Limits of a ground vehicle. All values must be finite and strictly positive.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GroundKinematicParams {
    /// Maximum speed, m/s.
    pub max_speed: MetersPerSecond,
    /// Maximum heading change rate, rad/s.
    pub max_turn_rate: RadiansPerSecond,
    /// Maximum speed change rate (acceleration and deceleration), m/s^2.
    pub max_accel: MetersPerSecondSquared,
}

/// The `builtin.ground.kinematic` motion model.
///
/// Heading is clockwise from true north in the local frame. The entity never reverses: speed is
/// non-negative. See [`MotionModel::step`] below for the per-step algorithm.
#[derive(Debug, Clone, PartialEq)]
pub struct GroundKinematic {
    id: MotionModelId,
    params: GroundKinematicParams,
}

/// An invalid motion-model parameter.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum MotionParamError {
    /// The built-in model id was rejected by the id grammar (cannot happen with the constant).
    #[error("the built-in motion model id is invalid")]
    InvalidId,
    /// A limit was zero, negative, NaN or infinite.
    #[error("{name} must be finite and > 0, got {value}")]
    NotPositive {
        /// The offending parameter.
        name: &'static str,
        /// The rejected value.
        value: f64,
    },
    /// A value was NaN or infinite (it may be zero or negative).
    #[error("{name} must be finite, got {value}")]
    NotFinite {
        /// The offending parameter.
        name: &'static str,
        /// The rejected value.
        value: f64,
    },
    /// A value was outside its allowed range (for example a minimum speed that is not below the
    /// maximum, or a bank angle outside (0, 90) degrees).
    #[error("{name} must be {expected}, got {value}")]
    OutOfRange {
        /// The offending parameter.
        name: &'static str,
        /// The rejected value, in the model's units.
        value: f64,
        /// The allowed range, in words.
        expected: &'static str,
    },
}

pub(crate) fn require_positive(name: &'static str, value: f64) -> Result<(), MotionParamError> {
    if value.is_finite() && value > 0.0 {
        Ok(())
    } else {
        Err(MotionParamError::NotPositive { name, value })
    }
}

impl GroundKinematic {
    /// Builds the model.
    ///
    /// # Errors
    /// [`MotionParamError::NotPositive`] naming the first limit that is not finite and > 0.
    pub fn new(params: GroundKinematicParams) -> Result<Self, MotionParamError> {
        require_positive("max_speed", params.max_speed.0)?;
        require_positive("max_turn_rate", params.max_turn_rate.0)?;
        require_positive("max_accel", params.max_accel.0)?;
        // The constant is a valid id by construction; report an error rather than panic should
        // that ever change.
        let id =
            MotionModelId::new(GROUND_KINEMATIC_ID).map_err(|_| MotionParamError::InvalidId)?;
        Ok(Self { id, params })
    }

    /// The limits this model was built with.
    pub fn params(&self) -> GroundKinematicParams {
        self.params
    }

    /// Reduces the intents by the ASM-030 rules.
    fn resolve(&self, intents: &[Intent]) -> Resolved {
        let mut hold = false;
        let mut destination = None;
        let mut heading = None;
        let mut set_speed = None;
        for intent in intents {
            match intent {
                Intent::Hold => hold = true,
                Intent::MoveTo { destination: d } if is_finite_ecef(*d) => {
                    destination = Some(*d);
                    heading = None;
                }
                // ADR 0016: the last of `MoveTo` and `SetHeading` steers; a non-finite heading
                // is ignored like a non-finite destination.
                Intent::SetHeading(h) if h.0.is_finite() => {
                    heading = Some(h.0);
                    destination = None;
                }
                // ASM-048: a non-finite speed request means "stop".
                Intent::SetSpeed(v) => set_speed = Some(if v.0.is_finite() { v.0 } else { 0.0 }),
                _ => {}
            }
        }
        let max = self.params.max_speed.0;
        if hold {
            return Resolved {
                destination: None,
                heading: None,
                cruise: 0.0,
            };
        }
        let cruise = match (destination, set_speed) {
            (_, Some(v)) => v.clamp(0.0, max),
            (Some(_), None) => max,
            (None, None) => 0.0,
        };
        Resolved {
            destination,
            heading,
            cruise,
        }
    }
}

fn is_finite_ecef(p: Ecef) -> bool {
    p.0.x.is_finite() && p.0.y.is_finite() && p.0.z.is_finite()
}

/// Intents reduced by the ASM-030 rules.
struct Resolved {
    destination: Option<Ecef>,
    /// Commanded true heading (rad) when `SetHeading` was the last steering intent.
    heading: Option<f64>,
    cruise: f64,
}

/// Horizontal geometry to the destination in the entity's local frame.
struct Approach {
    east: f64,
    north: f64,
    distance: f64,
    /// Heading error, rad, in (-pi, pi].
    error: f64,
}

fn approach(frame: &LocalFrame, destination: Ecef, heading: f64) -> Approach {
    let offset = frame.to_enu(destination);
    let distance = math::hypot(offset.east, offset.north);
    let error = if distance <= 1e-12 {
        0.0
    } else {
        math::wrap_pi(Radians(math::atan2(offset.east, offset.north) - heading)).0
    };
    Approach {
        east: offset.east,
        north: offset.north,
        distance,
        error,
    }
}

impl GroundKinematic {
    /// Desired speed toward the destination: cruise, discrete stopping speed, and turning-circle
    /// limit.
    ///
    /// The stopping speed is the largest `v` from which repeated decelerations of `a * dt` stop
    /// within `d`: `v = sqrt(h^2 + 2 a d) - h` with `h = a dt / 2`.
    fn desired_speed(&self, cruise: f64, a: &Approach, dt: f64) -> f64 {
        let p = &self.params;
        let h = 0.5 * p.max_accel.0 * dt;
        let braking = (h * h + 2.0 * p.max_accel.0 * a.distance).sqrt() - h;
        let mut v = cruise.min(braking);
        let sin_e = math::sin(a.error).abs();
        if sin_e > 0.0 {
            v = v.min(p.max_turn_rate.0 * a.distance / (2.0 * sin_e));
        }
        v
    }
}

impl MotionModel for GroundKinematic {
    fn id(&self) -> &MotionModelId {
        &self.id
    }

    /// One step of the ground model.
    ///
    /// 1. Intents are resolved (ASM-030): `Hold` wins; otherwise the last of `MoveTo` and
    ///    `SetHeading` steers (ADR 0016; the entity turns to the heading, then holds it) and the
    ///    last `SetSpeed` wins; no intent decelerates to a stop; unknown variants are ignored.
    /// 2. Speed moves toward the desired speed by at most `max_accel * dt`, heading toward the
    ///    destination bearing by at most `max_turn_rate * dt`.
    ///    The desired speed uses the discrete stopping speed, so a decelerating entity reaches
    ///    the destination at about `max_accel * dt` or less. The heading changes only while the
    ///    new speed is positive (no pivot turns, ASM-049).
    /// 3. Arrival snap: when the destination is within this step's travel (or 1 mm), ahead, and
    ///    both the old and the new speed are at most `2 * max_accel * dt`, the entity lands on it and
    ///    stops. That final step changes speed by at most `2 * max_accel * dt`; a faster entity
    ///    keeps moving.
    /// 4. Terrain following: the new height is `terrain.height_at`; pitch and roll are zero.
    ///
    /// Precondition: `dt` is finite and > 0; otherwise the state is returned unchanged.
    fn step(
        &self,
        state: &KinematicState,
        intents: &[Intent],
        dt: Seconds,
        terrain: &dyn TerrainProvider,
    ) -> KinematicState {
        if !(dt.0.is_finite() && dt.0 > 0.0) {
            return *state;
        }
        let p = &self.params;
        let frame = LocalFrame::at(state.position.to_geodetic());
        let heading = frame.euler_from_orientation(state.orientation).heading.0;
        let speed = state.velocity.norm();
        let resolved = self.resolve(intents);
        let dt = dt.0;

        let toward = resolved.destination.map(|d| approach(&frame, d, heading));
        let desired = toward.as_ref().map_or(resolved.cruise, |a| {
            self.desired_speed(resolved.cruise, a, dt)
        });
        let accel_step = p.max_accel.0 * dt;
        let mut new_speed = speed + (desired - speed).clamp(-accel_step, accel_step);
        let turn_step = p.max_turn_rate.0 * dt;
        let turn_error = toward.as_ref().map(|a| a.error).or_else(|| {
            resolved
                .heading
                .map(|h| math::wrap_pi(Radians(h - heading)).0)
        });
        let new_heading = match turn_error {
            Some(error) if new_speed > 0.0 => {
                math::wrap_pi(Radians(heading + error.clamp(-turn_step, turn_step))).0
            }
            _ => heading,
        };

        let travel = new_speed * dt;
        let (east, north) = match &toward {
            Some(a)
                if a.distance <= travel.max(ARRIVAL_EPSILON_M)
                    && speed.max(new_speed) <= 2.0 * accel_step
                    && a.error.abs() < core::f64::consts::FRAC_PI_2 =>
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
        // A non-finite terrain height keeps the (finite) height of the moved point.
        let grounded = moved.with_height(terrain.height_at(moved)).unwrap_or(moved);
        let new_frame = LocalFrame::at(grounded);
        let (s, c) = math::sin_cos(new_heading);
        KinematicState {
            position: grounded.to_ecef(),
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
