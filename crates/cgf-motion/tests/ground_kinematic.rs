//! AT-8: `builtin.ground.kinematic`. Requirements: REQ-M-2, REQ-M-1, REQ-M-4, ADR 0002,
//! ASM-030, ASM-031, ASM-032.
//!
//! Parameters are the spec 7.1 example (18 m/s, 40 deg/s, 2.5 m/s^2) at 30 Hz, near 52 N 4 E.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

use core::f64::consts::{PI, TAU};

use cgf_core::motion::contract::check_motion_model;
use cgf_core::{
    Ecef, Enu, Euler, FlatTerrain, Geodetic, Intent, KinematicState, LocalFrame, Meters,
    MetersPerSecond, MetersPerSecondSquared, MotionModel, Radians, RadiansPerSecond, Seconds, Vec3,
};
use cgf_motion::{GROUND_KINEMATIC_ID, GroundKinematic, GroundKinematicParams, MotionParamError};
use proptest::prelude::*;

const HZ: f64 = 30.0;
const DT: Seconds = Seconds(1.0 / HZ);
const MAX_SPEED: f64 = 18.0;
const MAX_ACCEL: f64 = 2.5;
const ACCEL_STEP: f64 = MAX_ACCEL / HZ;
/// 40 degrees per second in radians per second.
const MAX_TURN: f64 = 40.0 * PI / 180.0;
const TURN_STEP: f64 = MAX_TURN / HZ;
const EPS: f64 = 1e-9;

fn params() -> GroundKinematicParams {
    GroundKinematicParams {
        max_speed: MetersPerSecond(MAX_SPEED),
        max_turn_rate: RadiansPerSecond(MAX_TURN),
        max_accel: MetersPerSecondSquared(MAX_ACCEL),
    }
}

fn model() -> GroundKinematic {
    GroundKinematic::new(params()).expect("valid params")
}

fn flat() -> FlatTerrain {
    FlatTerrain {
        height: Meters(0.0),
    }
}

fn origin() -> Geodetic {
    Geodetic::from_degrees(52.0, 4.0, 0.0).expect("valid origin")
}

/// Entity at the origin with the given true heading (radians) and speed.
fn start(heading: f64, speed: f64) -> KinematicState {
    let g = origin();
    let frame = LocalFrame::at(g);
    let (s, c) = cgf_core::math::sin_cos(heading);
    KinematicState {
        position: g.to_ecef(),
        velocity: frame.vector_from_enu(Enu {
            east: speed * s,
            north: speed * c,
            up: 0.0,
        }),
        orientation: frame.orientation_from_euler(Euler {
            heading: Radians(heading),
            pitch: Radians(0.0),
            roll: Radians(0.0),
        }),
    }
}

/// A point `east`/`north` meters from the origin on the ground plane.
fn dest(east: f64, north: f64) -> Ecef {
    LocalFrame::at(origin()).from_enu(Enu {
        east,
        north,
        up: 0.0,
    })
}

fn move_to(east: f64, north: f64) -> Intent {
    Intent::MoveTo {
        destination: dest(east, north),
    }
}

fn speed_of(s: &KinematicState) -> f64 {
    s.velocity.norm()
}

fn heading_of(s: &KinematicState) -> f64 {
    LocalFrame::at(s.position.to_geodetic())
        .euler_from_orientation(s.orientation)
        .heading
        .0
}

/// Signed smallest difference `a - b` wrapped to (-pi, pi].
fn angle_delta(a: f64, b: f64) -> f64 {
    let mut d = (a - b) % TAU;
    if d > PI {
        d -= TAU;
    }
    if d <= -PI {
        d += TAU;
    }
    d
}

/// Position of `s` in ENU relative to the scenario origin.
fn enu_of(s: &KinematicState) -> Enu {
    LocalFrame::at(origin()).to_enu(s.position)
}

fn horizontal_distance(s: &KinematicState, east: f64, north: f64) -> f64 {
    let p = enu_of(s);
    let (de, dn) = (p.east - east, p.north - north);
    (de * de + dn * dn).sqrt()
}

/// The states after each of `steps` steps (the start state is not included).
fn run(
    m: &GroundKinematic,
    from: KinematicState,
    intents: &[Intent],
    steps: usize,
) -> Vec<KinematicState> {
    let terrain = flat();
    let mut out = Vec::with_capacity(steps);
    let mut s = from;
    for _ in 0..steps {
        s = m.step(&s, intents, DT, &terrain);
        out.push(s);
    }
    out
}

