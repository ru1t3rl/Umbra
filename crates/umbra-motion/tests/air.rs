//! AT-13 (M7): `builtin.air.kinematic`. Requirements: REQ-M-2, REQ-M-1, REQ-M-4, ADR 0051,
//! ASM-607..609.
//!
//! Parameters: 50..200 m/s, 5 m/s^2, climb 15 m/s, descent 10 m/s, bank 30 degrees (turn rate
//! `9.80665 * tan(30 deg) / v`, 0.0566 rad/s at 100 m/s), no turn cap unless a test sets one,
//! 30 Hz, near 52 N 4 E. Written in M7 WI-1 against stubs, enabled in M7 WI-6.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod common;

use core::f64::consts::{FRAC_PI_2, PI};

use common::{
    DT, HZ, SlopedTerrain, angle_delta, climb_rate_of, euler_of, flat, heading_of, height_of,
    move_to, run, speed_of, start, with_start,
};
use umbra_core::motion::contract::check_motion_model;
use umbra_core::{
    Geodetic, Intent, LocalFrame, Meters, MetersPerSecond, MetersPerSecondSquared, MotionModel,
    Radians, RadiansPerSecond, TerrainProvider,
};
use umbra_motion::{AIR_KINEMATIC_ID, AirKinematic, AirKinematicParams};

const MIN_SPEED: f64 = 50.0;
const MAX_SPEED: f64 = 200.0;
const MAX_ACCEL: f64 = 5.0;
const CLIMB: f64 = 15.0;
const DESCENT: f64 = 10.0;
const BANK_DEG: f64 = 30.0;
const G: f64 = 9.80665;
const EPS: f64 = 1e-9;

fn params() -> AirKinematicParams {
    AirKinematicParams {
        max_speed: MetersPerSecond(MAX_SPEED),
        min_speed: MetersPerSecond(MIN_SPEED),
        max_accel: MetersPerSecondSquared(MAX_ACCEL),
        max_climb_rate: MetersPerSecond(CLIMB),
        max_descent_rate: MetersPerSecond(DESCENT),
        max_bank: Radians(BANK_DEG * PI / 180.0),
        max_turn_rate: None,
        min_altitude_agl: Meters(0.0),
    }
}

fn model_with(p: AirKinematicParams) -> AirKinematic {
    AirKinematic::new(p).expect("valid params")
}

fn model() -> AirKinematic {
    model_with(params())
}

/// The bank-limited turn rate at `speed`, rad/s.
fn bank_turn_rate(speed: f64) -> f64 {
    G * umbra_core::math::tan(BANK_DEG * PI / 180.0) / speed.max(1.0)
}

/// REQ-M-2, ADR 0002: the model id is the namespaced built-in id.
#[test]
fn given_the_model_when_asked_for_its_id_then_it_is_builtin_air_kinematic() {
    assert_eq!(model().id().as_str(), "builtin.air.kinematic");
    assert_eq!(AIR_KINEMATIC_ID, "builtin.air.kinematic");
}

/// REQ-M-2: `params()` returns what the model was built with.
#[test]
fn given_valid_params_when_built_then_params_round_trip() {
    assert_eq!(model().params(), params());
}

/// REQ-M-2, ADR 0051: speeds, rates and the bank are validated.
#[test]
fn given_an_invalid_limit_when_built_then_it_is_rejected() {
    let bad = [
        AirKinematicParams {
            min_speed: MetersPerSecond(MAX_SPEED),
            ..params()
        },
        AirKinematicParams {
            min_speed: MetersPerSecond(-1.0),
            ..params()
        },
        AirKinematicParams {
            max_speed: MetersPerSecond(0.0),
            min_speed: MetersPerSecond(0.0),
            ..params()
        },
        AirKinematicParams {
            max_accel: MetersPerSecondSquared(0.0),
            ..params()
        },
        AirKinematicParams {
            max_climb_rate: MetersPerSecond(0.0),
            ..params()
        },
        AirKinematicParams {
            max_descent_rate: MetersPerSecond(-2.0),
            ..params()
        },
        AirKinematicParams {
            max_bank: Radians(0.0),
            ..params()
        },
        AirKinematicParams {
            max_bank: Radians(FRAC_PI_2),
            ..params()
        },
        AirKinematicParams {
            max_bank: Radians(f64::NAN),
            ..params()
        },
        AirKinematicParams {
            max_turn_rate: Some(RadiansPerSecond(0.0)),
            ..params()
        },
        AirKinematicParams {
            min_altitude_agl: Meters(-1.0),
            ..params()
        },
        AirKinematicParams {
            max_speed: MetersPerSecond(f64::INFINITY),
            ..params()
        },
    ];

    for p in bad {
        assert!(AirKinematic::new(p).is_err(), "{p:?} must be rejected");
    }
}

