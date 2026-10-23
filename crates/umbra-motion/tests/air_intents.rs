//! AT-13 (M7, WI-6 rows): arrival and the per-step contract of `builtin.air.kinematic` beyond the
//! WI-1 rows. Requirements: REQ-M-2, REQ-M-1, ADR 0051, ASM-607..609, ASM-699..ASM-705.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod common;

use core::f64::consts::{PI, TAU};

use common::{
    DT, angle_delta, climb_rate_of, dest, euler_of, flat, heading_of, height_of, move_to, speed_of,
    start,
};
use umbra_core::{
    Intent, LocalFrame, Meters, MetersPerSecond, MetersPerSecondSquared, MotionModel, Radians,
    Seconds,
};
use umbra_motion::{AirKinematic, AirKinematicParams};

const G: f64 = 9.80665;

/// The climb and descent rate limits of every rig, m/s.
const CLIMB: f64 = 15.0;
const DESCENT: f64 = 10.0;

/// The height of the start states and of the level destinations of the arrival rows (non-level
/// destinations are in `air_vertical.rs` and the sweep of `air_prop.rs`).
const LEVEL_M: f64 = 2000.0;

/// The height error of the destinations of the arrival grid (they are this far above the start).
const HEIGHT_ERROR_M: f64 = 120.0;

/// Steps an aircraft that can hover keeps checking after it has landed: it must stay landed.
const HOLD_STEPS: usize = 30;

/// A parameter set for the arrival tests.
#[derive(Clone, Copy)]
struct Rig {
    max_speed: f64,
    min_speed: f64,
    max_accel: f64,
    bank_deg: f64,
    dt: f64,
}

/// 30..100 m/s, 5 m/s^2, 45 degrees of bank (radius 92 m at 30 m/s), 30 Hz.
const DEFAULT: Rig = Rig {
    max_speed: 100.0,
    min_speed: 30.0,
    max_accel: 5.0,
    bank_deg: 45.0,
    dt: 1.0 / 30.0,
};
/// High acceleration (50 m/s^2).
const HIGH_ACCEL: Rig = Rig {
    max_accel: 50.0,
    ..DEFAULT
};
/// A coarse step: 0.5 s, a step of up to 50 m (below the 92 m radius).
const COARSE_DT: Rig = Rig { dt: 0.5, ..DEFAULT };
/// A step longer than the smallest turn radius (60 m/s * 1 s against 6 m); schema-valid.
const STEP_OVER_RADIUS: Rig = Rig {
    max_speed: 60.0,
    min_speed: 10.0,
    max_accel: 5.0,
    bank_deg: 60.0,
    dt: 1.0,
};
/// An aircraft that can stop (`min_speed` 0).
const STOPPER: Rig = Rig {
    min_speed: 0.0,
    ..DEFAULT
};
/// A stopper with a coarse step.
const STOPPER_COARSE: Rig = Rig {
    min_speed: 0.0,
    dt: 0.5,
    ..DEFAULT
};
/// A weak engine (0.05 m/s^2) that can stop: a step's travel is far below a millimeter at rest.
const WEAK_STOPPER: Rig = Rig {
    min_speed: 0.0,
    max_accel: 0.05,
    ..DEFAULT
};

impl Rig {
    fn model(self) -> AirKinematic {
        AirKinematic::new(AirKinematicParams {
            max_speed: MetersPerSecond(self.max_speed),
            min_speed: MetersPerSecond(self.min_speed),
            max_accel: MetersPerSecondSquared(self.max_accel),
            max_climb_rate: MetersPerSecond(CLIMB),
            max_descent_rate: MetersPerSecond(DESCENT),
            max_bank: Radians(self.bank_deg.to_radians()),
            max_turn_rate: None,
            min_altitude_agl: Meters(0.0),
        })
        .expect("valid params")
    }

