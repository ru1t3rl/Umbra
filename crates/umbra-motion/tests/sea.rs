//! AT-14 (M7): `builtin.sea.kinematic`. Requirements: REQ-M-2, REQ-M-1, REQ-M-4, ADR 0051,
//! ASM-610, ASM-611.
//!
//! Parameters: 10 m/s, 1 m/s^2, 50 m minimum turn radius (turn rate 0.2 rad/s at full speed),
//! surface height 2 m, 30 Hz, near 52 N 4 E. Written in M7 WI-1
//! against stubs, enabled by M7 WI-5.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod common;

use core::f64::consts::FRAC_PI_2;

use common::{
    DT, HZ, SlopedTerrain, angle_delta, euler_of, flat, heading_of, height_of, move_to, run,
    speed_of, start, with_start,
};
use umbra_core::motion::contract::check_motion_model;
use umbra_core::{Intent, Meters, MetersPerSecond, MetersPerSecondSquared, MotionModel, Radians};
use umbra_motion::{SEA_KINEMATIC_ID, SeaKinematic, SeaKinematicParams};

const MAX_SPEED: f64 = 10.0;
const MAX_ACCEL: f64 = 1.0;
const RADIUS: f64 = 50.0;
const SURFACE: f64 = 2.0;
const EPS: f64 = 1e-9;

fn params() -> SeaKinematicParams {
    SeaKinematicParams {
        max_speed: MetersPerSecond(MAX_SPEED),
        max_accel: MetersPerSecondSquared(MAX_ACCEL),
        min_turn_radius: Meters(RADIUS),
        surface_height: Meters(SURFACE),
    }
}

fn model() -> SeaKinematic {
    SeaKinematic::new(params()).expect("valid params")
}

/// REQ-M-2, ADR 0002: the model id is the namespaced built-in id.
#[test]
fn given_the_model_when_asked_for_its_id_then_it_is_builtin_sea_kinematic() {
    assert_eq!(model().id().as_str(), "builtin.sea.kinematic");
    assert_eq!(SEA_KINEMATIC_ID, "builtin.sea.kinematic");
}

/// REQ-M-2: `params()` returns what the model was built with.
#[test]
fn given_valid_params_when_built_then_params_round_trip() {
    assert_eq!(model().params(), params());
}

/// REQ-M-2: speed, acceleration and turn radius must be finite and > 0, the surface height finite.
#[test]
fn given_a_non_positive_or_non_finite_limit_when_built_then_it_is_rejected() {
    let bad = [
        SeaKinematicParams {
            max_speed: MetersPerSecond(0.0),
            ..params()
        },
        SeaKinematicParams {
            max_speed: MetersPerSecond(f64::NAN),
            ..params()
        },
        SeaKinematicParams {
            max_accel: MetersPerSecondSquared(-1.0),
            ..params()
        },
        SeaKinematicParams {
            min_turn_radius: Meters(0.0),
            ..params()
        },
        SeaKinematicParams {
            min_turn_radius: Meters(f64::INFINITY),
            ..params()
        },
        SeaKinematicParams {
            surface_height: Meters(f64::NAN),
            ..params()
        },
    ];

    for p in bad {
        assert!(SeaKinematic::new(p).is_err(), "{p:?} must be rejected");
    }
}

/// REQ-M-1: the shared motion-model contract (deterministic, finite, unit quaternion, no drift at
/// rest) holds for at rest, under way, `MoveTo`, `SetHeading` and `Hold`.
#[test]
fn given_assorted_states_and_intents_when_checked_then_the_motion_contract_holds() {
    let rest = start(0.0, 0.0, SURFACE);
    let cases = vec![
        (rest, Vec::new()),
        (rest, vec![Intent::Hold]),
        (start(1.0, 7.0, SURFACE), Vec::new()),
        (start(1.0, 7.0, SURFACE), vec![move_to(300.0, -200.0, 50.0)]),
        (
            start(2.0, 3.0, SURFACE),
            vec![
                Intent::SetHeading(Radians(0.5)),
                Intent::SetSpeed(MetersPerSecond(5.0)),
            ],
        ),
    ];

    let result = check_motion_model(&model(), &cases, DT, &flat());

    assert_eq!(result, Ok(()));
}