/// REQ-M-2, ADR 0002: the model id is the namespaced built-in id.
#[test]
fn id_is_builtin_ground_kinematic() {
    assert_eq!(model().id().as_str(), "builtin.ground.kinematic");
    assert_eq!(GROUND_KINEMATIC_ID, "builtin.ground.kinematic");
}

/// REQ-M-2: `params()` returns what the model was built with.
#[test]
fn params_round_trip() {
    assert_eq!(model().params(), params());
}

/// REQ-M-2: every limit must be finite and strictly positive; the error names the field.
#[test]
fn rejects_non_positive_or_non_finite_params() {
    for bad in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut p = params();
        p.max_speed = MetersPerSecond(bad);
        assert!(
            matches!(
                GroundKinematic::new(p),
                Err(MotionParamError::NotPositive {
                    name: "max_speed",
                    ..
                })
            ),
            "max_speed {bad}"
        );

        let mut p = params();
        p.max_turn_rate = RadiansPerSecond(bad);
        assert!(
            matches!(
                GroundKinematic::new(p),
                Err(MotionParamError::NotPositive {
                    name: "max_turn_rate",
                    ..
                })
            ),
            "max_turn_rate {bad}"
        );

        let mut p = params();
        p.max_accel = MetersPerSecondSquared(bad);
        assert!(
            matches!(
                GroundKinematic::new(p),
                Err(MotionParamError::NotPositive {
                    name: "max_accel",
                    ..
                })
            ),
            "max_accel {bad}"
        );
    }
}

/// REQ-M-1: the model passes the LSP contract checks (including the stationary case).
#[test]
fn satisfies_motion_model_contract() {
    let cases = vec![
        (start(0.0, 0.0), vec![]),
        (start(0.0, 0.0), vec![Intent::Hold]),
        (start(1.0, 10.0), vec![move_to(300.0, -200.0)]),
        (
            start(2.0, 5.0),
            vec![Intent::SetSpeed(MetersPerSecond(12.0))],
        ),
        (
            start(3.0, 18.0),
            vec![move_to(0.0, 1.0), Intent::SetSpeed(MetersPerSecond(100.0))],
        ),
    ];

    assert_eq!(check_motion_model(&model(), &cases, DT, &flat()), Ok(()));
}

/// REQ-M-2: speed changes by at most `max_accel * dt` per step, and accelerates at that rate.
#[test]
fn accelerates_by_at_most_max_accel_dt() {
    let states = run(&model(), start(0.0, 0.0), &[move_to(0.0, 2000.0)], 300);

    let mut prev = 0.0;
    for (i, s) in states.iter().enumerate() {
        let v = speed_of(s);
        assert!(
            (v - prev).abs() <= ACCEL_STEP + EPS,
            "step {i}: {prev} -> {v}"
        );
        prev = v;
    }
    assert!(
        (speed_of(&states[0]) - ACCEL_STEP).abs() < EPS,
        "first step uses the full limit"
    );
    assert!(
        (speed_of(&states[149]) - 12.5).abs() < 1e-6,
        "after 5 s at 2.5 m/s^2"
    );
}

/// REQ-M-2: speed never exceeds `max_speed`, even when a higher `SetSpeed` is requested.
#[test]
fn never_exceeds_max_speed() {
    let intents = [
        Intent::SetSpeed(MetersPerSecond(100.0)),
        move_to(0.0, 5000.0),
    ];

    let states = run(&model(), start(0.0, MAX_SPEED), &intents, 200);

    for (i, s) in states.iter().enumerate() {
        assert!(speed_of(s) <= MAX_SPEED + EPS, "step {i}: {}", speed_of(s));
    }
    assert!((speed_of(&states[199]) - MAX_SPEED).abs() < EPS);
}

/// REQ-M-2: `SetSpeed` below the maximum caps the cruise speed.
#[test]
fn set_speed_caps_cruise_speed() {
    let intents = [Intent::SetSpeed(MetersPerSecond(6.0)), move_to(0.0, 5000.0)];

    let states = run(&model(), start(0.0, 0.0), &intents, 300);

    assert!((speed_of(&states[299]) - 6.0).abs() < EPS);
}

