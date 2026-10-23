//! AT-14 (M7, WI-5 rows): intent reduction and edge cases of `builtin.sea.kinematic`, beyond the
//! WI-1 rows. Requirements: REQ-M-2, REQ-M-1, ADR 0016, ADR 0051, ASM-030, ASM-048, ASM-694.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod common;

use core::f64::consts::FRAC_PI_2;

use common::{DT, angle_delta, dest, flat, heading_of, move_to, run, speed_of, start};
use umbra_core::{
    Ecef, Intent, KinematicState, LocalFrame, Meters, MetersPerSecond, MetersPerSecondSquared,
    MotionModel, Radians, Seconds,
};
use umbra_motion::{SeaKinematic, SeaKinematicParams};

fn model() -> SeaKinematic {
    SeaKinematic::new(SeaKinematicParams {
        max_speed: MetersPerSecond(10.0),
        max_accel: MetersPerSecondSquared(1.0),
        min_turn_radius: Meters(50.0),
        surface_height: Meters(2.0),
    })
    .expect("valid params")
}

/// ADR 0051: a `SetSpeed` above `max_speed` is clamped, not followed.
#[test]
fn given_a_speed_request_above_max_when_stepped_then_speed_stops_at_max() {
    let from = start(0.0, 9.9, 2.0);

    let states = run(
        &model(),
        &flat(),
        from,
        &[Intent::SetSpeed(MetersPerSecond(50.0))],
        60,
    );

    for s in &states {
        assert!(speed_of(s) <= 10.0 + 1e-9, "{}", speed_of(s));
    }
    assert!((speed_of(&states[59]) - 10.0).abs() < 1e-6);
}

/// ASM-048: a negative or non-finite speed request means stop; the vessel never reverses.
#[test]
fn given_a_negative_or_non_finite_speed_request_when_stepped_then_it_stops_without_reversing() {
    for request in [-5.0, f64::NAN, f64::INFINITY] {
        let from = start(0.0, 3.0, 2.0);

        let states = run(
            &model(),
            &flat(),
            from,
            &[Intent::SetSpeed(MetersPerSecond(request))],
            120,
        );

        assert!(speed_of(&states[119]) < 1e-9, "{request}");
        for s in &states {
            // heading stays north: velocity never points south
            assert!(angle_delta(heading_of(s), 0.0).abs() < 1e-9, "{request}");
        }
    }
}

/// ASM-030: `Hold` wins over a `MoveTo` and `SetSpeed`.
#[test]
fn given_hold_with_a_destination_and_speed_when_stepped_then_the_vessel_still_stops() {
    let intents = [
        move_to(0.0, 1000.0, 0.0),
        Intent::SetSpeed(MetersPerSecond(10.0)),
        Intent::Hold,
    ];

    let states = run(&model(), &flat(), start(0.0, 5.0, 2.0), &intents, 200);

    assert!(speed_of(&states[199]) < 1e-9);
}

/// ADR 0016: the last of `MoveTo` and `SetHeading` steers.
#[test]
fn given_a_move_to_then_a_set_heading_when_stepped_then_the_heading_command_wins() {
    let intents = [
        move_to(1000.0, 0.0, 0.0),
        Intent::SetHeading(Radians(-FRAC_PI_2)),
        Intent::SetSpeed(MetersPerSecond(10.0)),
    ];

    let states = run(&model(), &flat(), start(0.0, 10.0, 2.0), &intents, 300);

    assert!(angle_delta(heading_of(&states[299]), -FRAC_PI_2).abs() < 1e-6);
}

/// ASM-030: a non-finite destination or heading is ignored (the vessel keeps its heading and,
/// with no other intent, slows to a stop).
#[test]
fn given_non_finite_steering_intents_when_stepped_then_they_are_ignored() {
    let nan_dest = Intent::MoveTo {
        destination: Ecef(umbra_core::Vec3::new(f64::NAN, 0.0, 0.0)),
    };
    for bad in [nan_dest, Intent::SetHeading(Radians(f64::NAN))] {
        let s = model().step(&start(0.3, 5.0, 2.0), &[bad], DT, &flat());

        assert!(angle_delta(heading_of(&s), 0.3).abs() < 1e-9);
        assert!(speed_of(&s).is_finite());
    }
}

