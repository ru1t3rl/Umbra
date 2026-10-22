//! AT-9: dead reckoning against hand-computed values. Requirements: REQ-N-6, ADR 0032, ASM-229.

#![forbid(unsafe_code)]

use core::f64::consts::FRAC_PI_2;

use umbra_core::dead_reckoning::{DeadReckoning, RemoteState, extrapolate};
use umbra_core::{Ecef, KinematicState, Quaternion, Seconds, Vec3, WorldEuler};

const EPS: f64 = 1e-9;

fn base(model: DeadReckoning) -> RemoteState {
    RemoteState {
        state: KinematicState {
            position: Ecef(Vec3::new(1000.0, 2000.0, 3000.0)),
            velocity: Vec3::new(10.0, -4.0, 2.0),
            orientation: Quaternion::IDENTITY,
        },
        acceleration: Vec3::new(1.0, 2.0, 0.0),
        angular_velocity: Vec3::ZERO,
        model,
    }
}

fn near(a: Vec3, b: Vec3) -> bool {
    (a - b).norm() < EPS
}

/// ADR 0032: `Static` stays where it is.
#[test]
fn static_returns_the_basis() {
    let b = base(DeadReckoning::Static);
    assert_eq!(extrapolate(&b, Seconds(7.0)), b.state);
}

/// ADR 0032: straight line, `p + v t`; velocity unchanged.
#[test]
fn velocity_is_a_straight_line() {
    let s = extrapolate(&base(DeadReckoning::Velocity), Seconds(2.0));
    assert!(near(s.position.0, Vec3::new(1020.0, 1992.0, 3004.0)));
    assert!(near(s.velocity, Vec3::new(10.0, -4.0, 2.0)));
    assert_eq!(s.orientation, Quaternion::IDENTITY);
}

/// ADR 0032: constant acceleration, `p + v t + a t^2 / 2` and `v + a t`.
#[test]
fn acceleration_is_quadratic() {
    let s = extrapolate(&base(DeadReckoning::Acceleration), Seconds(2.0));
    assert!(near(
        s.position.0,
        Vec3::new(1000.0 + 20.0 + 2.0, 2000.0 - 8.0 + 4.0, 3004.0)
    ));
    assert!(near(s.velocity, Vec3::new(12.0, 0.0, 2.0)));
}

/// ADR 0032: a constant yaw rate (about body z = down) for a quarter turn.
#[test]
fn constant_yaw_rate_quarter_turn() {
    let mut b = base(DeadReckoning::VelocityRotation);
    b.angular_velocity = Vec3::new(0.0, 0.0, FRAC_PI_2);
    let s = extrapolate(&b, Seconds(1.0));
    // Position is the straight line; the body forward axis (1,0,0) turned about z by 90 degrees.
    assert!(near(s.position.0, Vec3::new(1010.0, 1996.0, 3002.0)));
    assert!(near(
        s.orientation.rotate(Vec3::new(1.0, 0.0, 0.0)),
        Vec3::new(0.0, 1.0, 0.0)
    ));
    let n = s.orientation;
    assert!((n.w * n.w + n.x * n.x + n.y * n.y + n.z * n.z - 1.0).abs() < 1e-12);
}

/// ADR 0032: `AccelerationRotation` combines both.
#[test]
fn acceleration_rotation_combines_both() {
    let mut b = base(DeadReckoning::AccelerationRotation);
    b.angular_velocity = Vec3::new(FRAC_PI_2, 0.0, 0.0);
    let s = extrapolate(&b, Seconds(1.0));
    assert!(near(s.position.0, Vec3::new(1010.5, 1997.0, 3002.0)));
    assert!(near(s.velocity, Vec3::new(11.0, -2.0, 2.0)));
    assert!(near(
        s.orientation.rotate(Vec3::new(0.0, 1.0, 0.0)),
        Vec3::new(0.0, 0.0, 1.0)
    ));
}

/// Rotation rates are in body axes: with a basis yaw of 90 degrees a roll rate turns about the
/// body forward axis (world +y), not about world x.
#[test]
fn rotation_is_about_body_axes() {
    let mut b = base(DeadReckoning::VelocityRotation);
    b.state.orientation = Quaternion::from_world_euler(WorldEuler {
        yaw: umbra_core::Radians(FRAC_PI_2),
        pitch: umbra_core::Radians(0.0),
        roll: umbra_core::Radians(0.0),
    });
    b.angular_velocity = Vec3::new(0.3, 0.0, 0.0);
    let s = extrapolate(&b, Seconds(1.0));
    // The forward axis stays on world +y while rolling.
    assert!(near(
        s.orientation.rotate(Vec3::new(1.0, 0.0, 0.0)),
        Vec3::new(0.0, 1.0, 0.0)
    ));
}