/// REQ-M-2: heading changes by at most `max_turn_rate * dt` per step, turning the short way.
#[test]
fn turns_by_at_most_max_turn_rate_dt() {
    let states = run(&model(), start(0.0, 10.0), &[move_to(2000.0, 0.0)], 60);

    let mut prev = 0.0;
    for (i, s) in states.iter().enumerate() {
        let h = heading_of(s);
        let turned = angle_delta(h, prev);
        assert!(turned.abs() <= TURN_STEP + EPS, "step {i}: turned {turned}");
        if i < 30 {
            assert!(
                (turned - TURN_STEP).abs() < EPS,
                "step {i}: full-rate clockwise turn, got {turned}"
            );
        }
        prev = h;
    }
}

/// REQ-M-2: a destination straight ahead is reached and the entity stops there, without
/// overshoot (the along-track distance never exceeds the destination's) and without ever
/// changing speed by more than twice `max_accel * dt` in one step, the last step included.
#[test]
fn moves_to_destination_and_stops_without_overshoot() {
    let states = run(&model(), start(0.0, 0.0), &[move_to(0.0, 300.0)], 1800);

    let furthest = states
        .iter()
        .map(|s| enu_of(s).north)
        .fold(f64::MIN, f64::max);
    let last = states[1799];
    assert!(furthest <= 300.0 + 1e-6, "overshoot to {furthest}");
    assert!(
        horizontal_distance(&last, 0.0, 300.0) < 1e-3,
        "final position"
    );
    assert!(speed_of(&last) < EPS, "final speed");
    let mut prev = 0.0;
    for (i, s) in states.iter().enumerate() {
        let v = speed_of(s);
        assert!(
            (v - prev).abs() <= 2.0 * ACCEL_STEP + EPS,
            "step {i}: {prev} -> {v}"
        );
        prev = v;
    }
}

/// REQ-M-2: at full speed with the destination 0.5 m ahead the entity cannot stop in one step;
/// it must not snap (speed 18 -> 0), it keeps decelerating at the limit.
#[test]
fn fast_entity_close_to_destination_does_not_snap() {
    let from = start(0.0, MAX_SPEED);

    let out = model().step(&from, &[move_to(0.0, 0.5)], DT, &flat());

    assert!(
        speed_of(&out) >= MAX_SPEED - ACCEL_STEP - EPS,
        "speed {}",
        speed_of(&out)
    );
}

/// REQ-M-2: the snap guard looks at the old speed too. A braking entity at 0.24 m/s with the
/// destination 5 mm ahead must not stop in one step (that would be a change of 2.9 times the
/// per-step limit); it keeps decelerating at the limit.
#[test]
fn slow_braking_entity_close_to_destination_does_not_over_brake() {
    let from = start(0.0, 0.24);

    let out = model().step(&from, &[move_to(0.0, 0.005)], DT, &flat());

    let dv = speed_of(&out) - speed_of(&from);
    assert!(dv.abs() <= 2.0 * ACCEL_STEP + EPS, "dv {dv}");
}

/// REQ-M-2: a stopped entity within 1 mm of the destination snaps onto it.
#[test]
fn stopped_entity_half_a_millimetre_away_snaps_onto_the_destination() {
    let out = model().step(&start(0.0, 0.0), &[move_to(0.0, 0.0005)], DT, &flat());

    assert!(horizontal_distance(&out, 0.0, 0.0005) < 1e-9);
    assert!(speed_of(&out) < EPS);
}

/// REQ-M-2: a destination 0.3 m behind a moving entity is not snapped to.
#[test]
fn destination_behind_a_moving_entity_is_not_snapped_to() {
    let out = model().step(&start(0.0, 10.0), &[move_to(0.0, -0.3)], DT, &flat());

    assert!(horizontal_distance(&out, 0.0, -0.3) > 0.3, "moved forward");
    assert!(speed_of(&out) > 1.0, "kept moving: {}", speed_of(&out));
}

/// REQ-M-2: the turning-circle term prevents orbiting. At full speed heading north with the
/// destination 30 m east and 5 m north, the entity must slow enough to reach it.
#[test]
fn fast_entity_with_close_side_destination_arrives_instead_of_orbiting() {
    let intents = [
        move_to(30.0, 5.0),
        Intent::SetSpeed(MetersPerSecond(MAX_SPEED)),
    ];

    let states = run(&model(), start(0.0, MAX_SPEED), &intents, 400);

    let nearest = states
        .iter()
        .map(|s| horizontal_distance(s, 30.0, 5.0))
        .fold(f64::MAX, f64::min);
    assert!(nearest < 1.0, "never closer than {nearest} m");
    assert!(horizontal_distance(&states[399], 30.0, 5.0) < 1.0, "stays");
}

