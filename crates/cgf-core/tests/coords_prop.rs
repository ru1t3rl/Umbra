//! AT-3 (property part): coordinate round trips and rotation invariants.
//! Requirements: REQ-C-3, section 15 (coordinate property tests).
//!
//! Tolerances are the plan's (AT-3). They are stricter than the closed-form method's published
//! accuracy in the worst cases, so WI-4 may add one fixed Newton correction step (plan, coords).

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "strategies only produce valid inputs")]

use core::f64::consts::PI;

use cgf_core::{Ecef, Enu, Euler, Geodetic, LocalFrame, Meters, Ned, Quaternion, Radians, Vec3};
use proptest::prelude::*;

fn geo(lat_deg: f64, lon_deg: f64, h: f64) -> Geodetic {
    Geodetic::from_degrees(lat_deg, lon_deg, h).expect("valid geodetic")
}

/// Smallest absolute difference between two angles, modulo a full turn.
fn angle_diff(a: f64, b: f64) -> f64 {
    let d = (a - b).abs();
    d.min(2.0 * PI - d)
}

proptest! {
    /// REQ-C-3: geodetic -> ECEF -> geodetic -> ECEF reproduces the point (plan tolerances).
    #[test]
    fn geodetic_ecef_round_trip(
        lat in -90.0_f64..=90.0,
        lon in -179.999_999_f64..=180.0,
        h in -12_000.0_f64..1_000_000.0,
    ) {
        let g = geo(lat, lon, h);
        let e = g.to_ecef();

        let back = e.to_geodetic();
        let e2 = back.to_ecef();

        prop_assert!((e2.0 - e.0).norm() < 1e-6, "ecef {:?} vs {:?}", e.0, e2.0);
        prop_assert!((back.height().0 - h).abs() < 1e-6, "h {} vs {h}", back.height().0);
        prop_assert!((back.lat().0 - g.lat().0).abs() < 1e-11, "lat");
        if lat.abs() < 89.9 {
            prop_assert!(angle_diff(back.lon().0, g.lon().0) < 1e-11, "lon");
        }
    }

    /// REQ-C-3: ECEF -> geodetic -> ECEF for any point with a radius from 6000 to 7400 km.
    #[test]
    fn ecef_geodetic_round_trip(
        dx in -1.0_f64..=1.0,
        dy in -1.0_f64..=1.0,
        dz in -1.0_f64..=1.0,
        r in 6.0e6_f64..7.4e6,
    ) {
        let dir = Vec3 { x: dx, y: dy, z: dz };
        prop_assume!(dir.norm() > 1e-3);
        let unit = dir.normalized().expect("non-zero");
        let p = Ecef(unit * r);

        let back = p.to_geodetic().to_ecef();

        prop_assert!((back.0 - p.0).norm() < 1e-6, "{:?} vs {:?}", p.0, back.0);
    }

    /// REQ-C-3: ENU conversion is invertible within 2000 km of the origin (1e-6 m).
    #[test]
    fn enu_round_trip_within_2000_km(
        lat in -89.0_f64..=89.0,
        lon in -180.0_f64..=180.0,
        h0 in -500.0_f64..9_000.0,
        east in -1.1e6_f64..1.1e6,
        north in -1.1e6_f64..1.1e6,
        up in -1.1e5_f64..1.1e5,
    ) {
        let frame = LocalFrame::at(geo(lat, lon, h0));
        let enu = Enu { east, north, up };

        let p = frame.from_enu(enu);
        let back = frame.to_enu(p);
        let again = frame.from_enu(back);

        prop_assert!((back.east - east).abs() < 1e-6, "east");
        prop_assert!((back.north - north).abs() < 1e-6, "north");
        prop_assert!((back.up - up).abs() < 1e-6, "up");
        prop_assert!((again.0 - p.0).norm() < 1e-6, "ecef");
    }

    /// REQ-C-3: NED <-> ENU is an exact axis swap, so the round trip is bit-exact.
    #[test]
    fn ned_enu_round_trip_is_exact(n in -1.0e7_f64..1.0e7, e in -1.0e7_f64..1.0e7, d in -1.0e7_f64..1.0e7) {
        let ned = Ned { north: n, east: e, down: d };

        prop_assert_eq!(Ned::from(Enu::from(ned)), ned);
        let enu = Enu { east: e, north: n, up: d };
        prop_assert_eq!(Enu::from(Ned::from(enu)), enu);
    }

    /// REQ-C-3: Euler -> quaternion -> Euler round trip away from gimbal lock (1e-9 rad), and the
    /// quaternion is a unit quaternion.
    #[test]
    fn euler_quaternion_round_trip(
        lat in -85.0_f64..=85.0,
        lon in -180.0_f64..=180.0,
        heading in -3.1_f64..3.1,
        pitch in -89.0_f64..89.0,
        roll in -3.1_f64..3.1,
    ) {
        let frame = LocalFrame::at(geo(lat, lon, 0.0));
        let e = Euler { heading: Radians(heading), pitch: Radians::from_degrees(pitch), roll: Radians(roll) };

        let q = frame.orientation_from_euler(e);
        let got = frame.euler_from_orientation(q);

        let norm = (q.w * q.w + q.x * q.x + q.y * q.y + q.z * q.z).sqrt();
        prop_assert!((norm - 1.0).abs() < 1e-12, "norm {norm}");
        prop_assert!((got.heading.0 - e.heading.0).abs() < 1e-9, "heading {} vs {}", got.heading.0, e.heading.0);
        prop_assert!((got.pitch.0 - e.pitch.0).abs() < 1e-9, "pitch");
        prop_assert!((got.roll.0 - e.roll.0).abs() < 1e-9, "roll");
    }

    /// REQ-C-3: rotation by a unit quaternion preserves vector length (relative 1e-9).
    #[test]
    fn rotation_preserves_length(
        ax in -1.0_f64..=1.0, ay in -1.0_f64..=1.0, az in -1.0_f64..=1.0,
        angle in -6.3_f64..6.3,
        vx in -1.0e7_f64..1.0e7, vy in -1.0e7_f64..1.0e7, vz in -1.0e7_f64..1.0e7,
    ) {
        let axis = Vec3 { x: ax, y: ay, z: az };
        prop_assume!(axis.norm() > 1e-3);
        let q = Quaternion::from_axis_angle(axis, Radians(angle)).expect("non-zero axis");
        let v = Vec3 { x: vx, y: vy, z: vz };

        let r = q.rotate(v);

        prop_assert!((r.norm() - v.norm()).abs() <= 1e-9 * v.norm().max(1.0));
    }

    /// REQ-C-3: a rotation matrix built from a quaternion converts back to the same rotation.
    #[test]
    fn quaternion_rotation_matrix_round_trip(
        ax in -1.0_f64..=1.0, ay in -1.0_f64..=1.0, az in -1.0_f64..=1.0,
        angle in -3.0_f64..3.0,
    ) {
        let axis = Vec3 { x: ax, y: ay, z: az };
        prop_assume!(axis.norm() > 1e-3);
        let q = Quaternion::from_axis_angle(axis, Radians(angle)).expect("non-zero axis");
        let cols = [
            q.rotate(Vec3 { x: 1.0, y: 0.0, z: 0.0 }),
            q.rotate(Vec3 { x: 0.0, y: 1.0, z: 0.0 }),
            q.rotate(Vec3 { x: 0.0, y: 0.0, z: 1.0 }),
        ];
        let m = [
            [cols[0].x, cols[1].x, cols[2].x],
            [cols[0].y, cols[1].y, cols[2].y],
            [cols[0].z, cols[1].z, cols[2].z],
        ];

        let q2 = Quaternion::from_rotation_matrix(m);

        let probe = Vec3 { x: 0.3, y: -0.7, z: 0.2 };
        prop_assert!((q2.rotate(probe) - q.rotate(probe)).norm() < 1e-9);
    }

    /// REQ-C-3, REQ-C-4: height is carried through `with_height` and the ECEF round trip for
    /// the whole supported range.
    #[test]
    fn height_round_trip_through_ecef(h in -12_000.0_f64..1_000_000.0, lat in -90.0_f64..=90.0) {
        let g = geo(lat, 10.0, 0.0).with_height(Meters(h));

        prop_assert!((g.to_ecef().to_geodetic().height().0 - h).abs() < 1e-6);
    }
}
