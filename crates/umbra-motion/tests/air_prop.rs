//! AT-13 (M7): property tests for `builtin.air.kinematic`: whatever the state and intents, the
//! per-step limits hold. Requirements: REQ-M-2, REQ-K-4, ADR 0051, ASM-607..609. Enabled in M7 WI-6.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod common;

use core::f64::consts::PI;

use common::{
    DT, HZ, angle_delta, climb_rate_of, dest, flat, heading_of, height_of, move_to, speed_of, start,
};
use proptest::prelude::*;
use umbra_core::{
    Intent, LocalFrame, Meters, MetersPerSecond, MetersPerSecondSquared, MotionModel, Radians,
    Seconds,
};
use umbra_motion::{AirKinematic, AirKinematicParams};

const MIN_SPEED: f64 = 50.0;
const MAX_SPEED: f64 = 200.0;
const MAX_ACCEL: f64 = 5.0;
const CLIMB: f64 = 15.0;
const DESCENT: f64 = 10.0;
const BANK: f64 = 30.0 * PI / 180.0;
const G: f64 = 9.80665;

fn model() -> AirKinematic {
    model_with_min(MIN_SPEED)
}

fn model_with_min(min_speed: f64) -> AirKinematic {
    AirKinematic::new(AirKinematicParams {
        max_speed: MetersPerSecond(MAX_SPEED),
        min_speed: MetersPerSecond(min_speed),
        max_accel: MetersPerSecondSquared(MAX_ACCEL),
        max_climb_rate: MetersPerSecond(CLIMB),
        max_descent_rate: MetersPerSecond(DESCENT),
        max_bank: Radians(BANK),
        max_turn_rate: None,
        min_altitude_agl: Meters(0.0),
    })
    .expect("valid params")
}

fn intent() -> impl Strategy<Value = Intent> {
    prop_oneof![
        Just(Intent::Hold),
        (
            -20_000.0..20_000.0_f64,
            -20_000.0..20_000.0_f64,
            -2000.0..2000.0_f64
        )
            .prop_map(|(e, n, u)| move_to(e, n, u)),
        (-7.0..7.0_f64).prop_map(|h| Intent::SetHeading(Radians(h))),
        (-10.0..400.0_f64).prop_map(|v| Intent::SetSpeed(MetersPerSecond(v))),
    ]
}