/// ASM-048: a non-finite `SetSpeed` means 0; a non-finite `MoveTo` is ignored.
#[test]
fn non_finite_intents_are_hardened() {
    let from = start(0.3, 8.0);
    let terrain = flat();
    let m = model();
    let nan_dest = Intent::MoveTo {
        destination: Ecef(Vec3 {
            x: f64::NAN,
            y: 0.0,
            z: 0.0,
        }),
    };

    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let got = m.step(
            &from,
            &[move_to(0.0, 1000.0), Intent::SetSpeed(MetersPerSecond(bad))],
            DT,
            &terrain,
        );
        let zero = m.step(
            &from,
            &[move_to(0.0, 1000.0), Intent::SetSpeed(MetersPerSecond(0.0))],
            DT,
            &terrain,
        );
        assert_eq!(got, zero, "SetSpeed({bad})");
    }
    assert_eq!(
        m.step(&from, &[nan_dest], DT, &terrain),
        m.step(&from, &[], DT, &terrain)
    );
}

/// `MotionModel::step` precondition: a non-finite or non-positive `dt` returns the state unchanged.
#[test]
fn invalid_dt_returns_state_unchanged() {
    let from = start(0.3, 8.0);

    for bad in [f64::NAN, f64::INFINITY, 0.0, -0.1] {
        let out = model().step(&from, &[move_to(0.0, 100.0)], Seconds(bad), &flat());
        assert_eq!(out, from, "dt {bad}");
    }
}

/// ASM-049: no pivot turns. A stopped entity that is told not to move does not rotate, even with
/// a destination behind it.
#[test]
fn stopped_entity_with_destination_behind_and_zero_speed_does_not_rotate() {
    let intents = [move_to(0.0, -100.0), Intent::SetSpeed(MetersPerSecond(0.0))];

    let states = run(&model(), start(0.0, 0.0), &intents, 30);

    for (i, s) in states.iter().enumerate() {
        assert!(angle_delta(heading_of(s), 0.0).abs() < EPS, "step {i}");
    }
}

/// ASM-049: a stopped entity with a destination behind it still accelerates and turns.
#[test]
fn stopped_entity_with_destination_behind_turns_while_moving() {
    let states = run(&model(), start(0.0, 0.0), &[move_to(0.0, -300.0)], 1200);

    assert!(
        horizontal_distance(&states[1199], 0.0, -300.0) < 5.0,
        "reached"
    );
}

/// REQ-M-2 (ASM-030): `Hold` decelerates at the limit to a stop and keeps the heading.
#[test]
fn hold_decelerates_to_stop_and_keeps_heading() {
    let east = PI / 2.0;

    let states = run(&model(), start(east, 10.0), &[Intent::Hold], 150);

    let mut prev = 10.0;
    for (i, s) in states.iter().enumerate() {
        let v = speed_of(s);
        assert!(
            v <= prev + EPS && prev - v <= ACCEL_STEP + EPS,
            "step {i}: {prev} -> {v}"
        );
        assert!(
            angle_delta(heading_of(s), east).abs() < EPS,
            "step {i}: heading changed"
        );
        prev = v;
    }
    assert!(speed_of(&states[119]) < EPS, "stopped after 10 / 2.5 = 4 s");
    let travelled = enu_of(&states[149]).east;
    assert!(
        (19.5..20.5).contains(&travelled),
        "stopping distance {travelled}"
    );
}

/// REQ-M-2 (ASM-030): with no intents the entity decelerates to a stop.
#[test]
fn no_intents_decelerates_to_stop() {
    let states = run(&model(), start(0.0, 10.0), &[], 150);

    assert!(speed_of(&states[119]) < EPS);
    assert!(speed_of(&states[0]) < 10.0);
}

/// REQ-M-2 (ASM-030): `SetSpeed` without a destination accelerates along the current heading.
#[test]
fn set_speed_without_destination_keeps_heading() {
    let heading = PI / 4.0;

    let states = run(
        &model(),
        start(heading, 0.0),
        &[Intent::SetSpeed(MetersPerSecond(10.0))],
        200,
    );

    for (i, s) in states.iter().enumerate() {
        assert!(angle_delta(heading_of(s), heading).abs() < EPS, "step {i}");
    }
    assert!((speed_of(&states[199]) - 10.0).abs() < EPS);
    let p = enu_of(&states[199]);
    assert!(p.east > 10.0 && (p.east - p.north).abs() < 1e-3, "{p:?}");
}

