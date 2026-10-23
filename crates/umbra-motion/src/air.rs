//! `builtin.air.kinematic`: a kinematic aircraft (REQ-M-2, REQ-M-1, REQ-M-4, ADR 0051,
//! ASM-607..609, ASM-611, ASM-699..).
//!
//! Stateless like every motion model. The speed (the length of the velocity vector) changes at
//! most `max_accel * dt` per step and stays in `[min_speed, max_speed]` while the aircraft is
//! flying; the turn rate is `g * tan(max_bank) / max(v, 1 m/s)`, capped by `max_turn_rate`, so
//! the turn radius `v / rate` grows with the square of the speed; the target height comes from the
//! `MoveTo` destination, with climb and descent limited by their rates and never below terrain
//! plus `min_altitude_agl`. Pitch is the flight-path angle and roll the bank used in the step.
//!
//! A `MoveTo` is approached at up to `max_speed`. The aircraft slows so that the circle through the
//! destination is no tighter than its turn radius (the ground model's rule, with the radius
//! `v^2 / (g tan bank)`), and, when `min_speed > 0`, never below `min_speed`: it cannot stop, so a
//! destination inside the smallest turning circle (radius at `min_speed`) is handled like the sea
//! model's (ASM-694): the heading is held until straight travel has carried the destination out
//! of that circle, then the aircraft turns in at the maximum rate and flies over the destination.
//! With `min_speed = 0` the aircraft slows to a stop on the destination (the ground model's
//! arrival snap). All trigonometry goes through `umbra_core::math` (ADR 0003).

use umbra_core::math;
use umbra_core::{
    Enu, Euler, Geodetic, Intent, KinematicState, LocalFrame, Meters, MetersPerSecond,
    MetersPerSecondSquared, MotionModel, MotionModelId, Radians, RadiansPerSecond, Seconds,
    TerrainProvider,
};

use crate::ground::{MotionParamError, require_positive};
use crate::steer::{self, ARRIVAL_EPSILON_M};

/// The namespaced id of this model (ADR 0002).
pub const AIR_KINEMATIC_ID: &str = "builtin.air.kinematic";

/// The sine of the steepest flight-path angle (45 degrees). The climb rate is a component of the
/// velocity vector, so it is limited to this fraction of the speed; a steeper path would also make
/// the heading undefined at plus or minus 90 degrees of pitch.
const MAX_PATH_SINE: f64 = core::f64::consts::FRAC_1_SQRT_2;