/// ASM-694: a destination ahead is reached: the vessel lands on it and stops.
#[test]
fn given_a_destination_ahead_when_stepped_long_enough_then_it_lands_on_it_and_stops() {
    let states = run(
        &model(),
        &flat(),
        start(0.0, 0.0, 2.0),
        &[move_to(0.0, 100.0, 0.0)],
        2000,
    );

    let last = states[1999];
    let target = dest(0.0, 100.0, 0.0);
    // horizontal distance: the destination height is ignored (2 m surface vs 0 m)
    let to = umbra_core::LocalFrame::at(last.position.to_geodetic()).to_enu(target);
    assert!(umbra_core::math::hypot(to.east, to.north) < 1e-3, "{to:?}");
    assert!(speed_of(&last) < 1e-9);
}

/// REQ-M-1: a non-positive or non-finite `dt` returns the state unchanged.
#[test]
fn given_a_bad_dt_when_stepped_then_the_state_is_unchanged() {
    let from = start(0.3, 5.0, 40.0);
    for dt in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert_eq!(model().step(&from, &[], Seconds(dt), &flat()), from, "{dt}");
    }
}

/// A parameter set for the arrival tests (surface height 2 m), with its step.
#[derive(Clone, Copy)]
struct Rig {
    max_speed: f64,
    max_accel: f64,
    radius: f64,
    dt: f64,
}

/// The default model at 30 Hz.
const DEFAULT: Rig = Rig {
    max_speed: 10.0,
    max_accel: 1.0,
    radius: 50.0,
    dt: 1.0 / 30.0,
};
/// High acceleration (10 m/s^2): the stopping speed is high until the very end.
const HIGH_ACCEL: Rig = Rig {
    max_accel: 10.0,
    ..DEFAULT
};
/// A coarse step: 0.1 s, a step of up to 1 m.
const COARSE_DT: Rig = Rig { dt: 0.1, ..DEFAULT };
/// A step longer than the turn radius (`max_speed * dt` = 15 m > 10 m); schema-valid.
const STEP_OVER_RADIUS: Rig = Rig {
    max_speed: 15.0,
    max_accel: 3.0,
    radius: 10.0,
    dt: 1.0,
};

impl Rig {
    fn model(self) -> SeaKinematic {
        SeaKinematic::new(SeaKinematicParams {
            max_speed: MetersPerSecond(self.max_speed),
            max_accel: MetersPerSecondSquared(self.max_accel),
            min_turn_radius: Meters(self.radius),
            surface_height: Meters(2.0),
        })
        .expect("valid params")
    }

    /// Path bound (ASM-694) for destinations up to `d_max` meters away.
    ///
    /// The vessel holds its heading at most `d_max + R + m` (the destination ahead is passed,
    /// then it leaves the margin circle; `m = 2 * max_speed * dt`), turns at most a full circle
    /// (`2 pi R`), and then runs at most the tangent length `R + m` (after a hold) or `d_max + R`
    /// (no hold): `d_max + 2 pi R + 2 R + 2 m`. A vessel that cannot stop within the distance
    /// overshoots by its stopping distance `max_speed^2 / (2 max_accel)` and comes back the same
    /// way: `+ max_speed^2 / max_accel`. Valid when `max_speed * dt <= R`.
    fn path_bound(self, d_max: f64) -> f64 {
        let m = 2.0 * self.max_speed * self.dt;
        d_max
            + core::f64::consts::TAU * self.radius
            + 2.0 * self.radius
            + 2.0 * m
            + self.max_speed * self.max_speed / self.max_accel
    }