/// ASM-610: the height is pinned to `surface_height`, whatever the start height, the `MoveTo`
/// height or the terrain.
#[test]
fn given_a_high_start_and_a_high_destination_over_hills_when_stepped_then_height_is_the_surface() {
    let hills = SlopedTerrain { slope: 0.5 };
    let states = run(
        &model(),
        &hills,
        start(0.0, 5.0, 100.0),
        &[move_to(0.0, 500.0, 500.0)],
        30,
    );

    for (i, s) in states.iter().enumerate() {
        assert!(
            (height_of(s) - SURFACE).abs() < 1e-6,
            "step {i}: height {} m",
            height_of(s)
        );
    }
}

/// ASM-610: pitch and roll stay zero.
#[test]
fn given_a_turning_vessel_when_stepped_then_pitch_and_roll_are_zero() {
    let states = run(
        &model(),
        &flat(),
        start(0.0, 10.0, SURFACE),
        &[move_to(300.0, 100.0, 0.0)],
        120,
    );

    for s in &states {
        let e = euler_of(s);
        assert!(e.pitch.0.abs() < EPS && e.roll.0.abs() < EPS, "{e:?}");
    }
}

/// ADR 0051: `Hold` decelerates at `max_accel` to a stop and never reverses.
#[test]
fn given_a_moving_vessel_when_holding_then_it_stops_at_max_deceleration_without_reversing() {
    let from = start(0.0, MAX_SPEED, SURFACE);
    let steps = 330;

    let states = with_start(from, &run(&model(), &flat(), from, &[Intent::Hold], steps));

    for pair in states.windows(2) {
        let (before, after) = (speed_of(&pair[0]), speed_of(&pair[1]));
        assert!(after >= -EPS, "speed {after}");
        assert!(
            before - after <= MAX_ACCEL / HZ + EPS,
            "deceleration {} per step",
            before - after
        );
    }
    assert!(
        speed_of(&states[states.len() - 1]) < EPS,
        "stopped after 10 s plus margin"
    );
}

/// ASM-610: no turn at rest, so a `SetHeading` without a speed leaves the heading alone.
#[test]
fn given_a_vessel_at_rest_when_told_a_heading_then_it_does_not_pivot() {
    let from = start(0.0, 0.0, SURFACE);

    let states = run(
        &model(),
        &flat(),
        from,
        &[Intent::SetHeading(Radians(FRAC_PI_2))],
        60,
    );

    for s in &states {
        assert!(angle_delta(heading_of(s), 0.0).abs() < EPS);
    }
}

/// ADR 0051, hand-computed: at 10 m/s with a 50 m radius the turn rate is 0.2 rad/s, so a quarter
/// turn to the east takes (pi/2) / (0.2 / 30) = 235.6 steps: still turning after 235, on heading
/// after 236, at constant speed.
#[test]
fn given_full_speed_when_told_east_then_the_quarter_turn_takes_236_steps_at_the_minimum_radius() {
    let from = start(0.0, MAX_SPEED, SURFACE);
    let intents = [
        Intent::SetHeading(Radians(FRAC_PI_2)),
        Intent::SetSpeed(MetersPerSecond(MAX_SPEED)),
    ];

    let states = run(&model(), &flat(), from, &intents, 236);

    let after_235 = angle_delta(FRAC_PI_2, heading_of(&states[234]));
    let after_236 = angle_delta(FRAC_PI_2, heading_of(&states[235]));
    assert!(
        after_235 > 1e-3,
        "still turning after 235 steps: {after_235}"
    );
    assert!(
        after_236.abs() < 1e-6,
        "on heading after 236 steps: {after_236}"
    );
    assert!((speed_of(&states[235]) - MAX_SPEED).abs() < 1e-6);
}

/// ADR 0051: the model never exceeds `max_speed` and reaches it at `max_accel`.
#[test]
fn given_a_far_destination_when_stepped_then_speed_ramps_at_max_accel_up_to_max_speed() {
    let states = run(
        &model(),
        &flat(),
        start(0.0, 0.0, SURFACE),
        &[move_to(0.0, 5000.0, 0.0)],
        400,
    );

    assert!(
        (speed_of(&states[0]) - MAX_ACCEL / HZ).abs() < 1e-6,
        "first step accelerates by a/hz"
    );
    for s in &states {
        assert!(speed_of(s) <= MAX_SPEED + 1e-6);
    }
    assert!((speed_of(&states[399]) - MAX_SPEED).abs() < 1e-6);
}