    /// The controlled speed: the horizontal speed of an aircraft that can hover (ASM-707), the
    /// norm of the velocity otherwise.
    fn speed(self, s: &umbra_core::KinematicState) -> f64 {
        if self.min_speed > 0.0 {
            return speed_of(s);
        }
        let v = LocalFrame::at(s.position.to_geodetic()).vector_to_enu(s.velocity);
        umbra_core::math::hypot(v.east, v.north)
    }

    /// The turn radius at `speed`: `v * max(v, 1) / (g tan bank)`.
    fn radius(self, speed: f64) -> f64 {
        speed * speed.max(1.0) / (G * umbra_core::math::tan(self.bank_deg.to_radians()))
    }

    /// Whether the path-length bound applies (a step no longer than the smallest turn radius).
    fn has_path_bound(self) -> bool {
        self.max_speed * self.dt <= self.radius(self.min_speed)
    }

    /// Path bound (ASM-699) for a destination at `d` meters from a start at `v0` m/s.
    ///
    /// Speed can reach at most `v_ref = min(max_speed, sqrt(v0^2 + 2 a d))` (and is at least
    /// `max(min_speed, v0)` while the aircraft turns), so every turn has a radius at most
    /// `R = radius(v_ref)`. The aircraft holds its heading at most `d + R + m` (`m = 2 max_speed
    /// dt`), turns at most a full circle (`2 pi R`) and then runs at most the tangent length
    /// `R + m`; slowing from `v0` costs at most `v0^2 / a` out and back (the stopping overshoot);
    /// `4 max_speed dt` covers the final step and the capture distance.
    fn path_bound(self, v0: f64, d: f64) -> f64 {
        let v_ref = (v0 * v0 + 2.0 * self.max_accel * d)
            .sqrt()
            .min(self.max_speed)
            .max(self.min_speed)
            .max(v0);
        let r = self.radius(v_ref);
        d + (TAU + 2.0) * r + v0 * v0 / self.max_accel + 4.0 * self.max_speed * self.dt
    }

    /// Arrival bound in steps: twice (stated headroom) the path at the slowest sustained speed
    /// (`min_speed`, or half of `max_speed` for an aircraft that can stop), the speed ramps and the
    /// height change `dh` (signed meters) at the rate limit with its ramp.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a positive finite step count"
    )]
    fn step_bound(self, v0: f64, d: f64, dh: f64) -> usize {
        let slow = if self.min_speed > 0.0 {
            self.min_speed
        } else {
            0.5 * self.max_speed
        };
        // plus the height change at the slower of the two rate limits and the vertical ramp and
        // stop (`2 * rate / a`)
        let rate = CLIMB.min(DESCENT);
        let vertical = dh.abs() / rate + 2.0 * rate / self.max_accel;
        let seconds = 2.0
            * (self.path_bound(v0, d) / slow + 2.0 * self.max_speed / self.max_accel + vertical);
        (seconds / self.dt).ceil() as usize
    }

    /// The capture distance: one step at `max_speed`. A model that flies straight at the
    /// destination passes within it; one that can stop (`min_speed` 0) must stop on it instead.
    fn capture(self) -> f64 {
        self.max_speed * self.dt
    }
}