/// REQ-M-2 (ASM-030): `Hold` wins over `MoveTo` and `SetSpeed`, in either order.
#[test]
fn hold_wins_over_move_to() {
    let from = start(0.0, 10.0);
    let terrain = flat();
    let m = model();

    let hold = m.step(&from, &[Intent::Hold], DT, &terrain);
    let hold_then_move = m.step(&from, &[Intent::Hold, move_to(2000.0, 0.0)], DT, &terrain);
    let move_then_hold = m.step(
        &from,
        &[
            move_to(2000.0, 0.0),
            Intent::SetSpeed(MetersPerSecond(18.0)),
            Intent::Hold,
        ],
        DT,
        &terrain,
    );

    assert_eq!(hold_then_move, hold);
    assert_eq!(move_then_hold, hold);
    assert!(speed_of(&hold) < 10.0, "decelerating");
}

/// REQ-M-2 (ASM-030): the last `MoveTo` and the last `SetSpeed` win.
#[test]
fn last_move_to_wins() {
    let from = start(0.0, 5.0);
    let terrain = flat();
    let m = model();

    let only_b = m.step(&from, &[move_to(0.0, 500.0)], DT, &terrain);
    let a_then_b = m.step(
        &from,
        &[move_to(500.0, 0.0), move_to(0.0, 500.0)],
        DT,
        &terrain,
    );
    let only_a = m.step(&from, &[move_to(500.0, 0.0)], DT, &terrain);
    let speed_only_last = m.step(
        &from,
        &[Intent::SetSpeed(MetersPerSecond(8.0)), move_to(0.0, 500.0)],
        DT,
        &terrain,
    );
    let speed_both = m.step(
        &from,
        &[
            Intent::SetSpeed(MetersPerSecond(3.0)),
            move_to(0.0, 500.0),
            Intent::SetSpeed(MetersPerSecond(8.0)),
        ],
        DT,
        &terrain,
    );

    assert_eq!(a_then_b, only_b);
    assert_ne!(
        only_a, only_b,
        "the two destinations must be distinguishable"
    );
    assert_eq!(speed_both, speed_only_last);
}

/// REQ-M-2, ASM-032: the entity follows the terrain height (120 m flat terrain) on every step,
/// including the first.
#[test]
fn follows_flat_terrain_height() {
    let m = model();
    let terrain = FlatTerrain {
        height: Meters(120.0),
    };
    let mut s = start(0.0, 5.0);

    for i in 0..60 {
        s = m.step(&s, &[move_to(0.0, 1000.0)], DT, &terrain);
        let h = s.position.to_geodetic().height().0;
        assert!((h - 120.0).abs() < 1e-6, "step {i}: height {h}");
    }
}

/// ASM-032: pitch and roll stay zero on flat terrain.
#[test]
fn pitch_and_roll_are_zero_on_flat_terrain() {
    let states = run(&model(), start(0.5, 5.0), &[move_to(300.0, 300.0)], 120);

    for (i, s) in states.iter().enumerate() {
        let e = LocalFrame::at(s.position.to_geodetic()).euler_from_orientation(s.orientation);
        assert!(
            e.pitch.0.abs() < EPS && e.roll.0.abs() < EPS,
            "step {i}: {e:?}"
        );
    }
}