/// Standard gravity, m/s^2 (the coordinated-turn rate is `g * tan(bank) / v`).
const G: f64 = 9.80665;

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
        require_positive("max_speed", params.max_speed.0)?;
        let min = params.min_speed.0;
        if !(min.is_finite() && (0.0..params.max_speed.0).contains(&min)) {
            return Err(MotionParamError::OutOfRange {
                name: "min_speed",
                value: min,
                expected: "finite, at least 0 and below max_speed",
            });
        }
        require_positive("max_accel", params.max_accel.0)?;
        require_positive("max_climb_rate", params.max_climb_rate.0)?;
        require_positive("max_descent_rate", params.max_descent_rate.0)?;
        let bank = params.max_bank.0;
        if !(bank.is_finite() && bank > 0.0 && bank < core::f64::consts::FRAC_PI_2) {
            return Err(MotionParamError::OutOfRange {
                name: "max_bank",
                value: bank,
                expected: "between 0 and 90 degrees (exclusive)",
            });
        }
        if let Some(cap) = params.max_turn_rate {
            require_positive("max_turn_rate", cap.0)?;
        }
        let agl = params.min_altitude_agl.0;
        if !(agl.is_finite() && agl >= 0.0) {
            return Err(MotionParamError::OutOfRange {
                name: "min_altitude_agl",
                value: agl,
                expected: "finite and at least 0",
            });
        }
        let id = MotionModelId::new(AIR_KINEMATIC_ID).map_err(|_| MotionParamError::InvalidId)?;
        Ok(Self { id, params })
    }

    /// The limits this model was built with.
    pub fn params(&self) -> AirKinematicParams {
        self.params
    }

    /// The bank-limited turn-rate numerator `g * tan(max_bank)`, m/s^2 (rate = this / speed).
    fn bank_accel(&self) -> f64 {
        G * math::tan(self.params.max_bank.0)
    }

    /// The turn rate at `speed`, rad/s: `g tan(bank) / max(v, 1)`, capped by `max_turn_rate`.
    fn turn_rate(&self, speed: f64) -> f64 {
        let rate = self.bank_accel() / speed.max(1.0);
        self.params
            .max_turn_rate
            .map_or(rate, |cap| rate.min(cap.0))
    }

    /// The turn radius at `speed`, meters (`v / turn_rate`; 0 at rest).
    fn turn_radius(&self, speed: f64) -> f64 {
        if speed <= 0.0 {
            0.0
        } else {
            speed / self.turn_rate(speed)
        }
    }

    /// The largest speed whose turn radius is at most `radius`: the inverse of
    /// [`Self::turn_radius`]. `v * max(v, 1) <= a * radius` for the bank and `v <= cap * radius`
    /// for the cap.
    fn speed_for_radius(&self, radius: f64) -> f64 {
        let bank = self.bank_accel() * radius;
        let by_bank = if bank <= 1.0 { bank } else { bank.sqrt() };
        self.params
            .max_turn_rate
            .map_or(by_bank, |cap| by_bank.min(cap.0 * radius))
    }

    /// Cruise speed for the reduced intents: `Hold` and no intent slow to `min_speed` (or to 0 for
    /// an aircraft that is not flying yet, below `min_speed`), `SetSpeed` is clamped to
    /// `[min_speed, max_speed]`, a `MoveTo` alone means `max_speed`.
    fn cruise(&self, s: &steer::Steering, speed: f64, dt: f64) -> f64 {
        let p = &self.params;
        // "Flying" is being at `min_speed` or within one acceleration step below it, so an
        // aircraft that slowed to `min_speed` (and rounds a hair under it) stays flying.
        let idle = if speed + p.max_accel.0 * dt >= p.min_speed.0 {
            p.min_speed.0
        } else {
            0.0
        };
        if s.hold {
            idle
        } else if let Some(v) = s.set_speed {
            v.clamp(p.min_speed.0, p.max_speed.0)
        } else if s.destination.is_some() {
            p.max_speed.0
        } else {
            idle
        }
    }

    /// Desired speed toward a destination: cruise, limited by the discrete stopping speed (only
    /// when the aircraft can stop) and by the turning circle through the destination, and never
    /// below `min_speed`.
    fn desired_toward(&self, cruise: f64, a: &steer::Approach, dt: f64) -> f64 {
        // Only an aircraft that can stop brakes for the destination; one with a minimum speed
        // flies over it at cruise speed (the turning-circle limit below still slows it).
        let mut v = if self.params.min_speed.0 == 0.0 {
            cruise.min(steer::stopping_speed(
                self.params.max_accel.0,
                dt,
                a.distance,
            ))
        } else {
            cruise
        };
        // The circle through the destination, tangent to the heading, has radius
        // `d / (2 sin e)`. Behind the aircraft (`|e| >= 90 degrees`) the aircraft must turn about
        // and that radius grows without bound toward `e = 180 degrees`, which would let it fly
        // away at full speed; the radius is then `d / 2`, the smallest circle that reaches it.
        let sin_e = if a.error.abs() >= core::f64::consts::FRAC_PI_2 {
            1.0
        } else {
            math::sin(a.error).abs()
        };
        if sin_e > 0.0 {
            v = v.min(self.speed_for_radius(a.distance / (2.0 * sin_e)));
        }
        v.max(self.params.min_speed.0)
    }
}

/// Where the height goes this step.
struct Vertical {
    /// Climb rate, m/s (negative: descent).
    rate: f64,
    /// The new ellipsoidal height, meters.
    height: f64,
}

impl AirKinematic {
    /// The climb rate toward `target` (the destination's height, else the current one), raised to
    /// the terrain floor at `ahead`, limited by the climb and descent rates and by the steepest
    /// flight-path angle (45 degrees of the speed).
    fn vertical(
        &self,
        height: f64,
        target: f64,
        ahead: Geodetic,
        speed: f64,
        dt: f64,
        terrain: &dyn TerrainProvider,
    ) -> Vertical {
        let p = &self.params;
        let ground = terrain.height_at(ahead).0;
        let floor = if ground.is_finite() {
            ground + p.min_altitude_agl.0
        } else {
            f64::NEG_INFINITY
        };
        let wanted = (target.max(floor) - height) / dt;
        let steepest = speed * MAX_PATH_SINE;
        let rate = wanted.clamp(
            -p.max_descent_rate.0.min(steepest),
            p.max_climb_rate.0.min(steepest),
        );
        Vertical {
            rate,
            height: height + rate * dt,
        }
    }
}