/// ASM-229: a body-frame acceleration is rotated into ECEF with the basis orientation.
#[test]
fn body_frame_acceleration_is_rotated_with_the_basis() {
    let mut b = base(DeadReckoning::BodyAcceleration);
    b.state.orientation = Quaternion::from_world_euler(WorldEuler {
        yaw: umbra_core::Radians(FRAC_PI_2),
        pitch: umbra_core::Radians(0.0),
        roll: umbra_core::Radians(0.0),
    });
    b.acceleration = Vec3::new(2.0, 0.0, 0.0); // body forward = world +y
    let s = extrapolate(&b, Seconds(2.0));
    assert!(near(s.velocity, Vec3::new(10.0, -4.0 + 4.0, 2.0)));
    assert!(near(
        s.position.0,
        Vec3::new(1020.0, 2000.0 - 8.0 + 4.0, 3004.0)
    ));
    // Without acceleration the body variants equal the world ones.
    let v = extrapolate(&base(DeadReckoning::BodyVelocity), Seconds(2.0));
    assert!(near(v.position.0, Vec3::new(1020.0, 1992.0, 3004.0)));
}

/// ADR 0032: the body rotating variants rotate like the world ones.
#[test]
fn body_rotating_variants_rotate() {
    for m in [
        DeadReckoning::BodyVelocityRotation,
        DeadReckoning::BodyAccelerationRotation,
    ] {
        let mut b = base(m);
        b.angular_velocity = Vec3::new(0.0, 0.0, FRAC_PI_2);
        let s = extrapolate(&b, Seconds(1.0));
        assert!(near(
            s.orientation.rotate(Vec3::new(1.0, 0.0, 0.0)),
            Vec3::new(0.0, 1.0, 0.0)
        ));
    }
}

/// A non-finite or negative elapsed time returns the basis (no panic, no NaN).
#[test]
fn bad_elapsed_returns_the_basis() {
    let b = base(DeadReckoning::Acceleration);
    for t in [f64::NAN, f64::INFINITY, -1.0] {
        assert_eq!(extrapolate(&b, Seconds(t)), b.state);
    }
}

/// ADR 0032: rotation renormalizes, so a basis that drifted off unit norm comes back unit.
#[test]
fn rotation_renormalizes_a_drifted_basis() {
    let mut b = base(DeadReckoning::VelocityRotation);
    b.state.orientation = Quaternion {
        w: 1.000_001,
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
    b.angular_velocity = Vec3::new(0.0, 0.0, 0.1);
    let q = extrapolate(&b, Seconds(1.0)).orientation;
    assert!((q.w * q.w + q.x * q.x + q.y * q.y + q.z * q.z - 1.0).abs() < 1e-12);
}

/// ASM-229: the body-frame rotating model rotates the acceleration with the basis orientation;
/// `BodyVelocityRotation` ignores acceleration.
#[test]
fn body_acceleration_rotation_and_velocity_rotation() {
    let yaw90 = Quaternion::from_world_euler(WorldEuler {
        yaw: umbra_core::Radians(FRAC_PI_2),
        pitch: umbra_core::Radians(0.0),
        roll: umbra_core::Radians(0.0),
    });
    let mut b = base(DeadReckoning::BodyAccelerationRotation);
    b.state.orientation = yaw90;
    b.acceleration = Vec3::new(2.0, 0.0, 0.0);
    let s = extrapolate(&b, Seconds(2.0));
    assert!(near(s.velocity, Vec3::new(10.0, 0.0, 2.0)));
    assert!(near(s.position.0, Vec3::new(1020.0, 1996.0, 3004.0)));
    b.model = DeadReckoning::BodyVelocityRotation;
    let s = extrapolate(&b, Seconds(2.0));
    assert!(near(s.velocity, Vec3::new(10.0, -4.0, 2.0)));
    assert!(near(s.position.0, Vec3::new(1020.0, 1992.0, 3004.0)));
}

/// A non-finite input never produces a non-finite state: the basis comes back.
#[test]
fn non_finite_inputs_return_the_basis() {
    let mut b = base(DeadReckoning::Acceleration);
    b.acceleration = Vec3::new(f64::NAN, 0.0, 0.0);
    assert_eq!(extrapolate(&b, Seconds(1.0)).position, b.state.position);
    let mut b = base(DeadReckoning::Velocity);
    b.state.velocity = Vec3::new(f64::INFINITY, 0.0, 0.0);
    assert_eq!(extrapolate(&b, Seconds(1.0)).position, b.state.position);
    let mut b = base(DeadReckoning::VelocityRotation);
    b.angular_velocity = Vec3::new(f64::NAN, 0.0, 0.0);
    assert_eq!(
        extrapolate(&b, Seconds(1.0)).orientation,
        b.state.orientation
    );
}

/// The zero-rotation path also renormalizes a drifted basis.
#[test]
fn zero_turn_renormalizes_too() {
    let mut b = base(DeadReckoning::VelocityRotation);
    b.state.orientation = Quaternion {
        w: 1.000_001,
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
    let q = extrapolate(&b, Seconds(1.0)).orientation;
    assert!((q.w * q.w + q.x * q.x + q.y * q.y + q.z * q.z - 1.0).abs() < 1e-12);
}