/// REQ-M-2: velocity is horizontal in the local frame and its magnitude is the speed.
#[test]
fn velocity_is_horizontal_and_along_heading() {
    let states = run(&model(), start(0.0, 5.0), &[move_to(0.0, 1000.0)], 30);

    for (i, s) in states.iter().enumerate() {
        let v = LocalFrame::at(s.position.to_geodetic()).vector_to_enu(s.velocity);
        let h = heading_of(s);
        let (sin_h, cos_h) = cgf_core::math::sin_cos(h);
        assert!(v.up.abs() < EPS, "step {i}: up {}", v.up);
        assert!((v.east - speed_of(s) * sin_h).abs() < 1e-6, "step {i}");
        assert!((v.north - speed_of(s) * cos_h).abs() < 1e-6, "step {i}");
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    /// REQ-M-2: from any initial speed and heading, any destination within 2 km is reached to within 5 m
    /// in at most `ceil((d / (max_speed / 2) + 2 pi / max_turn_rate + max_speed / max_accel + 30 s) * hz)` ticks; the
    /// reachability term keeps the entity from orbiting the destination.
    #[test]
    fn reaches_any_destination_within_bound(
        heading in 0.0_f64..TAU,
        bearing in 0.0_f64..TAU,
        distance in 10.0_f64..2000.0,
        speed in 0.0_f64..=MAX_SPEED,
    ) {
        let (sin_b, cos_b) = cgf_core::math::sin_cos(bearing);
        let (east, north) = (distance * sin_b, distance * cos_b);
        let bound = ((distance / (MAX_SPEED / 2.0) + TAU / MAX_TURN + MAX_SPEED / MAX_ACCEL + 30.0) * HZ).ceil();
        let m = model();
        let terrain = flat();
        let intents = [move_to(east, north)];
        let mut s = start(heading, speed);

        let mut reached = None;
        let mut tick = 0.0;
        while tick < bound {
            s = m.step(&s, &intents, DT, &terrain);
            tick += 1.0;
            if horizontal_distance(&s, east, north) <= 5.0 {
                reached = Some(tick);
                break;
            }
        }

        prop_assert!(reached.is_some(), "not within 5 m after {bound} ticks (d = {distance})");
    }

    /// REQ-M-2: with small destinations (the arrival snap region, mostly ahead of the entity) from any speed, no
    /// step of the whole trajectory changes the speed by more than twice `max_accel * dt`.
    #[test]
    fn whole_trajectory_respects_the_speed_change_limit(
        heading in 0.0_f64..TAU,
        side in prop_oneof![Just(0.0), -0.3_f64..0.3, 0.0_f64..TAU],
        distance in prop_oneof![0.0_f64..0.05, 0.0_f64..20.0],
        speed in prop_oneof![0.0_f64..0.5, 0.0_f64..=MAX_SPEED],
    ) {
        let (sin_b, cos_b) = cgf_core::math::sin_cos(heading + side);
        let intents = [move_to(distance * sin_b, distance * cos_b)];
        let states = run(&model(), start(heading, speed), &intents, 900);

        let mut prev = speed;
        for (i, s) in states.iter().enumerate() {
            let v = speed_of(s);
            prop_assert!((v - prev).abs() <= 2.0 * ACCEL_STEP + EPS, "step {i}: {prev} -> {v}");
            prev = v;
        }
    }

    /// REQ-M-1: a step is a pure function of its inputs: repeated calls, and a freshly built
    /// model, give bit-identical results.
    #[test]
    fn step_is_bitwise_deterministic(
        heading in 0.0_f64..TAU,
        speed in 0.0_f64..=MAX_SPEED,
        east in -500.0_f64..500.0,
        north in -500.0_f64..500.0,
        cruise in 0.0_f64..30.0,
        hold in any::<bool>(),
    ) {
        let mut intents = vec![move_to(east, north), Intent::SetSpeed(MetersPerSecond(cruise))];
        if hold {
            intents.push(Intent::Hold);
        }
        let from = start(heading, speed);
        let terrain = flat();
        let m = model();

        let first = m.step(&from, &intents, DT, &terrain);
        let second = m.step(&from, &intents, DT, &terrain);
        let fresh = model().step(&from, &intents, DT, &terrain);

        prop_assert_eq!(first, second);
        prop_assert_eq!(first, fresh);
    }

    /// REQ-M-2: for any inputs the limits hold on a single step and the output is finite.
    #[test]
    fn single_step_respects_limits(
        heading in 0.0_f64..TAU,
        speed in 0.0_f64..=MAX_SPEED,
        east in -500.0_f64..500.0,
        north in -500.0_f64..500.0,
    ) {
        let from = start(heading, speed);

        let out = model().step(&from, &[move_to(east, north)], DT, &flat());

        let dv = speed_of(&out) - speed_of(&from);
        prop_assert!(speed_of(&out) <= MAX_SPEED + EPS);
        // The arrival snap is only taken from at most 2 * max_accel * dt, all other steps obey max_accel.
        prop_assert!(dv.abs() <= 2.0 * ACCEL_STEP + EPS, "dv {dv}");
        prop_assert!(angle_delta(heading_of(&out), heading_of(&from)).abs() <= TURN_STEP + 1e-7);
        let p: Vec3 = out.position.0;
        prop_assert!(p.x.is_finite() && p.y.is_finite() && p.z.is_finite());
    }
}