impl MotionModel for AirKinematic {
    fn id(&self) -> &MotionModelId {
        &self.id
    }

    /// One step of the air model.
    ///
    /// 1. Intents are reduced like the ground model's (`Hold` wins, the last of `MoveTo` and
    ///    `SetHeading` steers, the last `SetSpeed` wins).
    /// 2. Speed moves toward the desired speed (see the module doc) by at most `max_accel * dt`.
    /// 3. The heading moves toward the bearing (or the commanded heading) by at most
    ///    `turn_rate(new_speed) * dt`; while the destination is inside the smallest turning circle
    ///    (radius at `min_speed`, plus the margin of the sea model) the heading is held.
    /// 4. The height moves toward the destination's height (else stays) at the climb or descent
    ///    rate, but never below terrain (sampled one step ahead) plus `min_altitude_agl`; the
    ///    horizontal speed is what is left of the speed after the climb rate.
    /// 5. Arrival snap (only with `min_speed = 0`): the ground model's, horizontal; the aircraft
    ///    lands on the destination's position and stops (speed 0, so no climb in that step).
    /// 6. Pitch is the flight-path angle, roll the bank that gives the turn made in this step.
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
        let dt = dt.0;
        let here = state.position.to_geodetic();
        let frame = LocalFrame::at(here);
        let heading = frame.euler_from_orientation(state.orientation).heading.0;
        let speed = state.velocity.norm();
        let reduced = steer::reduce(intents);
        let cruise = self.cruise(&reduced, speed, dt);
        let toward = reduced
            .destination
            .map(|d| steer::approach(&frame, d, heading));
        let desired = toward
            .as_ref()
            .map_or(cruise, |a| self.desired_toward(cruise, a, dt));
        let accel_step = p.max_accel.0 * dt;
        let mut new_speed = (speed + (desired - speed).clamp(-accel_step, accel_step)).max(0.0);

        let margin = 2.0 * speed.max(new_speed) * dt;
        let floor_radius = self.turn_radius(p.min_speed.0);
        let steer_out = floor_radius > 0.0
            && toward
                .as_ref()
                .is_some_and(|a| steer::inside_turn_circle(a, floor_radius, margin));
        let turn_step = self.turn_rate(new_speed) * dt;
        let error = if steer_out {
            None
        } else {
            steer::turn_error(toward.as_ref(), reduced.heading, heading)
        };
        let new_heading = match error {
            Some(error) if new_speed > 0.0 => {
                math::wrap_pi(Radians(heading + error.clamp(-turn_step, turn_step))).0
            }
            _ => heading,
        };

        let (sin_h, cos_h) = math::sin_cos(new_heading);
        let target_height = reduced
            .destination
            .map_or(here.height().0, |d| d.to_geodetic().height().0);
        let ahead = frame
            .from_enu(Enu {
                east: new_speed * dt * sin_h,
                north: new_speed * dt * cos_h,
                up: 0.0,
            })
            .to_geodetic();
        let mut vertical = self.vertical(
            here.height().0,
            target_height,
            ahead,
            new_speed,
            dt,
            terrain,
        );
        let mut horizontal = (new_speed * new_speed - vertical.rate * vertical.rate)
            .max(0.0)
            .sqrt();

        let travel = horizontal * dt;
        let (east, north) = match &toward {
            Some(a)
                if p.min_speed.0 == 0.0
                    && a.distance <= travel.max(ARRIVAL_EPSILON_M)
                    && speed.max(new_speed) <= 2.0 * accel_step
                    && (a.error.abs() < core::f64::consts::FRAC_PI_2
                        || a.distance <= ARRIVAL_EPSILON_M) =>
            {
                new_speed = 0.0;
                horizontal = 0.0;
                vertical = Vertical {
                    rate: 0.0,
                    height: here.height().0,
                };
                (a.east, a.north)
            }
            _ => (travel * sin_h, travel * cos_h),
        };

        let turned = math::wrap_pi(Radians(new_heading - heading)).0;
        let motion = Motion {
            east,
            north,
            heading: new_heading,
            turned,
            speed: new_speed,
            horizontal,
            vertical,
        };
        motion.state_from(&frame, dt)
    }
}