/// Runs `rig` from `v0` toward `target` (east, north, up) and returns the steps until arrival and
/// the path length, or `None` within `limit` steps. Arrival: within `capture` for an aircraft that
/// cannot stop, at rest within 1 cm for one that can. Checks the per-step limits on the way.
fn steps_to_arrive(
    rig: Rig,
    v0: f64,
    target: (f64, f64, f64),
    limit: usize,
) -> Option<(usize, f64)> {
    let model = rig.model();
    let dt = Seconds(rig.dt);
    let intents = [move_to(target.0, target.1, target.2)];
    let goal = dest(target.0, target.1, target.2);
    let mut s = start(0.0, v0, LEVEL_M);
    let mut path = 0.0;
    for i in 0..limit {
        let next = model.step(&s, &intents, dt, &flat());
        path += (next.position - s.position).norm();
        let (a, b) = (rig.speed(&s), rig.speed(&next));
        let landed = b < 1e-9;
        // the landing step may change the speed by twice the limit (ASM-701)
        let limit = rig.max_accel * rig.dt * if landed { 2.0 } else { 1.0 };
        assert!((b - a).abs() <= limit + 1e-6, "step {i}: speed {a} -> {b}");
        let turned = angle_delta(heading_of(&next), heading_of(&s)).abs();
        // the turn made in a step is at most the rate at the new speed
        let allowed = G * umbra_core::math::tan(rig.bank_deg.to_radians()) / b.max(1.0) * rig.dt;
        assert!(
            landed || turned <= allowed * (1.0 + 1e-9) + 1e-9,
            "step {i}: turned {turned} > {allowed} (target {target:?}, v0 {v0}, speed {b}, rig dt {})",
            rig.dt
        );
        s = next;
        let off = LocalFrame::at(s.position.to_geodetic()).to_enu(goal);
        let miss = umbra_core::math::hypot(off.east, off.north);
        if rig.min_speed > 0.0 {
            if miss <= rig.capture() {
                return Some((i, path));
            }
        } else if on_destination(&s, miss, goal) {
            // it must stay landed (no creep, no limit cycle)
            for _ in 0..HOLD_STEPS {
                s = model.step(&s, &intents, dt, &flat());
                let off = LocalFrame::at(s.position.to_geodetic()).to_enu(goal);
                let miss = umbra_core::math::hypot(off.east, off.north);
                assert!(
                    on_destination(&s, miss, goal),
                    "step {i}: left the destination after landing ({miss} m)"
                );
            }
            return Some((i, path));
        }
    }
    None
}

/// On the destination at rest: within 1 cm horizontally, 1 mm in height, no motion (hover rows).
fn on_destination(s: &umbra_core::KinematicState, miss: f64, goal: umbra_core::Ecef) -> bool {
    let v = LocalFrame::at(s.position.to_geodetic()).vector_to_enu(s.velocity);
    miss < 1e-2
        && umbra_core::math::hypot(v.east, v.north) < 1e-9
        && v.up.abs() < 1e-9
        && (s.position.to_geodetic().height().0 - goal.to_geodetic().height().0).abs() < 1e-3
}

/// Arrives within the derived bounds; returns the steps taken.
fn assert_arrives(rig: Rig, v0: f64, target: (f64, f64), d_max: f64) -> usize {
    let bound = rig.step_bound(v0, d_max, 0.0);
    let (steps, path) = steps_to_arrive(rig, v0, (target.0, target.1, LEVEL_M), bound)
        .unwrap_or_else(|| panic!("{target:?} from {v0} m/s: no arrival in {bound} steps"));
    if rig.has_path_bound() {
        let limit = rig.path_bound(v0, d_max);
        assert!(
            path <= limit,
            "{target:?}: path {path} m > {limit} m (re-orbit)"
        );
    }
    steps
}

/// ASM-699: a destination inside the smallest turning circle (92 m radius at 30 m/s), abeam at
/// speeds from the minimum to the maximum, on both sides, is flown over.
#[test]
fn given_a_destination_inside_the_smallest_turning_circle_when_stepped_then_it_is_flown_over() {
    for east in [60.0, -60.0, 150.0, -150.0] {
        for v0 in [30.0, 65.0, 100.0] {
            assert_arrives(DEFAULT, v0, (east, 0.0), 150.0);
        }
    }
}

/// ASM-699: directly behind, near and far, and a 1 m destination.
#[test]
fn given_a_destination_directly_behind_when_stepped_then_it_is_flown_over() {
    for behind in [1.0, 5.0, 40.0, 200.0, 1000.0] {
        assert_arrives(DEFAULT, 100.0, (0.0, -behind), 1000.0);
    }
}

