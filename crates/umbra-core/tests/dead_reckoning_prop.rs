//! AT-9 (property part): dead-reckoning invariants. Requirements: REQ-N-6, ADR 0032.

#![forbid(unsafe_code)]

use proptest::prelude::*;
use umbra_core::dead_reckoning::{DeadReckoning, RemoteState, extrapolate};
use umbra_core::{Ecef, KinematicState, Quaternion, Seconds, Vec3};

fn vec(r: f64) -> impl Strategy<Value = Vec3> {
    (-r..r, -r..r, -r..r).prop_map(|(x, y, z)| Vec3::new(x, y, z))
}

fn model() -> impl Strategy<Value = DeadReckoning> {
    prop::sample::select(vec![
        DeadReckoning::Static,
        DeadReckoning::Velocity,
        DeadReckoning::VelocityRotation,
        DeadReckoning::AccelerationRotation,
        DeadReckoning::Acceleration,
        DeadReckoning::BodyVelocity,
        DeadReckoning::BodyVelocityRotation,
        DeadReckoning::BodyAccelerationRotation,
        DeadReckoning::BodyAcceleration,
    ])
}

fn remote() -> impl Strategy<Value = RemoteState> {
    (
        vec(7e6),
        vec(500.0),
        (-1.0_f64..1.0, -1.0_f64..1.0, -1.0_f64..1.0, -1.0_f64..1.0),
        vec(50.0),
        vec(3.0),
        model(),
    )
        .prop_filter_map("non-zero quaternion", |(p, v, q, a, w, m)| {
            let o = Quaternion {
                w: q.0,
                x: q.1,
                y: q.2,
                z: q.3,
            }
            .normalized()?;
            Some(RemoteState {
                state: KinematicState {
                    position: Ecef(p),
                    velocity: v,
                    orientation: o,
                },
                acceleration: a,
                angular_velocity: w,
                model: m,
            })
        })
}

proptest! {
    /// ADR 0032: zero elapsed time returns the basis.
    #[test]
    fn zero_elapsed_returns_the_basis(b in remote()) {
        prop_assert_eq!(extrapolate(&b, Seconds(0.0)), b.state);
    }

    /// ADR 0032: the velocity model is linear in time.
    #[test]
    fn velocity_is_linear_in_time(mut b in remote(), t in 0.0_f64..100.0) {
        b.model = DeadReckoning::Velocity;
        let p = b.state.position.0;
        let one = extrapolate(&b, Seconds(t));
        let two = extrapolate(&b, Seconds(2.0 * t));
        prop_assert!(((two.position.0 - p) - (one.position.0 - p) * 2.0).norm() < 1e-6);
        prop_assert_eq!(one.velocity, b.state.velocity);
    }

    /// ADR 0032: the orientation keeps unit norm within 1e-12 after rotation.
    #[test]
    fn rotation_keeps_unit_norm(b in remote(), t in 0.0_f64..1000.0) {
        let q = extrapolate(&b, Seconds(t)).orientation;
        let n = (q.w * q.w + q.x * q.x + q.y * q.y + q.z * q.z).sqrt();
        prop_assert!((n - 1.0).abs() < 1e-12, "norm {n}");
    }
}
