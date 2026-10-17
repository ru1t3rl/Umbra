//! AT-3 (vector, quaternion and identity parts): `cgf_core::geo`. Requirement: REQ-C-3.
//!
//! Conventions under test (ASM-033): Hamilton quaternion `(w, x, y, z)`, active rotation;
//! matrices are row-major with the images of the basis vectors as columns.

#![forbid(unsafe_code)]

use core::f64::consts::FRAC_PI_2;

use cgf_core::{Quaternion, Radians, Vec3};

const EPS: f64 = 1e-12;

fn v(x: f64, y: f64, z: f64) -> Vec3 {
    Vec3 { x, y, z }
}

fn near(a: Vec3, b: Vec3) -> bool {
    (a.x - b.x).abs() < EPS && (a.y - b.y).abs() < EPS && (a.z - b.z).abs() < EPS
}

/// REQ-C-3: `Vec3::new` stores x, y, z in order.
#[test]
fn vec3_new_stores_components_in_order() {
    assert_eq!(Vec3::new(1.0, 2.0, 3.0), v(1.0, 2.0, 3.0));
    assert_eq!(Vec3::ZERO, v(0.0, 0.0, 0.0));
}

/// REQ-C-3: component-wise operators.
#[test]
fn vec3_operators_are_component_wise() {
    let a = v(1.0, 2.0, 3.0);
    let b = v(4.0, -5.0, 6.0);

    assert_eq!(a + b, v(5.0, -3.0, 9.0));
    assert_eq!(a - b, v(-3.0, 7.0, -3.0));
    assert_eq!(-a, v(-1.0, -2.0, -3.0));
    assert_eq!(a * 2.0, v(2.0, 4.0, 6.0));
    assert_eq!(a / 2.0, v(0.5, 1.0, 1.5));
}

/// REQ-C-3: dot product.
#[test]
fn vec3_dot_of_orthogonal_axes_is_zero_and_of_self_is_norm_squared() {
    assert_eq!(v(1.0, 0.0, 0.0).dot(v(0.0, 1.0, 0.0)), 0.0);
    assert_eq!(v(1.0, 2.0, 3.0).dot(v(1.0, 2.0, 3.0)), 14.0);
}

/// REQ-C-3: right-handed cross product.
#[test]
fn vec3_cross_is_right_handed() {
    let x = v(1.0, 0.0, 0.0);
    let y = v(0.0, 1.0, 0.0);
    let z = v(0.0, 0.0, 1.0);

    assert_eq!(x.cross(y), z);
    assert_eq!(y.cross(z), x);
    assert_eq!(z.cross(x), y);
    assert_eq!(y.cross(x), -z);
}

/// REQ-C-3: Euclidean norm (3-4-5 triangle is exact).
#[test]
fn vec3_norm_of_3_4_0_is_5() {
    assert_eq!(v(3.0, 4.0, 0.0).norm(), 5.0);
}

/// REQ-C-3: normalising the zero vector is `None`, otherwise the result has unit length.
#[test]
fn vec3_normalized_is_none_for_zero_and_unit_otherwise() {
    assert_eq!(Vec3::ZERO.normalized(), None);
    let n = v(0.0, 3.0, 4.0).normalized();
    assert!(n.is_some_and(|n| near(n, v(0.0, 0.6, 0.8))), "{n:?}");
}

/// REQ-C-3 (convention): the identity quaternion rotates nothing.
#[test]
fn identity_quaternion_rotates_nothing() {
    let p = v(1.0, -2.0, 3.0);

    assert_eq!(Quaternion::IDENTITY.rotate(p), p);
    assert_eq!(
        Quaternion::IDENTITY,
        Quaternion {
            w: 1.0,
            x: 0.0,
            y: 0.0,
            z: 0.0
        }
    );
}

/// REQ-C-3 (convention): active rotation, +90 degrees about +z maps +x to +y.
#[test]
fn quarter_turn_about_z_maps_x_to_y() {
    let q = Quaternion::from_axis_angle(v(0.0, 0.0, 1.0), Radians(FRAC_PI_2));

    assert!(q.is_some_and(|q| near(q.rotate(v(1.0, 0.0, 0.0)), v(0.0, 1.0, 0.0))));
}