/// ASM-699: a grid of speeds, distances and bearings arrives for every parameter set: high
/// acceleration, a coarse step, a step longer than the turn radius (arrival only; there is no
/// circle to bound the path by), and aircraft that can stop (landing at rest on the destination).
#[test]
fn given_destinations_around_the_aircraft_when_stepped_then_every_one_arrives() {
    let rows = [
        DEFAULT,
        HIGH_ACCEL,
        COARSE_DT,
        STEP_OVER_RADIUS,
        STOPPER,
        STOPPER_COARSE,
    ];
    for rig in rows {
        let r_min = rig.radius(rig.min_speed).max(1.0);
        let r_max = rig.radius(rig.max_speed);
        let distances: Vec<f64> = [0.01, 0.1, 0.5, 0.95, 1.0, 1.05, 2.0, 2.5, 6.0]
            .iter()
            .map(|k| k * r_min)
            .chain([0.1 * r_max, 2.0 * r_max])
            .collect();
        let d_max = distances.iter().copied().fold(0.0, f64::max);
        let starts = if rig.min_speed > 0.0 {
            [
                rig.min_speed,
                f64::midpoint(rig.min_speed, rig.max_speed),
                rig.max_speed,
            ]
        } else {
            [0.0, 0.4 * rig.max_speed, rig.max_speed]
        };
        for v0 in starts {
            for d in &distances {
                for k in 0..12 {
                    let bearing = f64::from(k) * TAU / 12.0;
                    let (sin, cos) = umbra_core::math::sin_cos(bearing);
                    let steps = rig.step_bound(v0, d_max, HEIGHT_ERROR_M);
                    let arrived = steps_to_arrive(
                        rig,
                        v0,
                        (d * sin, d * cos, LEVEL_M + HEIGHT_ERROR_M),
                        steps,
                    );
                    assert!(
                        arrived.is_some(),
                        "R_min {r_min:.1} dt {:.3} v0 {v0} d {d:.1} bearing {bearing:.3}",
                        rig.dt
                    );
                    if rig.has_path_bound() {
                        assert_arrives(rig, v0, (d * sin, d * cos), d_max);
                    }
                }
            }
        }
    }
}

/// ASM-699: the destination 0.9 m ahead while flying at maximum speed is passed in one step and
/// the aircraft keeps flying (an aircraft with a minimum speed never stops on a destination).
#[test]
fn given_a_minimum_speed_when_over_the_destination_then_the_aircraft_keeps_flying() {
    let model = DEFAULT.model();
    let mut s = start(0.0, 100.0, 2000.0);
    // long enough for the speed to fall from 100 to the minimum if nothing held it up
    for _ in 0..600 {
        s = model.step(&s, &[move_to(0.0, 0.0, 0.0)], DT, &flat());
        assert!(speed_of(&s) >= 30.0 - 1e-6, "speed {}", speed_of(&s));
    }
}

/// ASM-699 (behind): a destination 5 m behind a fast aircraft is not a reason to accelerate; the
/// turn about is made at the slowest speed, not at a speed whose circle is kilometers wide.
#[test]
fn given_a_destination_just_behind_when_stepped_then_the_aircraft_does_not_speed_up() {
    let model = DEFAULT.model();
    let mut s = start(0.0, 40.0, 2000.0);
    for _ in 0..30 {
        s = model.step(&s, &[move_to(0.0, -5.0, 0.0)], DT, &flat());
    }
    assert!(speed_of(&s) <= 40.0 + 1e-6, "speed {}", speed_of(&s));
}

