//! AT-14 (M7): property tests for `builtin.sea.kinematic`: whatever the state and intents, the
//! per-step limits hold. Requirements: REQ-M-2, REQ-K-4, ADR 0051, ASM-610.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod common;

use common::{
    DT, HZ, angle_delta, euler_of, flat, heading_of, height_of, move_to, speed_of, start,
};
use proptest::prelude::*;
use umbra_core::{Intent, Meters, MetersPerSecond, MetersPerSecondSquared, MotionModel, Radians};
use umbra_motion::{SeaKinematic, SeaKinematicParams};

const MAX_SPEED: f64 = 10.0;
const MAX_ACCEL: f64 = 1.0;
const RADIUS: f64 = 50.0;
const SURFACE: f64 = 2.0;

fn model() -> SeaKinematic {
    SeaKinematic::new(SeaKinematicParams {
        max_speed: MetersPerSecond(MAX_SPEED),
        max_accel: MetersPerSecondSquared(MAX_ACCEL),
        min_turn_radius: Meters(RADIUS),
        surface_height: Meters(SURFACE),
    })
    .expect("valid params")
}

fn intent() -> impl Strategy<Value = Intent> {
    prop_oneof![
        Just(Intent::Hold),
        (-1000.0..1000.0_f64, -1000.0..1000.0_f64, -300.0..300.0_f64)
            .prop_map(|(e, n, u)| move_to(e, n, u)),
        (-7.0..7.0_f64).prop_map(|h| Intent::SetHeading(Radians(h))),
        (-5.0..20.0_f64).prop_map(|v| Intent::SetSpeed(MetersPerSecond(v))),
    ]
}

proptest! {
    /// REQ-M-2: speed in [0, max], speed change within `max_accel * dt`, heading change within
    /// `v / min_turn_radius * dt` (so the turn radius never drops below the minimum), height
    /// pinned, pitch and roll zero, output finite.
    #[test]
    fn given_any_state_and_intents_when_stepped_then_every_limit_holds(
        heading in -3.0..3.0_f64,
        speed in 0.0..MAX_SPEED,
        height in -50.0..500.0_f64,
        intents in proptest::collection::vec(intent(), 0..4),
        steps in 1usize..40,
    ) {
        let m = model();
        let terrain = flat();
        let mut s = start(heading, speed, height);
        for _ in 0..steps {
            let next = m.step(&s, &intents, DT, &terrain);

            let (v0, v1) = (speed_of(&s), speed_of(&next));
            prop_assert!((-1e-9..=MAX_SPEED + 1e-6).contains(&v1), "speed {v1}");
            if v0 <= MAX_SPEED {
                prop_assert!((v1 - v0).abs() <= MAX_ACCEL / HZ + 1e-6, "speed step {v0} -> {v1}");
            }
            let turned = angle_delta(heading_of(&next), heading_of(&s)).abs();
            let allowed = v0.max(v1) / RADIUS / HZ + 1e-6;
            prop_assert!(turned <= allowed || v0 + v1 < 1e-9, "turned {turned} > {allowed}");
            prop_assert!((height_of(&next) - SURFACE).abs() < 1e-6);
            let e = euler_of(&next);
            prop_assert!(e.pitch.0.abs() < 1e-9 && e.roll.0.abs() < 1e-9);
            prop_assert!(next.position.0.x.is_finite() && next.position.0.y.is_finite());
            s = next;
        }
    }

    /// REQ-K-4: a step is a pure function of its inputs.
    #[test]
    fn given_the_same_inputs_when_stepped_twice_then_outputs_are_bit_identical(
        heading in -3.0..3.0_f64,
        speed in 0.0..MAX_SPEED,
        intents in proptest::collection::vec(intent(), 0..4),
    ) {
        let m = model();
        let from = start(heading, speed, SURFACE);

        let a = m.step(&from, &intents, DT, &flat());
        let b = m.step(&from, &intents, DT, &flat());

        prop_assert_eq!(a.position.0.x.to_bits(), b.position.0.x.to_bits());
        prop_assert_eq!(a.velocity.y.to_bits(), b.velocity.y.to_bits());
        prop_assert_eq!(a.orientation.w.to_bits(), b.orientation.w.to_bits());
    }
}