    /// Arrival bound in steps: the path at `max_speed`, plus `2 * max_speed / max_accel` for the
    /// speed ramps (speeding up and the final stop), with a stated headroom factor of 2 for the
    /// slower stretches (the stopping profile slows the vessel near the destination).
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a positive finite step count of a few thousand"
    )]
    fn step_bound(self, d_max: f64) -> usize {
        let seconds =
            2.0 * (self.path_bound(d_max) / self.max_speed + 2.0 * self.max_speed / self.max_accel);
        (seconds / self.dt).ceil() as usize
    }

    /// Whether the path-length bound applies (a step no longer than the turn radius).
    fn has_path_bound(self) -> bool {
        self.max_speed * self.dt <= self.radius
    }
}

/// Runs a vessel from `from` to `target` (east, north) with `rig`; returns the steps until it is
/// at rest within 1 cm of the target and the path length, or `None` within `limit` steps. Checks
/// the per-step limits on the way (speed change, turn per step).
fn steps_to_arrive(
    rig: Rig,
    from: KinematicState,
    target: (f64, f64),
    limit: usize,
) -> Option<(usize, f64)> {
    let m = rig.model();
    let dt = Seconds(rig.dt);
    let intents = [move_to(target.0, target.1, 0.0)];
    let d = dest(target.0, target.1, 0.0);
    let mut s = from;
    let mut path = 0.0;
    for i in 0..limit {
        let next = m.step(&s, &intents, dt, &flat());
        path += (next.position - s.position).norm();
        let accel = (speed_of(&next) - speed_of(&s)).abs() / rig.dt;
        // the arrival snap may drop up to `2 * max_accel * dt` of speed to zero in one step
        let landed = speed_of(&next) == 0.0;
        assert!(
            landed || accel <= rig.max_accel + 1e-6,
            "step {i}: accel {accel}"
        );
        let turn = angle_delta(heading_of(&next), heading_of(&s)).abs();
        let max_turn = speed_of(&s).max(speed_of(&next)) / rig.radius * rig.dt;
        assert!(
            turn <= max_turn + 1e-9,
            "step {i}: turn {turn} > {max_turn}"
        );
        s = next;
        let off = LocalFrame::at(s.position.to_geodetic()).to_enu(d);
        if speed_of(&s) < 1e-9 && umbra_core::math::hypot(off.east, off.north) < 1e-2 {
            return Some((i, path));
        }
    }
    None
}

/// Arrives within the derived bound for `rig` (and, when it applies, the path bound that rules
/// out a second circle). `d_max` is the largest distance the caller uses.
fn assert_arrives(rig: Rig, speed: f64, target: (f64, f64), d_max: f64) -> usize {
    let bound = rig.step_bound(d_max);
    let (steps, path) = steps_to_arrive(rig, start(0.0, speed, 2.0), target, bound)
        .unwrap_or_else(|| panic!("{target:?} from {speed} m/s: no arrival in {bound} steps"));
    if rig.has_path_bound() {
        let limit = rig.path_bound(d_max);
        assert!(
            path <= limit,
            "{target:?}: path {path} m > {limit} m (re-orbit)"
        );
    }
    steps
}

/// ASM-694 (M1 of the WI-5 review): a destination inside the turning circle used to be circled
/// forever. 30 m abeam at full speed arrives.
#[test]
fn given_a_destination_inside_the_turning_circle_abeam_at_full_speed_when_stepped_then_it_arrives()
{
    for east in [30.0, -30.0] {
        assert_arrives(DEFAULT, 10.0, (east, 0.0), 30.0);
    }
}

/// ASM-694: 10 m abeam from rest used to end in a full-speed orbit.
#[test]
fn given_a_destination_inside_the_turning_circle_abeam_from_rest_when_stepped_then_it_arrives() {
    for east in [10.0, -10.0] {
        assert_arrives(DEFAULT, 0.0, (east, 0.0), 10.0);
    }
}