/// ASM-608: a parked aircraft (speed 0) with `Hold` or no intent stays parked; one that is flying
/// slows to `min_speed` and then keeps it, however the last step rounds.
#[test]
fn given_hold_when_parked_or_flying_then_parked_stays_and_flying_holds_min_speed() {
    let model = DEFAULT.model();
    let parked = model.step(&start(0.0, 0.0, 2000.0), &[Intent::Hold], DT, &flat());
    assert!(speed_of(&parked) == 0.0, "parked: {}", speed_of(&parked));
    let mut s = start(0.0, 31.0, 2000.0);
    for _ in 0..600 {
        s = model.step(&s, &[], DT, &flat());
        assert!(speed_of(&s) >= 30.0 - 1e-9, "speed {}", speed_of(&s));
    }
    assert!((speed_of(&s) - 30.0).abs() < 1e-9);
}

/// ASM-699: a start slightly under `min_speed` (accelerating up) is still flying with `Hold`.
#[test]
fn given_a_speed_just_under_the_minimum_when_holding_then_it_climbs_back_to_the_minimum() {
    let model = DEFAULT.model();

    let s = model.step(&start(0.0, 29.99, 2000.0), &[Intent::Hold], DT, &flat());

    assert!(speed_of(&s) > 29.99, "speed {}", speed_of(&s));
}

/// ASM-608: banking left is a negative roll, right a positive one, and the roll is zero when the
/// heading does not change.
#[test]
fn given_turns_both_ways_when_stepped_then_the_roll_follows_the_turn() {
    let model = DEFAULT.model();
    let from = start(0.0, 60.0, 2000.0);

    let right = model.step(&from, &[Intent::SetHeading(Radians(1.0))], DT, &flat());
    let left = model.step(&from, &[Intent::SetHeading(Radians(-1.0))], DT, &flat());
    let level = model.step(&from, &[Intent::SetHeading(Radians(0.0))], DT, &flat());

    assert!(euler_of(&right).roll.0 > 0.1);
    assert!(euler_of(&left).roll.0 < -0.1);
    assert!(euler_of(&level).roll.0.abs() < 1e-12);
}

/// ASM-608: a descent has negative pitch equal to the flight-path angle.
#[test]
fn given_a_descent_when_stepped_then_pitch_is_the_negative_flight_path_angle() {
    let model = DEFAULT.model();
    let from = start(0.0, 60.0, 2000.0);

    let s = model.step(&from, &[move_to(0.0, 5000.0, 0.0)], DT, &flat());

    let rate = climb_rate_of(&s);
    assert!((rate + 10.0).abs() < 1e-6, "descent {rate}");
    let horizontal = (speed_of(&s) * speed_of(&s) - rate * rate).sqrt();
    let gamma = umbra_core::math::atan2(rate, horizontal);
    assert!((euler_of(&s).pitch.0 - gamma).abs() < 1e-9);
    assert!((speed_of(&s) - (60.0 + 5.0 / 30.0)).abs() < 1e-9);
}

// Arrival-snap guards (min_speed = 0): each single-condition mutant must die.

/// Speed guard: at 100 m/s a destination 0.2 m ahead is within a step's travel but the aircraft
/// cannot stop dead; it brakes at the limit.
#[test]
fn given_a_close_destination_ahead_at_full_speed_when_stepped_then_it_brakes_at_the_limit() {
    let from = start(0.0, 100.0, 2000.0);

    let next = STOPPER
        .model()
        .step(&from, &[move_to(0.0, 0.2, 0.0)], DT, &flat());

    assert!(
        speed_of(&next) >= 100.0 - 5.0 / 30.0 - 1e-6,
        "speed fell to {}",
        speed_of(&next)
    );
}

/// Direction guard: inside a step's travel but 3 mm behind a slow aircraft, the destination is not
/// landed on (that would be a reversal): the aircraft keeps its heading and does not go backwards.
#[test]
fn given_a_destination_just_behind_a_slow_aircraft_when_stepped_then_it_does_not_reverse() {
    let from = start(0.0, 0.3, 2000.0);

    let next = STOPPER
        .model()
        .step(&from, &[move_to(0.0, -0.003, 0.0)], DT, &flat());

    let along = LocalFrame::at(from.position.to_geodetic())
        .to_enu(next.position)
        .north;
    assert!(along >= 0.0, "moved {along} m along the heading");
    assert!(speed_of(&next) > 0.0, "it landed behind itself");
}

