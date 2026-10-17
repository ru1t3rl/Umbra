//! AT-14 (WI-7 rows): `Intent::SetHeading` in `builtin.ground.kinematic`.
//! Requirements: REQ-B-2, REQ-M-1, ADR 0016, ASM-082, ASM-048.
//!
//! Step 4 of M3: written before WI-7. Tests that fail today (the model ignores `SetHeading`) are
//! ignored with the work item that makes them pass; tests of rules that already hold (a later
//! `MoveTo` wins, `Hold` wins, a non-finite heading is ignored) are live and must stay green.
//!
//! Parameters are the same as `ground_kinematic.rs` (18 m/s, 40 deg/s, 2.5 m/s^2) at 30 Hz.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

use core::f64::consts::{FRAC_PI_2, PI, TAU};

use cgf_core::motion::contract::check_motion_model;
use cgf_core::{
    Ecef, Enu, Euler, FlatTerrain, Geodetic, Intent, KinematicState, LocalFrame, Meters,
    MetersPerSecond, MetersPerSecondSquared, MotionModel, Radians, RadiansPerSecond, Seconds,
};
use cgf_motion::{GroundKinematic, GroundKinematicParams};

const HZ: f64 = 30.0;
const DT: Seconds = Seconds(1.0 / HZ);
const MAX_TURN: f64 = 40.0 * PI / 180.0;
const TURN_STEP: f64 = MAX_TURN / HZ;
const EPS: f64 = 1e-9;

fn model() -> GroundKinematic {
    GroundKinematic::new(GroundKinematicParams {
        max_speed: MetersPerSecond(18.0),
        max_turn_rate: RadiansPerSecond(MAX_TURN),
        max_accel: MetersPerSecondSquared(2.5),
    })
    .expect("valid params")
}

fn flat() -> FlatTerrain {
    FlatTerrain {
        height: Meters(0.0),
    }
}

fn origin() -> Geodetic {
    Geodetic::from_degrees(52.0, 4.0, 0.0).expect("valid origin")
}

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

fn dest(east: f64, north: f64) -> Ecef {
    LocalFrame::at(origin()).from_enu(Enu {
        east,
        north,
        up: 0.0,
    })
}

fn heading_of(s: &KinematicState) -> f64 {
    LocalFrame::at(s.position.to_geodetic())
        .euler_from_orientation(s.orientation)
        .heading
        .0
}

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

fn cruise(speed: f64) -> Intent {
    Intent::SetSpeed(MetersPerSecond(speed))
}

fn run(from: KinematicState, intents: &[Intent], steps: usize) -> Vec<KinematicState> {
    let m = model();
    let terrain = flat();
    let mut out = Vec::new();
    let mut s = from;
    for _ in 0..steps {
        s = m.step(&s, intents, DT, &terrain);
        out.push(s);
    }
    out
}

/// REQ-B-2, ADR 0016: `SetHeading` steers toward the heading, limited by the turn rate.
#[test]
#[ignore = "WI-7"]
fn given_set_heading_when_stepped_then_heading_turns_by_exactly_one_turn_step() {
    let states = run(
        start(0.0, 10.0),
        &[cruise(10.0), Intent::SetHeading(Radians(FRAC_PI_2))],
        1,
    );

    assert!((heading_of(&states[0]) - TURN_STEP).abs() < EPS);
}

/// REQ-B-2: the entity reaches the commanded heading and then keeps it.
#[test]
#[ignore = "WI-7"]
fn given_set_heading_when_stepped_long_enough_then_heading_is_reached_and_held() {
    let target = FRAC_PI_2;
    let steps = 120; // 90 deg at 40 deg/s is 2.25 s = 68 steps at 30 Hz.

    let states = run(
        start(0.0, 10.0),
        &[cruise(10.0), Intent::SetHeading(Radians(target))],
        steps,
    );

    for s in &states[70..] {
        assert!(angle_delta(heading_of(s), target).abs() < 1e-6);
    }
}