/// ASM-694: directly behind, at distances from inside the margin to a few circles. The hold
/// margin covers a destination closer than about `sqrt(2 R m)` behind; it still arrives.
#[test]
fn given_a_destination_directly_behind_when_stepped_then_it_arrives() {
    for behind in [1.0, 8.0, 30.0, 100.0, 500.0] {
        assert_arrives(DEFAULT, 10.0, (0.0, -behind), 500.0);
    }
}

/// ASM-694 (re-review M-A, band regression): a destination that leaves the hold zone, or a
/// slightly off-track one on the final approach, no longer costs a second circle. These are the
/// reviewer's cases, each of which looped before the hold band.
#[test]
fn given_a_near_miss_on_the_final_approach_when_stepped_then_it_arrives_without_a_second_circle() {
    // 10 m/s^2 from rest, destination 50.05 m abeam: five passes before the band
    assert_arrives(HIGH_ACCEL, 0.0, (50.05, 0.0), 50.05);
    // abeam at 49.95 m from rest: it leaves the hold zone exactly on a step boundary
    for east in [49.95, -49.95] {
        assert_arrives(DEFAULT, 0.0, (east, 0.0), 49.95);
    }
    // 0.1 s steps from 5 m/s, destination 0.5 m away: missed by 0.08 m and looped four times
    for bearing in [0.0, 0.5, 1.0, 1.5, 2.5, 4.0] {
        let (sin, cos) = umbra_core::math::sin_cos(bearing);
        assert_arrives(COARSE_DT, 5.0, (0.5 * sin, 0.5 * cos), 0.5);
    }
}

/// ASM-694: the grid of speeds, distances and bearings around the circle boundary arrives for
/// every parameter set, including the high-acceleration row, the coarse step and a step longer
/// than the turn radius (arrival only; there is no circle to bound the path by).
#[test]
fn given_destinations_around_the_vessel_when_stepped_then_every_one_arrives() {
    let rows = [
        (DEFAULT, [2.0, 20.0, 60.0, 95.0, 100.0, 105.0, 150.0, 400.0]),
        (
            HIGH_ACCEL,
            [2.0, 20.0, 60.0, 95.0, 100.0, 105.0, 150.0, 400.0],
        ),
        (
            COARSE_DT,
            [2.0, 20.0, 60.0, 95.0, 100.0, 105.0, 150.0, 400.0],
        ),
        (
            STEP_OVER_RADIUS,
            [1.0, 8.0, 20.0, 30.0, 40.0, 60.0, 90.0, 400.0],
        ),
    ];
    for (rig, distances) in rows {
        let d_max = distances.iter().copied().fold(0.0, f64::max);
        for speed in [0.0, 0.4 * rig.max_speed, rig.max_speed] {
            for distance in distances {
                for k in 0..24 {
                    let bearing = f64::from(k) * core::f64::consts::TAU / 24.0;
                    let (sin, cos) = umbra_core::math::sin_cos(bearing);
                    let target = (distance * sin, distance * cos);
                    let bound = rig.step_bound(d_max);
                    let arrived = steps_to_arrive(rig, start(0.0, speed, 2.0), target, bound);
                    assert!(
                        arrived.is_some(),
                        "R {} dt {:.3} v {speed} d {distance} bearing {bearing:.3}",
                        rig.radius,
                        rig.dt
                    );
                    if rig.has_path_bound() {
                        assert_arrives(rig, speed, target, d_max);
                    }
                }
            }
        }
    }
}

/// A weak engine: the stopping overshoot dominates (`max_speed^2 / max_accel` = 2000 m), so the
/// path bound needs its overshoot term.
const WEAK_ENGINE: Rig = Rig {
    max_accel: 0.05,
    ..DEFAULT
};
/// A tight, fast vessel: 20 m/s, 5 m/s^2, R = 5 m, 20 Hz.
const TIGHT: Rig = Rig {
    max_speed: 20.0,
    max_accel: 5.0,
    radius: 5.0,
    dt: 0.05,
};