/// Minimum-speed guard: with a minimum speed of 0.1 m/s (below `2 * max_accel * dt`) the aircraft
/// never lands: 0.5 mm ahead it flies on at the minimum speed.
#[test]
fn given_a_tiny_minimum_speed_when_over_the_destination_then_it_does_not_land() {
    let rig = Rig {
        min_speed: 0.1,
        ..DEFAULT
    };
    let from = start(0.0, 0.1, 2000.0);

    let next = rig
        .model()
        .step(&from, &[move_to(0.0, 0.0005, 0.0)], DT, &flat());

    assert!(speed_of(&next) >= 0.1 - 1e-12, "speed {}", speed_of(&next));
}

/// Floor guard: within 1 mm the snap applies at rest in any direction, so an aircraft lands in the
/// first step on a destination 0.9 mm behind it (a step's travel is far below that here).
#[test]
fn given_a_destination_within_a_millimeter_behind_an_aircraft_at_rest_when_stepped_then_it_lands() {
    let rig = WEAK_STOPPER;
    let from = start(0.0, 0.0, 2000.0);
    let goal = dest(0.0, -0.0009, LEVEL_M);

    let next = rig.model().step(
        &from,
        &[move_to(0.0, -0.0009, LEVEL_M)],
        Seconds(rig.dt),
        &flat(),
    );

    let off = LocalFrame::at(next.position.to_geodetic()).to_enu(goal);
    assert!(
        umbra_core::math::hypot(off.east, off.north) < 1e-6,
        "{off:?}"
    );
    assert!(rig.speed(&next) < 1e-12);
}

/// Per-step contract: a slow pass beside the destination neither moves the aircraft sideways nor
/// drops more speed than the limit, and it does not land (nothing captures a pass at a distance).
#[test]
fn given_a_slow_pass_beside_the_destination_when_stepped_then_the_step_obeys_the_limits() {
    let rig = WEAK_STOPPER;
    let from = start(0.0, 0.0033, 2000.0);

    let next = rig.model().step(
        &from,
        &[move_to(0.006, 0.003, LEVEL_M)],
        Seconds(rig.dt),
        &flat(),
    );

    let accel = (speed_of(&next) - speed_of(&from)).abs() / rig.dt;
    assert!(
        accel <= rig.max_accel + 1e-6,
        "speed changed at {accel} m/s^2"
    );
    let moved = (next.position - from.position).norm();
    let allowed = speed_of(&from).max(speed_of(&next)) * rig.dt + 1e-3;
    assert!(moved <= allowed, "moved {moved} m, at most {allowed}");
    let _ = PI;
}

/// ASM-700: an aircraft that can only fly over a destination does not brake for it: 500 m ahead at
/// 100 m/s it keeps its speed until it is over it.
#[test]
fn given_a_destination_ahead_when_the_aircraft_has_a_minimum_speed_then_it_does_not_brake() {
    let model = DEFAULT.model();
    let mut s = start(0.0, 100.0, 2000.0);
    for _ in 0..120 {
        s = model.step(&s, &[move_to(0.0, 500.0, 0.0)], DT, &flat());
        assert!(speed_of(&s) >= 100.0 - 1e-9, "braked to {}", speed_of(&s));
    }
}