/// REQ-C-3: a zero axis has no rotation; a non-unit axis is normalised, not rejected.
#[test]
fn from_axis_angle_rejects_zero_axis_and_normalises_others() {
    assert_eq!(Quaternion::from_axis_angle(Vec3::ZERO, Radians(1.0)), None);

    let q = Quaternion::from_axis_angle(v(0.0, 0.0, 5.0), Radians(FRAC_PI_2));
    assert!(q.is_some_and(|q| near(q.rotate(v(1.0, 0.0, 0.0)), v(0.0, 1.0, 0.0))));
}

/// REQ-C-3: `a.mul(b)` applies `b` first, then `a` (Hamilton product, active rotations).
#[test]
fn quaternion_mul_applies_right_hand_side_first() {
    let about_z = Quaternion::from_axis_angle(v(0.0, 0.0, 1.0), Radians(FRAC_PI_2));
    let about_x = Quaternion::from_axis_angle(v(1.0, 0.0, 0.0), Radians(FRAC_PI_2));
    let p = v(0.0, 1.0, 0.0);

    let (Some(qz), Some(qx)) = (about_z, about_x) else {
        panic!("axes are non-zero");
    };

    assert!(near(qz.mul(qx).rotate(p), qz.rotate(qx.rotate(p))));
    // x first: y -> z; then z-turn leaves z fixed.
    assert!(near(qz.mul(qx).rotate(p), v(0.0, 0.0, 1.0)));
}

/// REQ-C-3: the conjugate of a unit quaternion is its inverse.
#[test]
fn conjugate_undoes_the_rotation() {
    let q = Quaternion::from_axis_angle(v(1.0, 2.0, 3.0), Radians(0.7));
    let p = v(0.3, -0.4, 0.5);

    assert!(q.is_some_and(|q| near(q.conjugate().rotate(q.rotate(p)), p)));
}

/// REQ-C-3: `normalized` returns a unit quaternion, or `None` for the zero quaternion.
#[test]
fn quaternion_normalized_is_unit_or_none() {
    let zero = Quaternion {
        w: 0.0,
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
    let q = Quaternion {
        w: 2.0,
        x: 0.0,
        y: 0.0,
        z: 0.0,
    }
    .normalized();

    assert_eq!(zero.normalized(), None);
    assert_eq!(q, Some(Quaternion::IDENTITY));
}

/// REQ-C-3: a rotation matrix with the basis images as columns converts to the same rotation.
#[test]
fn from_rotation_matrix_of_quarter_turn_about_z_maps_x_to_y() {
    // Columns: x -> y, y -> -x, z -> z.
    let m = [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]];

    let q = Quaternion::from_rotation_matrix(m);

    assert!(near(q.rotate(v(1.0, 0.0, 0.0)), v(0.0, 1.0, 0.0)));
    assert!(near(q.rotate(v(0.0, 1.0, 0.0)), v(-1.0, 0.0, 0.0)));
    assert!(near(q.rotate(v(0.0, 0.0, 1.0)), v(0.0, 0.0, 1.0)));
}

/// REQ-C-3: the identity matrix converts to a rotation that moves nothing.
#[test]
fn from_rotation_matrix_of_identity_is_a_no_op_rotation() {
    let q = Quaternion::from_rotation_matrix([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);

    assert!(near(q.rotate(v(1.0, 2.0, 3.0)), v(1.0, 2.0, 3.0)));
}

/// REQ-C-3: a half turn exercises the trace-negative branch of the matrix conversion.
#[test]
fn from_rotation_matrix_of_half_turn_about_x_flips_y_and_z() {
    let m = [[1.0, 0.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, -1.0]];

    let q = Quaternion::from_rotation_matrix(m);

    assert!(near(q.rotate(v(0.0, 1.0, 0.0)), v(0.0, -1.0, 0.0)));
    assert!(near(q.rotate(v(0.0, 0.0, 1.0)), v(0.0, 0.0, -1.0)));
}

/// ASM-033: Euler angles are reported in `(-pi, pi]`: a heading or roll of exactly `-pi` comes back
/// as `+pi`.
#[test]
fn euler_heading_and_roll_wrap_minus_pi_to_pi() {
    use core::f64::consts::PI;

    use cgf_core::Euler;

    for (heading, roll) in [(-PI, 0.0), (0.0, -PI)] {
        let q = Euler {
            heading: Radians(heading),
            pitch: Radians(0.0),
            roll: Radians(roll),
        }
        .to_quaternion();

        let back = Euler::from_quaternion(q);

        if heading != 0.0 {
            assert_eq!(back.heading.0, PI, "heading {heading}");
        } else {
            assert_eq!(back.roll.0, PI, "roll {roll}");
        }
    }
}