/// ASM-694 (re-review MA-2): destinations the vessel cannot stop for overshoot by the stopping
/// distance and come back; the path bound includes that, and these two cases exceeded the bound
/// without it (1.47 and 3.03 times).
#[test]
fn given_a_destination_too_near_to_stop_for_when_stepped_then_it_arrives_within_the_path_bound() {
    assert_arrives(TIGHT, 20.0, (0.0, 0.1), 0.1);
    assert_arrives(WEAK_ENGINE, 10.0, (0.0, 0.0), 0.0);
    assert_arrives(WEAK_ENGINE, 10.0, (0.0, 30.0), 30.0);
}

/// REQ-M-1, ASM-694 (re-review MA-1): the arrival snap is the ground model's. A step that goes by
/// the destination at a distance neither moves the vessel sideways nor drops more speed than the
/// limit: 0.1 m/s, destination 6 mm east and 3 mm north, 30 Hz.
#[test]
fn given_a_slow_pass_beside_the_destination_when_stepped_then_the_step_obeys_the_limits() {
    let from = start(0.0, 0.1, 2.0);
    let intents = [move_to(0.006, 0.003, 0.0)];

    let next = model().step(&from, &intents, DT, &flat());

    let accel = (speed_of(&next) - speed_of(&from)).abs() / DT.0;
    assert!(accel <= 1.0 + 1e-6, "speed changed at {accel} m/s^2");
    let moved = (next.position - from.position).norm();
    let allowed = speed_of(&from).max(speed_of(&next)) * DT.0 + 1e-3;
    assert!(moved <= allowed, "moved {moved} m, at most {allowed}");
    assert!(speed_of(&next) > 0.0, "it did not land");
}

/// ASM-610, ASM-694 (fourth review): the near snap needs both speeds at most `2 * max_accel * dt`.
/// At full speed a destination 0.2 m dead ahead is within a step's travel, but the vessel passes
/// it braking at the limit instead of stopping dead.
#[test]
fn given_a_close_destination_ahead_at_full_speed_when_stepped_then_it_brakes_at_the_limit() {
    let from = start(0.0, 10.0, 2.0);

    let next = model().step(&from, &[move_to(0.0, 0.2, 0.0)], DT, &flat());

    assert!(
        speed_of(&next) >= 10.0 - 1.0 / 30.0 - 1e-6,
        "speed fell to {}",
        speed_of(&next)
    );
}

/// ASM-610 (fourth review): the snap needs the destination ahead. Just inside a step's travel but
/// 1.05 mm behind a slow vessel, it is not landed on (that would reverse the vessel).
#[test]
fn given_a_destination_just_behind_a_slow_vessel_when_stepped_then_it_does_not_reverse() {
    let from = start(0.0, 0.066, 2.0);

    let next = model().step(&from, &[move_to(0.0, -0.00105, 0.0)], DT, &flat());

    let along = LocalFrame::at(from.position.to_geodetic())
        .to_enu(next.position)
        .north;
    assert!(along >= 0.0, "moved {along} m along the heading");
    assert!(speed_of(&next) > 0.0, "it landed behind itself");
}

/// ASM-694 (fourth review): within 1 mm the snap applies at rest and in any direction, so the
/// vessel lands in the first step on a destination 0.9 mm behind it (the pinned-height residual).
#[test]
fn given_a_destination_within_a_millimeter_behind_a_vessel_at_rest_when_stepped_then_it_lands() {
    // a weak engine, so that a step's travel (about 0.3 mm) is below the 1 mm floor
    let from = start(0.0, 0.0, 2.0);
    let target = dest(0.0, -0.0009, 0.0);

    let next = WEAK_ENGINE
        .model()
        .step(&from, &[move_to(0.0, -0.0009, 0.0)], DT, &flat());

    let off = LocalFrame::at(next.position.to_geodetic()).to_enu(target);
    assert!(
        umbra_core::math::hypot(off.east, off.north) < 1e-6,
        "{off:?}"
    );
    assert!(speed_of(&next) < 1e-12);
}
