//! AT-10: world-frame (ECEF) Euler angles and the angle between orientations.
//! Requirements: REQ-N-6 (ECEF conversion), REQ-C-3, ADR 0032.
//!
//! Convention: `yaw` about ECEF z, then `pitch` about the new y, then `roll` about the new x
//! (z-y'-x''), taking body FRD axes to ECEF axes.

#![forbid(unsafe_code)]

use core::f64::consts::{FRAC_PI_2, PI};

use proptest::prelude::*;
use umbra_core::{Euler, Geodetic, LocalFrame, Quaternion, Radians, Vec3, WorldEuler};

const EPS: f64 = 1e-12;

fn near(a: Vec3, b: Vec3) -> bool {
    (a - b).norm() < EPS
}

fn we(yaw: f64, pitch: f64, roll: f64) -> WorldEuler {
    WorldEuler {
        yaw: Radians(yaw),
        pitch: Radians(pitch),
        roll: Radians(roll),
    }
}

/// Smallest rotation angle between two orientations, ignoring the double cover.
fn same_rotation(a: Quaternion, b: Quaternion) -> bool {
    let d = a.w * b.w + a.x * b.x + a.y * b.y + a.z * b.z;
    (d.abs() - 1.0).abs() < 1e-12
}

/// ADR 0032: the identity orientation has zero angles.
#[test]
fn identity_is_all_zero() {
    let e = Quaternion::IDENTITY.to_world_euler();
    assert!(e.yaw.0.abs() < EPS && e.pitch.0.abs() < EPS && e.roll.0.abs() < EPS);
}

/// ADR 0032: a pure yaw turns the forward axis about ECEF z.
#[test]
fn yaw_turns_about_ecef_z() {
    let q = Quaternion::from_world_euler(we(FRAC_PI_2, 0.0, 0.0));
    assert!(near(
        q.rotate(Vec3::new(1.0, 0.0, 0.0)),
        Vec3::new(0.0, 1.0, 0.0)
    ));
    let e = q.to_world_euler();
    assert!((e.yaw.0 - FRAC_PI_2).abs() < EPS && e.pitch.0.abs() < EPS && e.roll.0.abs() < EPS);
}

/// ADR 0032: positive pitch is about the (new) y axis, so forward tips toward -z.
#[test]
fn pitch_turns_about_y() {
    let q = Quaternion::from_world_euler(we(0.0, FRAC_PI_2 / 2.0, 0.0));
    let f = q.rotate(Vec3::new(1.0, 0.0, 0.0));
    let s = (0.5_f64).sqrt();
    assert!(near(f, Vec3::new(s, 0.0, -s)));
    let e = q.to_world_euler();
    assert!((e.pitch.0 - FRAC_PI_2 / 2.0).abs() < EPS);
}

/// ADR 0032: positive roll is about the forward x axis, so the y axis tips toward +z.
#[test]
fn roll_turns_about_x() {
    let q = Quaternion::from_world_euler(we(0.0, 0.0, FRAC_PI_2));
    assert!(near(
        q.rotate(Vec3::new(0.0, 1.0, 0.0)),
        Vec3::new(0.0, 0.0, 1.0)
    ));
    let e = q.to_world_euler();
    assert!((e.roll.0 - FRAC_PI_2).abs() < EPS && e.yaw.0.abs() < EPS && e.pitch.0.abs() < EPS);
}

/// ADR 0032: at pitch +-90 degrees the decomposition is a gimbal lock; the angles recovered
/// still rebuild the same rotation, and pitch is +-pi/2.
#[test]
fn gimbal_edges_rebuild_the_rotation() {
    for p in [FRAC_PI_2, -FRAC_PI_2] {
        let q = Quaternion::from_world_euler(we(0.4, p, 0.3));
        let e = q.to_world_euler();
        assert!((e.pitch.0 - p).abs() < 1e-7, "pitch {} vs {p}", e.pitch.0);
        assert!(e.yaw.0.is_finite() && e.roll.0.is_finite());
        assert!(same_rotation(Quaternion::from_world_euler(e), q));
        // ASM-260: roll is 0 and yaw carries the combined rotation, 0.4 -/+ 0.3.
        let expect_yaw = if p > 0.0 { 0.4 - 0.3 } else { 0.4 + 0.3 };
        assert!(e.roll.0 == 0.0, "roll {}", e.roll.0);
        assert!(
            (e.yaw.0 - expect_yaw).abs() < 1e-9,
            "yaw {} vs {expect_yaw}",
            e.yaw.0
        );
    }
}