/// REQ-M-1: the shared motion-model contract holds for level, climbing, turning and holding cases.
#[test]
fn given_assorted_states_and_intents_when_checked_then_the_motion_contract_holds() {
    let cruise = start(1.0, 120.0, 1000.0);
    let cases = vec![
        (cruise, Vec::new()),
        (cruise, vec![Intent::Hold]),
        (cruise, vec![move_to(2000.0, -500.0, 1500.0)]),
        (
            cruise,
            vec![
                Intent::SetHeading(Radians(-2.0)),
                Intent::SetSpeed(MetersPerSecond(80.0)),
            ],
        ),
        (start(0.0, 0.0, 10.0), vec![move_to(0.0, 1000.0, 200.0)]),
    ];

    let result = check_motion_model(&model(), &cases, DT, &flat());

    assert_eq!(result, Ok(()));
}

/// ADR 0051: a far, high destination is approached at exactly the maximum climb rate and never
/// faster.
#[test]
fn given_a_high_destination_when_stepped_then_climb_rate_is_capped_and_reached() {
    let from = start(0.0, 120.0, 1000.0);

    let states = with_start(
        from,
        &run(
            &model(),
            &flat(),
            from,
            &[move_to(0.0, 20_000.0, 3000.0)],
            150,
        ),
    );

    let rates: Vec<f64> = states
        .windows(2)
        .map(|w| (height_of(&w[1]) - height_of(&w[0])) * HZ)
        .collect();
    assert!(
        rates.iter().all(|r| *r <= CLIMB + 1e-6),
        "max rate {:?}",
        rates.iter().copied().fold(f64::MIN, f64::max)
    );
    assert!(
        rates.iter().any(|r| (*r - CLIMB).abs() < 1e-6),
        "the cap is reached"
    );
}

/// ADR 0051: a low destination is approached at exactly the maximum descent rate.
#[test]
fn given_a_low_destination_when_stepped_then_descent_rate_is_capped_and_reached() {
    let from = start(0.0, 120.0, 3000.0);

    let states = with_start(
        from,
        &run(
            &model(),
            &flat(),
            from,
            &[move_to(0.0, 20_000.0, -2000.0)],
            150,
        ),
    );

    let rates: Vec<f64> = states
        .windows(2)
        .map(|w| (height_of(&w[0]) - height_of(&w[1])) * HZ)
        .collect();
    assert!(rates.iter().all(|r| *r <= DESCENT + 1e-6));
    assert!(rates.iter().any(|r| (*r - DESCENT).abs() < 1e-6));
}

/// ASM-607: the aircraft levels off at the destination's height without overshooting it.
#[test]
fn given_a_destination_100_m_up_when_stepped_long_enough_then_it_levels_off_exactly_there() {
    let from = start(0.0, 120.0, 1000.0);
    // The destination's *ellipsoidal* height is 1100 m (ASM-607). `move_to(.., up)` would put it
    // `up` meters above the tangent plane at the origin, which at 30 km is about 70 m higher still
    // because of the Earth's curvature, so the point is built from geodetic coordinates.
    let target = Geodetic::from_degrees(52.27, 4.0, 1100.0).expect("valid position");

    let states = run(
        &model(),
        &flat(),
        from,
        &[Intent::MoveTo {
            destination: target.to_ecef(),
        }],
        400,
    );

    for s in &states {
        assert!(
            height_of(s) <= 1100.0 + 1e-6,
            "no overshoot: {}",
            height_of(s)
        );
    }
    assert!((height_of(&states[399]) - 1100.0).abs() < 1e-6);
}

/// ASM-607: `SetHeading` and `SetSpeed` keep the current height.
#[test]
fn given_a_heading_command_when_stepped_then_height_is_kept() {
    let from = start(0.0, 120.0, 1234.0);

    let states = run(
        &model(),
        &flat(),
        from,
        &[Intent::SetHeading(Radians(1.0))],
        90,
    );

    for s in &states {
        assert!((height_of(s) - 1234.0).abs() < 1e-6);
    }
}

/// ASM-608, hand-computed: at 100 m/s and 30 degrees of bank the turn rate is
/// 9.80665 * tan(30 deg) / 100 = 0.05662 rad/s, and the roll equals the bank in a saturated right
/// turn.
#[test]
fn given_a_90_degree_right_turn_at_100_mps_when_stepped_then_turn_rate_is_the_bank_limited_rate() {
    let from = start(0.0, 100.0, 1000.0);
    let intents = [
        Intent::SetHeading(Radians(FRAC_PI_2)),
        Intent::SetSpeed(MetersPerSecond(100.0)),
    ];

    let states = with_start(from, &run(&model(), &flat(), from, &intents, 60));

    let expected = bank_turn_rate(100.0) / HZ;
    for w in states.windows(2) {
        let turned = angle_delta(heading_of(&w[1]), heading_of(&w[0]));
        assert!(
            (turned - expected).abs() < expected * 0.01,
            "turned {turned}, expected {expected}"
        );
    }
    let roll = euler_of(&states[30]).roll.0;
    assert!((roll - BANK_DEG * PI / 180.0).abs() < 1e-3, "roll {roll}");
}