/// The outcome of a step, before it is turned back into a [`KinematicState`].
struct Motion {
    /// Horizontal displacement in the old local frame, meters.
    east: f64,
    north: f64,
    /// The new heading and the heading change made, radians.
    heading: f64,
    turned: f64,
    /// Speed (norm of the velocity) and its horizontal part, m/s.
    speed: f64,
    horizontal: f64,
    vertical: Vertical,
}

impl Motion {
    fn state_from(&self, frame: &LocalFrame, dt: f64) -> KinematicState {
        let moved = frame
            .from_enu(Enu {
                east: self.east,
                north: self.north,
                up: 0.0,
            })
            .to_geodetic();
        // The height is finite (it comes from finite inputs and bounded rates); the fallback keeps
        // the moved point should that ever change.
        let placed = moved
            .with_height(Meters(self.vertical.height))
            .unwrap_or(moved);
        let new_frame = LocalFrame::at(placed);
        let (sin_h, cos_h) = math::sin_cos(self.heading);
        let bank = math::atan(self.turned / dt * self.speed.max(1.0) / G);
        KinematicState {
            position: placed.to_ecef(),
            velocity: new_frame.vector_from_enu(Enu {
                east: self.horizontal * sin_h,
                north: self.horizontal * cos_h,
                up: self.vertical.rate,
            }),
            orientation: new_frame.orientation_from_euler(Euler {
                heading: Radians(self.heading),
                pitch: Radians(math::atan2(self.vertical.rate, self.horizontal)),
                roll: Radians(bank),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::factory::located;

    fn model(cap: Option<f64>, bank_deg: f64) -> AirKinematic {
        AirKinematic::new(AirKinematicParams {
            max_speed: MetersPerSecond(200.0),
            min_speed: MetersPerSecond(0.0),
            max_accel: MetersPerSecondSquared(5.0),
            max_climb_rate: MetersPerSecond(15.0),
            max_descent_rate: MetersPerSecond(10.0),
            max_bank: Radians::from_degrees(bank_deg),
            max_turn_rate: cap.map(RadiansPerSecond::from_degrees_per_second),
            min_altitude_agl: Meters(0.0),
        })
        .expect("valid params")
    }

    /// `speed_for_radius` is the inverse of `turn_radius`: the radius at the returned speed is the
    /// requested one (below the cap and the bank limit, and in the `v < 1 m/s` branch).
    #[test]
    fn speed_for_radius_inverts_turn_radius() {
        for m in [
            model(None, 30.0),
            model(Some(2.0), 30.0),
            model(Some(0.05), 60.0),
        ] {
            for radius in [0.001, 0.05, 0.3, 2.0, 40.0, 900.0, 20_000.0] {
                let v = m.speed_for_radius(radius);
                let back = m.turn_radius(v);
                assert!(
                    back <= radius * (1.0 + 1e-9),
                    "r {radius}: v {v} gives {back}"
                );
                // and it is the largest such speed: a bit faster exceeds the radius
                let over = m.turn_radius(v * 1.001);
                assert!(over > radius, "r {radius}: v {v} is not the largest speed");
            }
        }
    }

    /// The turn rate caps at `max_turn_rate`, and is `g tan(bank) / max(v, 1)` otherwise.
    #[test]
    fn turn_rate_is_the_bank_rate_capped_and_floored_at_one_meter_per_second() {
        let capped = model(Some(1.0), 30.0);
        let cap = 1.0_f64.to_radians();
        assert!((capped.turn_rate(100.0) - cap).abs() < 1e-12);
        let free = model(None, 30.0);
        let rate = G * math::tan(30.0_f64.to_radians());
        assert!((free.turn_rate(100.0) - rate / 100.0).abs() < 1e-12);
        assert!((free.turn_rate(0.2) - rate).abs() < 1e-12);
        assert_eq!(free.turn_radius(0.0), 0.0);
    }

    /// The factory's key mapping covers `OutOfRange` (and shows the bank in degrees).
    #[test]
    fn located_points_an_out_of_range_error_at_its_key() {
        let error = located(&MotionParamError::OutOfRange {
            name: "max_bank",
            value: core::f64::consts::FRAC_PI_2,
            expected: "between 0 and 90 degrees (exclusive)",
        });

        assert_eq!(error.param.as_deref(), Some("/max_bank_deg"));
        assert!(error.to_string().contains("got 90"), "{error}");
        let unknown = located(&MotionParamError::OutOfRange {
            name: "nope",
            value: 1.0,
            expected: "x",
        });
        assert_eq!(unknown.param, None);
    }
}