/// ASM-702: the floor is sampled where the step ends up (one step ahead), so on a 5% slope an
/// aircraft that was exactly on the floor is on or above the floor of its new position.
#[test]
fn given_an_aircraft_on_the_floor_of_a_slope_when_stepped_then_it_is_above_the_new_floor() {
    let terrain = common::SlopedTerrain { slope: 0.05 };
    let model = DEFAULT.model();
    let floor = |s: &umbra_core::KinematicState| {
        umbra_core::TerrainProvider::height_at(&terrain, s.position.to_geodetic()).0
    };
    let mut s = start(0.0, 100.0, 0.0);
    // put it on the floor of its own position
    let g = s.position.to_geodetic();
    let on_floor = g
        .with_height(umbra_core::TerrainProvider::height_at(&terrain, g))
        .expect("finite");
    s.position = on_floor.to_ecef();

    for i in 0..60 {
        s = model.step(&s, &[Intent::SetHeading(Radians(0.0))], DT, &terrain);
        assert!(
            height_of(&s) >= floor(&s) - 1e-3,
            "step {i}: {} m under the floor",
            floor(&s) - height_of(&s)
        );
    }
}

/// ASM-608: below 1 m/s the turn rate is the rate at 1 m/s, and the roll the bank that gives it:
/// a saturated turn at 0.5 m/s rolls by the full bank (30 degrees).
#[test]
fn given_a_saturated_turn_below_one_mps_when_stepped_then_the_roll_is_the_bank_limit() {
    let rig = Rig {
        min_speed: 0.0,
        bank_deg: 30.0,
        ..DEFAULT
    };
    let from = start(0.0, 0.5, 2000.0);

    let next = rig.model().step(
        &from,
        &[
            Intent::SetHeading(Radians(3.0)),
            Intent::SetSpeed(MetersPerSecond(0.5)),
        ],
        DT,
        &flat(),
    );

    // `SetSpeed(0.5)` keeps the speed at 0.5 m/s, below 1 m/s
    assert!(speed_of(&next) < 1.0);
    assert!((euler_of(&next).roll.0 - 30.0_f64.to_radians()).abs() < 1e-6);
}

/// A terrain that rises 5% toward the east, for the floor lookahead along the heading.
struct EastSlope;

impl umbra_core::TerrainProvider for EastSlope {
    fn height_at(&self, at: umbra_core::Geodetic) -> Meters {
        let east_m = (at.lon().0 - common::origin().lon().0)
            * 6_367_000.0
            * umbra_core::math::cos(common::origin().lat().0);
        Meters(0.05 * east_m)
    }

    fn surface_normal_at(&self, _at: umbra_core::Geodetic) -> umbra_core::Enu {
        umbra_core::Enu {
            east: 0.0,
            north: 0.0,
            up: 1.0,
        }
    }
}

/// ASM-702: the floor lookahead follows the heading (east here, not only north).
#[test]
fn given_an_aircraft_on_the_floor_of_an_east_slope_when_stepped_then_it_is_above_the_new_floor() {
    let model = DEFAULT.model();
    let mut s = start(core::f64::consts::FRAC_PI_2, 100.0, 0.0);
    let g = s.position.to_geodetic();
    s.position = g
        .with_height(umbra_core::TerrainProvider::height_at(&EastSlope, g))
        .expect("finite")
        .to_ecef();

    for i in 0..60 {
        s = model.step(
            &s,
            &[Intent::SetHeading(Radians(core::f64::consts::FRAC_PI_2))],
            DT,
            &EastSlope,
        );
        let floor = umbra_core::TerrainProvider::height_at(&EastSlope, s.position.to_geodetic()).0;
        assert!(
            height_of(&s) >= floor - 1e-3,
            "step {i}: {} m under",
            floor - height_of(&s)
        );
    }
}

/// ASM-700: the turning-circle limit never takes the speed below `min_speed`: a destination 3 m
/// abeam asks for a tiny circle, and the aircraft settles at the minimum speed, not under it.
#[test]
fn given_a_destination_abeam_when_the_circle_limit_is_tiny_then_speed_stops_at_the_minimum() {
    let from = start(0.0, 30.05, 2000.0);

    let next = DEFAULT
        .model()
        .step(&from, &[move_to(3.0, 0.0, 0.0)], DT, &flat());

    assert!(
        (speed_of(&next) - 30.0).abs() < 1e-9,
        "speed {}",
        speed_of(&next)
    );
}