/// REQ-B-2: the shorter way round is taken across north.
#[test]
#[ignore = "WI-7"]
fn given_set_heading_across_north_when_stepped_then_it_turns_the_short_way() {
    let from = 350.0_f64.to_radians();
    let to = 10.0_f64.to_radians();

    let states = run(
        start(from, 10.0),
        &[cruise(10.0), Intent::SetHeading(Radians(to))],
        1,
    );

    // Turning clockwise by one step is +TURN_STEP (wrapped), not a 340 degree turn the other way.
    assert!((angle_delta(heading_of(&states[0]), from) - TURN_STEP).abs() < EPS);
}

/// ASM-049 stays true for `SetHeading`: no pivot turn, the heading changes only while moving.
#[test]
fn given_set_heading_when_stationary_then_heading_does_not_change() {
    let states = run(
        start(0.0, 0.0),
        &[Intent::SetHeading(Radians(FRAC_PI_2))],
        3,
    );

    for s in &states {
        assert!(angle_delta(heading_of(s), 0.0).abs() < EPS);
    }
}

/// ASM-030, ADR 0016: the last of `MoveTo` and `SetHeading` steers; a later `SetHeading` wins.
#[test]
#[ignore = "WI-7"]
fn given_move_to_then_set_heading_when_stepped_then_set_heading_wins() {
    // MoveTo is due east; SetHeading (later) asks for due north, which is the current heading.
    let intents = [
        cruise(10.0),
        Intent::MoveTo {
            destination: dest(1000.0, 0.0),
        },
        Intent::SetHeading(Radians(0.0)),
    ];

    let states = run(start(0.0, 10.0), &intents, 5);

    for s in &states {
        assert!(angle_delta(heading_of(s), 0.0).abs() < EPS);
    }
}

/// ASM-030, ADR 0016: a later `MoveTo` wins over an earlier `SetHeading`.
#[test]
fn given_set_heading_then_move_to_when_stepped_then_move_to_wins() {
    // SetHeading asks for due east; MoveTo (later) is due north, which is the current heading.
    let intents = [
        cruise(10.0),
        Intent::SetHeading(Radians(FRAC_PI_2)),
        Intent::MoveTo {
            destination: dest(0.0, 1000.0),
        },
    ];

    let states = run(start(0.0, 10.0), &intents, 5);

    for s in &states {
        assert!(angle_delta(heading_of(s), 0.0).abs() < EPS);
    }
}

/// ASM-030: `Hold` still wins over everything, including `SetHeading`.
#[test]
fn given_hold_and_set_heading_when_stepped_then_heading_does_not_change() {
    let intents = [Intent::SetHeading(Radians(FRAC_PI_2)), Intent::Hold];

    let states = run(start(0.0, 10.0), &intents, 5);

    for s in &states {
        assert!(angle_delta(heading_of(s), 0.0).abs() < EPS);
    }
}

/// ASM-048 style: a non-finite heading is ignored, never propagated into the state.
#[test]
fn given_non_finite_set_heading_when_stepped_then_it_is_ignored_and_state_stays_finite() {
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let intents = [cruise(10.0), Intent::SetHeading(Radians(bad))];

        let states = run(start(0.0, 10.0), &intents, 3);

        for s in &states {
            assert!(s.position.0.x.is_finite() && s.velocity.x.is_finite());
            assert!(angle_delta(heading_of(s), 0.0).abs() < EPS, "heading {bad}");
        }
    }
}

/// REQ-M-1: the contract checks (determinism, finite, unit quaternion, stationary) hold with
/// `SetHeading` intents.
#[test]
fn given_set_heading_intents_then_the_motion_contract_holds() {
    let cases = vec![
        (
            start(0.0, 0.0),
            vec![Intent::SetHeading(Radians(FRAC_PI_2))],
        ),
        (
            start(1.0, 10.0),
            vec![cruise(10.0), Intent::SetHeading(Radians(-2.0))],
        ),
        (
            start(2.0, 5.0),
            vec![Intent::SetHeading(Radians(f64::NAN)), Intent::Hold],
        ),
    ];

    assert_eq!(check_motion_model(&model(), &cases, DT, &flat()), Ok(()));
}
