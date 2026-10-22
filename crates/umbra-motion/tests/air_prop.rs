//! AT-13 (M7): property tests for `builtin.air.kinematic`: whatever the state and intents, the
//! per-step limits hold. Requirements: REQ-M-2, REQ-K-4, ADR 0051, ASM-607..609. Ignored until
//! M7 WI-6.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod common;

use core::f64::consts::PI;

use common::{DT, HZ, angle_delta, climb_rate_of, flat, heading_of, move_to, speed_of, start};
use proptest::prelude::*;
use umbra_core::{Intent, Meters, MetersPerSecond, MetersPerSecondSquared, MotionModel, Radians};
use umbra_motion::{AirKinematic, AirKinematicParams};

const MIN_SPEED: f64 = 50.0;
const MAX_SPEED: f64 = 200.0;
const MAX_ACCEL: f64 = 5.0;
const CLIMB: f64 = 15.0;
const DESCENT: f64 = 10.0;
const BANK: f64 = 30.0 * PI / 180.0;
const G: f64 = 9.80665;

fn model() -> AirKinematic {
    AirKinematic::new(AirKinematicParams {
        max_speed: MetersPerSecond(MAX_SPEED),
        min_speed: MetersPerSecond(MIN_SPEED),
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
    #[ignore = "M7 WI-6"]
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
    #[ignore = "M7 WI-6"]
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
}