proptest! {
    /// REQ-M-2: speed within [min, max] and `max_accel * dt` per step, climb and descent within
    /// their rates, heading change within the bank-limited rate, output finite.
    #[test]
    fn given_any_state_and_intents_when_stepped_then_every_limit_holds(
        heading in -3.0..3.0_f64,
        speed in MIN_SPEED..MAX_SPEED,
        height in 100.0..5000.0_f64,
        intents in proptest::collection::vec(intent(), 0..4),
        steps in 1usize..40,
    ) {
        let m = model();
        let terrain = flat();
        let mut s = start(heading, speed, height);
        for _ in 0..steps {
            let next = m.step(&s, &intents, DT, &terrain);

            let (v0, v1) = (speed_of(&s), speed_of(&next));
            prop_assert!((MIN_SPEED - 1e-6..=MAX_SPEED + 1e-6).contains(&v1), "speed {v1}");
            prop_assert!((v1 - v0).abs() <= MAX_ACCEL / HZ + 1e-6, "speed step {v0} -> {v1}");
            let h0 = common::height_of(&s);
            let h1 = common::height_of(&next);
            prop_assert!((h1 - h0) * HZ <= CLIMB + 1e-6 && (h0 - h1) * HZ <= DESCENT + 1e-6, "height {h0} -> {h1}");
            let turned = angle_delta(heading_of(&next), heading_of(&s)).abs();
            let rate = G * umbra_core::math::tan(BANK) / v0.min(v1).max(1.0);
            prop_assert!(turned <= rate / HZ * 1.01 + 1e-6, "turned {turned} > {}", rate / HZ);
            prop_assert!(climb_rate_of(&next).is_finite());
            prop_assert!(next.position.0.x.is_finite() && next.orientation.w.is_finite());
            s = next;
        }
    }

    /// REQ-K-4: a step is a pure function of its inputs.
    #[test]
    fn given_the_same_inputs_when_stepped_twice_then_outputs_are_bit_identical(
        heading in -3.0..3.0_f64,
        speed in MIN_SPEED..MAX_SPEED,
        intents in proptest::collection::vec(intent(), 0..4),
    ) {
        let m = model();
        let from = start(heading, speed, 1000.0);

        let a = m.step(&from, &intents, DT, &flat());
        let b = m.step(&from, &intents, DT, &flat());

        prop_assert_eq!(a.position.0.z.to_bits(), b.position.0.z.to_bits());
        prop_assert_eq!(a.velocity.x.to_bits(), b.velocity.x.to_bits());
        prop_assert_eq!(a.orientation.z.to_bits(), b.orientation.z.to_bits());
    }

    /// ASM-707: a destination at any height (+-1500 m of the start) and at any step (0.1, 1 or 2 s;
    /// `accel * dt` up to 10 m/s) is reached in height by an aircraft that cannot hover (it flies
    /// over the point at 45 degrees at most and keeps flying toward the destination's height) and
    /// by one that can (it lands on the point, at rest, within 1 cm horizontally and 1 mm
    /// vertically, and holds for 60 steps), with the climb and descent rates respected on the way.
    #[test]
    fn given_a_destination_at_any_height_when_flown_then_the_height_is_reached(
        east in -5000.0..5000.0_f64,
        north in -5000.0..5000.0_f64,
        up in 1000.0..4000.0_f64,
        heading in -3.0..3.0_f64,
        hover in any::<bool>(),
        dt in prop::sample::select(vec![0.1, 1.0, 2.0]),
    ) {
        let m = model_with_min(if hover { 0.0 } else { MIN_SPEED });
        let step = Seconds(dt);
        let goal = dest(east, north, up);
        let goal_height = goal.to_geodetic().height().0;
        let v0 = if hover { 0.0 } else { MIN_SPEED };
        let d = umbra_core::math::hypot(east, north);
        let limit = step_bound(hover, d, goal_height - 2500.0, v0, dt);
        let hold = 60;
        let mut s = start(heading, v0, 2500.0);
        let mut passed = false;
        let mut tail = Vec::new();
        for i in 0..limit + hold {
            let next = m.step(&s, &[Intent::MoveTo { destination: goal }], step, &flat());
            let rate = climb_rate_of(&next);
            prop_assert!(rate <= CLIMB + 1e-6 && -rate <= DESCENT + 1e-6, "rate {rate}");
            let off = LocalFrame::at(next.position.to_geodetic()).to_enu(goal);
            let miss = umbra_core::math::hypot(off.east, off.north);
            passed |= miss <= MAX_SPEED * dt;
            s = next;
            if i >= limit {
                let v = LocalFrame::at(s.position.to_geodetic()).vector_to_enu(s.velocity);
                tail.push((miss, height_of(&s), umbra_core::math::hypot(v.east, v.north)));
            }
        }
        prop_assert!(passed, "never within a step of ({east}, {north})");
        for (miss, h, v) in tail {
            if hover {
                prop_assert!((h - goal_height).abs() < 1e-3, "height {h} vs {goal_height}");
                prop_assert!(miss < 1e-2 && v < 1e-6, "miss {miss} speed {v}");
            } else {
                prop_assert!((h - goal_height).abs() < 1.0, "height {h} vs {goal_height}");
            }
        }
    }
}

/// Steps allowed to reach a destination `d` meters away and `dh` meters (signed) above: twice
/// (stated headroom) the sum of the straight run at the slowest sustained speed (half of the top
/// speed for an aircraft that can stop, else `MIN_SPEED`), the height change at the rate limit, a
/// full turn at the turn rate of the speed reachable over `d`, and the speed and vertical ramps
/// and stops (`2 * (MAX_SPEED + rate) / MAX_ACCEL`), plus ten steps for the last step.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a positive finite step count"
)]
fn step_bound(hover: bool, d: f64, dh: f64, v0: f64, dt: f64) -> usize {
    let rate = if dh >= 0.0 { CLIMB } else { DESCENT };
    let slow = if hover { 0.5 * MAX_SPEED } else { MIN_SPEED };
    let v_ref = (v0 * v0 + 2.0 * MAX_ACCEL * d).sqrt().clamp(1.0, MAX_SPEED);
    let turn = core::f64::consts::TAU * v_ref.max(1.0) / (G * umbra_core::math::tan(BANK));
    let seconds = d / slow + dh.abs() / rate + turn + 2.0 * (MAX_SPEED + rate) / MAX_ACCEL;
    (2.0 * seconds / dt).ceil() as usize + 10
}