/// ASM-260: just short of the pole the roll is recovered, not discarded.
#[test]
fn near_pole_round_trip_keeps_roll() {
    let p = FRAC_PI_2 - 1.4e-6;
    let q = Quaternion::from_world_euler(we(0.4, p, 2.5));
    let e = q.to_world_euler();
    assert!((e.roll.0 - 2.5).abs() < 1e-8, "roll {}", e.roll.0);
    assert!((e.yaw.0 - 0.4).abs() < 1e-8, "yaw {}", e.yaw.0);
    assert!(same_rotation(Quaternion::from_world_euler(e), q));
}

/// ADR 0032: consistent with `LocalFrame::euler_from_orientation` at the equator and the prime
/// meridian: a body pointing east (NED heading 90 degrees), level, has ECEF yaw 90 degrees and
/// roll -90 degrees (the local down axis is -x there).
#[test]
fn consistent_with_local_euler_at_equator_prime_meridian() {
    let frame = LocalFrame::at(Geodetic::from_degrees(0.0, 0.0, 0.0).expect("valid"));
    let local = Euler {
        heading: Radians(FRAC_PI_2),
        pitch: Radians(0.0),
        roll: Radians(0.0),
    };
    let q = frame.orientation_from_euler(local);
    let w = q.to_world_euler();
    assert!((w.yaw.0 - FRAC_PI_2).abs() < EPS, "yaw {}", w.yaw.0);
    assert!(w.pitch.0.abs() < EPS, "pitch {}", w.pitch.0);
    assert!((w.roll.0 + FRAC_PI_2).abs() < EPS, "roll {}", w.roll.0);
    // And the local Euler angles come back from the same orientation.
    let back = frame.euler_from_orientation(q);
    assert!((back.heading.0 - FRAC_PI_2).abs() < EPS);
}

/// ADR 0032: level, east-pointing at lat 0 / lon 90 degrees east: forward is -x, down is -y.
#[test]
fn consistent_with_local_euler_at_lon_90() {
    let frame = LocalFrame::at(Geodetic::from_degrees(0.0, 90.0, 0.0).expect("valid"));
    let q = frame.orientation_from_euler(Euler {
        heading: Radians(FRAC_PI_2),
        pitch: Radians(0.0),
        roll: Radians(0.0),
    });
    // Heading east at lon 90: forward is -x, down is -y. Yaw 180, pitch 0, roll -90.
    let w = q.to_world_euler();
    assert!((w.yaw.0.abs() - PI).abs() < EPS, "yaw {}", w.yaw.0);
    assert!(w.pitch.0.abs() < EPS);
    assert!((w.roll.0 + FRAC_PI_2).abs() < EPS, "roll {}", w.roll.0);
}

/// ADR 0032: the angle between orientations.
#[test]
fn angle_between_is_zero_on_equal_and_known_on_a_turn() {
    let q = Quaternion::from_world_euler(we(0.3, 0.2, -0.1));
    assert!(q.angle_between(q).0.abs() < 1e-7);
    let r = Quaternion::from_world_euler(we(0.3 + 0.5, 0.2, -0.1));
    let a = Quaternion::from_world_euler(we(0.0, 0.0, 0.0))
        .angle_between(Quaternion::from_world_euler(we(0.5, 0.0, 0.0)));
    assert!((a.0 - 0.5).abs() < EPS, "angle {}", a.0);
    assert!(q.angle_between(r).0 > 0.0);
    // The double cover does not matter.
    let neg = Quaternion {
        w: -q.w,
        x: -q.x,
        y: -q.y,
        z: -q.z,
    };
    assert!(q.angle_between(neg).0 < 1e-7);
}

fn arb_unit() -> impl Strategy<Value = Quaternion> {
    (-1.0_f64..1.0, -1.0_f64..1.0, -1.0_f64..1.0, -1.0_f64..1.0)
        .prop_filter_map("non-zero", |(w, x, y, z)| {
            Quaternion { w, x, y, z }.normalized()
        })
}

proptest! {
    /// ADR 0032: `from(to(q))` is `q` up to sign.
    #[test]
    fn round_trip_up_to_sign(q in arb_unit()) {
        let back = Quaternion::from_world_euler(q.to_world_euler());
        prop_assert!(same_rotation(q, back), "{q:?} vs {back:?}");
    }

    /// ADR 0032: the angle between two orientations is symmetric.
    #[test]
    fn angle_between_symmetric(a in arb_unit(), b in arb_unit()) {
        prop_assert!((a.angle_between(b).0 - b.angle_between(a).0).abs() < 1e-12);
        prop_assert!(a.angle_between(a).0 < 1e-7);
    }
}