/// ASM-608: a lower `max_turn_rate` caps the bank-limited rate.
#[test]
fn given_a_turn_rate_cap_when_turning_then_the_cap_is_the_limit() {
    let cap = 1.0_f64.to_radians();
    let capped = model_with(AirKinematicParams {
        max_turn_rate: Some(RadiansPerSecond(cap)),
        ..params()
    });
    let from = start(0.0, 100.0, 1000.0);
    let intents = [
        Intent::SetHeading(Radians(FRAC_PI_2)),
        Intent::SetSpeed(MetersPerSecond(100.0)),
    ];

    let states = with_start(from, &run(&capped, &flat(), from, &intents, 60));

    for w in states.windows(2) {
        let turned = angle_delta(heading_of(&w[1]), heading_of(&w[0])).abs();
        assert!(turned <= cap / HZ + 1e-9, "turned {turned}");
    }
}

/// ASM-608: `Hold` slows to `min_speed` (not to a stop) and keeps heading and height.
#[test]
fn given_a_fast_aircraft_when_holding_then_it_slows_to_min_speed_keeping_heading_and_height() {
    let from = start(0.7, 150.0, 2000.0);

    let states = run(&model(), &flat(), from, &[Intent::Hold], 700);

    let last = &states[699];
    assert!(
        (speed_of(last) - MIN_SPEED).abs() < 1e-6,
        "speed {}",
        speed_of(last)
    );
    assert!(angle_delta(heading_of(last), 0.7).abs() < 1e-6);
    assert!((height_of(last) - 2000.0).abs() < 1e-6);
    for s in &states {
        assert!(speed_of(s) >= MIN_SPEED - 1e-6);
    }
}

/// ASM-608: pitch equals the flight-path angle (positive nose up) during a climb.
#[test]
fn given_a_steady_climb_when_stepped_then_pitch_is_the_flight_path_angle() {
    let from = start(0.0, 120.0, 1000.0);

    let states = run(
        &model(),
        &flat(),
        from,
        &[move_to(0.0, 20_000.0, 3000.0)],
        60,
    );

    let s = &states[59];
    let enu = LocalFrame::at(s.position.to_geodetic()).vector_to_enu(s.velocity);
    let horizontal = (enu.east * enu.east + enu.north * enu.north).sqrt();
    let gamma = umbra_core::math::atan2(enu.up, horizontal);
    assert!(gamma > 0.05, "climbing: {gamma}");
    assert!(
        (euler_of(s).pitch.0 - gamma).abs() < 1e-6,
        "pitch {} vs gamma {gamma}",
        euler_of(s).pitch.0
    );
    assert!((climb_rate_of(s) - CLIMB).abs() < 1e-6);
}

/// ASM-609: over rising terrain the aircraft stays above terrain plus `min_altitude_agl` at its
/// position (a half-meter margin covers the one-step sampling lag), although the destination is
/// below the terrain.
#[test]
fn given_rising_terrain_when_flying_toward_a_low_point_then_it_stays_above_the_agl_floor() {
    let agl = 100.0;
    let terrain = SlopedTerrain { slope: 0.05 };
    let m = model_with(AirKinematicParams {
        min_altitude_agl: Meters(agl),
        ..params()
    });
    let from = start(0.0, 100.0, agl + 10.0);

    let states = run(&m, &terrain, from, &[move_to(0.0, 30_000.0, -50.0)], 600);

    for (i, s) in states.iter().enumerate() {
        let floor = terrain.height_at(s.position.to_geodetic()).0 + agl;
        assert!(
            height_of(s) >= floor - 0.5,
            "step {i}: {} m below floor {floor} m",
            height_of(s)
        );
    }
}

/// ASM-609: below the floor the aircraft climbs at the maximum climb rate.
#[test]
fn given_an_aircraft_below_the_floor_when_stepped_then_it_climbs_at_max_climb_rate() {
    let m = model_with(AirKinematicParams {
        min_altitude_agl: Meters(100.0),
        ..params()
    });
    let from = start(0.0, 100.0, 50.0);

    let next = m.step(&from, &[Intent::SetHeading(Radians(0.0))], DT, &flat());

    assert!(
        (climb_rate_of(&next) - CLIMB).abs() < 1e-6,
        "climb rate {}",
        climb_rate_of(&next)
    );
}

/// ADR 0051: speed stays in `[min_speed, max_speed]` and changes by at most `max_accel * dt`.
#[test]
fn given_a_speed_command_above_max_when_stepped_then_speed_ramps_and_stops_at_max() {
    let from = start(0.0, MIN_SPEED, 1000.0);

    let states = with_start(
        from,
        &run(
            &model(),
            &flat(),
            from,
            &[Intent::SetSpeed(MetersPerSecond(1000.0))],
            1000,
        ),
    );

    for w in states.windows(2) {
        assert!((speed_of(&w[1]) - speed_of(&w[0])).abs() <= MAX_ACCEL / HZ + EPS);
        assert!(speed_of(&w[1]) <= MAX_SPEED + 1e-6);
    }
    assert!((speed_of(&states[1000]) - MAX_SPEED).abs() < 1e-6);
}
