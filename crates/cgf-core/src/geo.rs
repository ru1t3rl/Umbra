//! Vectors, unit quaternions and Euler helpers (REQ-C-3, ASM-033).
//!
//! These are frame-agnostic value types. The frame of a value (ECEF, ENU, NED, body) is stated by
//! the API that produces or consumes it (see [`crate::coords`]). All lengths are meters unless a
//! function says otherwise, all angles radians (REQ-C-4). Trigonometry goes through
//! [`crate::math`] (ADR 0003).
//!
//! Conventions (ASM-033):
//! - [`Quaternion`] is `(w, x, y, z)`, Hamilton product, **active** rotation: `q.rotate(v)` turns
//!   the vector `v` about the quaternion's axis by its angle (right-handed).
//! - Rotation matrices are row-major `[[f64; 3]; 3]`; the images of the basis vectors are the
//!   columns.
//! - [`Euler`] angles are relative to the local NED frame, applied in ZYX order: heading about
//!   down, then pitch about the new east axis, then roll about the new north axis (body forward).
//!   Heading is clockwise from true north, positive pitch raises the nose, positive roll lowers
//!   the right wing.

use core::ops::{Add, Div, Mul, Neg, Sub};

use crate::math;
use crate::units::Radians;

/// A 3-vector of `f64` components. The frame is defined by the producer (ECEF unless stated).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec3 {
    /// First component (ECEF: toward lon 0 on the equator), meters or the vector's own unit.
    pub x: f64,
    /// Second component (ECEF: toward lon 90 E on the equator).
    pub y: f64,
    /// Third component (ECEF: toward the north pole).
    pub z: f64,
}

impl Vec3 {
    /// The zero vector.
    pub const ZERO: Self = Self {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };

    /// Builds a vector from its components.
    #[must_use]
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    /// Dot product.
    #[must_use]
    pub fn dot(self, o: Self) -> f64 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }

    /// Right-handed cross product `self x o`.
    #[must_use]
    pub fn cross(self, o: Self) -> Self {
        Self {
            x: self.y * o.z - self.z * o.y,
            y: self.z * o.x - self.x * o.z,
            z: self.x * o.y - self.y * o.x,
        }
    }

    /// Euclidean length.
    #[must_use]
    pub fn norm(self) -> f64 {
        self.dot(self).sqrt()
    }

    /// The unit vector in the same direction, or `None` for a zero or non-finite vector.
    #[must_use]
    pub fn normalized(self) -> Option<Self> {
        let n = self.norm();
        (n > 0.0 && n.is_finite()).then(|| self / n)
    }
}

impl Add for Vec3 {
    type Output = Vec3;
    fn add(self, rhs: Vec3) -> Vec3 {
        Vec3::new(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z)
    }
}

impl Sub for Vec3 {
    type Output = Vec3;
    fn sub(self, rhs: Vec3) -> Vec3 {
        Vec3::new(self.x - rhs.x, self.y - rhs.y, self.z - rhs.z)
    }
}

impl Neg for Vec3 {
    type Output = Vec3;
    fn neg(self) -> Vec3 {
        Vec3::new(-self.x, -self.y, -self.z)
    }
}

impl Mul<f64> for Vec3 {
    type Output = Vec3;
    fn mul(self, rhs: f64) -> Vec3 {
        Vec3::new(self.x * rhs, self.y * rhs, self.z * rhs)
    }
}

impl Div<f64> for Vec3 {
    type Output = Vec3;
    fn div(self, rhs: f64) -> Vec3 {
        Vec3::new(self.x / rhs, self.y / rhs, self.z / rhs)
    }
}

/// Unit quaternion `(w, x, y, z)`, Hamilton convention, active rotation (ASM-033).
///
/// Constructors keep it normalised; fields are public for hashing and interchange, so code that
/// builds one by hand must keep it unit length (see [`Quaternion::normalized`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quaternion {
    /// Scalar part.
    pub w: f64,
    /// Vector part, x.
    pub x: f64,
    /// Vector part, y.
    pub y: f64,
    /// Vector part, z.
    pub z: f64,
}

impl Quaternion {
    /// The rotation that moves nothing.
    pub const IDENTITY: Self = Self {
        w: 1.0,
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };

    /// Rotation by `angle` (right-handed, active) about `axis`.
    ///
    /// The axis need not be unit length. `None` for a zero or non-finite axis.
    #[must_use]
    pub fn from_axis_angle(axis: Vec3, angle: Radians) -> Option<Self> {
        let u = axis.normalized()?;
        let (s, c) = math::sin_cos(angle.0 * 0.5);
        Some(Self {
            w: c,
            x: u.x * s,
            y: u.y * s,
            z: u.z * s,
        })
    }

    /// Hamilton product `self * rhs`: as a rotation, `rhs` is applied first, then `self`.
    #[expect(
        clippy::should_implement_trait,
        reason = "named method per the plan; the operator form would hide the application order"
    )]
    #[must_use]
    pub fn mul(self, rhs: Self) -> Self {
        Self {
            w: self.w * rhs.w - self.x * rhs.x - self.y * rhs.y - self.z * rhs.z,
            x: self.w * rhs.x + self.x * rhs.w + self.y * rhs.z - self.z * rhs.y,
            y: self.w * rhs.y - self.x * rhs.z + self.y * rhs.w + self.z * rhs.x,
            z: self.w * rhs.z + self.x * rhs.y - self.y * rhs.x + self.z * rhs.w,
        }
    }

    /// Conjugate; the inverse rotation for a unit quaternion.
    #[must_use]
    pub fn conjugate(self) -> Self {
        Self {
            w: self.w,
            x: -self.x,
            y: -self.y,
            z: -self.z,
        }
    }

    /// Rotates `v` (active rotation): `q v q*`.
    #[must_use]
    pub fn rotate(self, v: Vec3) -> Vec3 {
        let u = Vec3::new(self.x, self.y, self.z);
        let t = u.cross(v) * 2.0;
        v + t * self.w + u.cross(t)
    }

    /// The unit quaternion in the same direction, or `None` for a zero or non-finite one.
    #[must_use]
    pub fn normalized(self) -> Option<Self> {
        let n = (self.w * self.w + self.x * self.x + self.y * self.y + self.z * self.z).sqrt();
        (n > 0.0 && n.is_finite()).then(|| Self {
            w: self.w / n,
            x: self.x / n,
            y: self.y / n,
            z: self.z / n,
        })
    }

    /// Converts a rotation matrix (row-major, basis images as columns) to a quaternion.
    ///
    /// Branches on the largest of the trace and the diagonal elements for numerical stability
    /// (Shepperd's method). The input is assumed to be a proper rotation matrix.
    #[must_use]
    pub fn from_rotation_matrix(m: [[f64; 3]; 3]) -> Self {
        let trace = m[0][0] + m[1][1] + m[2][2];
        let q = if trace > m[0][0].max(m[1][1]).max(m[2][2]) {
            let s = (trace + 1.0).sqrt() * 2.0;
            Self {
                w: 0.25 * s,
                x: (m[2][1] - m[1][2]) / s,
                y: (m[0][2] - m[2][0]) / s,
                z: (m[1][0] - m[0][1]) / s,
            }
        } else if m[0][0] >= m[1][1] && m[0][0] >= m[2][2] {
            let s = (1.0 + m[0][0] - m[1][1] - m[2][2]).sqrt() * 2.0;
            Self {
                w: (m[2][1] - m[1][2]) / s,
                x: 0.25 * s,
                y: (m[0][1] + m[1][0]) / s,
                z: (m[0][2] + m[2][0]) / s,
            }
        } else if m[1][1] >= m[2][2] {
            let s = (1.0 + m[1][1] - m[0][0] - m[2][2]).sqrt() * 2.0;
            Self {
                w: (m[0][2] - m[2][0]) / s,
                x: (m[0][1] + m[1][0]) / s,
                y: 0.25 * s,
                z: (m[1][2] + m[2][1]) / s,
            }
        } else {
            let s = (1.0 + m[2][2] - m[0][0] - m[1][1]).sqrt() * 2.0;
            Self {
                w: (m[1][0] - m[0][1]) / s,
                x: (m[0][2] + m[2][0]) / s,
                y: (m[1][2] + m[2][1]) / s,
                z: 0.25 * s,
            }
        };
        q.normalized().unwrap_or(q)
    }
}

/// Heading, pitch and roll relative to the local NED frame, ZYX order (ASM-033).
///
/// Heading is clockwise from true north, positive pitch raises the nose, positive roll lowers
/// the right wing. This is a helper for input and display; the canonical orientation is the
/// [`Quaternion`]. Pitch at +-90 degrees is a gimbal lock: heading and roll are then not
/// separately recoverable.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Euler {
    /// Rotation about down, clockwise from true north.
    pub heading: Radians,
    /// Rotation about the east axis after heading, positive nose up.
    pub pitch: Radians,
    /// Rotation about the body forward axis after pitch, positive right wing down.
    pub roll: Radians,
}

impl Euler {
    /// The rotation taking body FRD (forward, right, down) axes to NED axes.
    #[must_use]
    pub fn to_quaternion(self) -> Quaternion {
        let (sh, ch) = math::sin_cos(self.heading.0 * 0.5);
        let (sp, cp) = math::sin_cos(self.pitch.0 * 0.5);
        let (sr, cr) = math::sin_cos(self.roll.0 * 0.5);
        Quaternion {
            w: cr * cp * ch + sr * sp * sh,
            x: sr * cp * ch - cr * sp * sh,
            y: cr * sp * ch + sr * cp * sh,
            z: cr * cp * sh - sr * sp * ch,
        }
    }

    /// Recovers the Euler angles from a body FRD to NED rotation.
    ///
    /// Heading and roll are in `(-pi, pi]`, pitch in `[-pi/2, pi/2]`.
    #[must_use]
    pub fn from_quaternion(q: Quaternion) -> Self {
        let roll = math::atan2(
            2.0 * (q.w * q.x + q.y * q.z),
            1.0 - 2.0 * (q.x * q.x + q.y * q.y),
        );
        let pitch = math::asin((2.0 * (q.w * q.y - q.z * q.x)).clamp(-1.0, 1.0));
        let heading = math::atan2(
            2.0 * (q.w * q.z + q.x * q.y),
            1.0 - 2.0 * (q.y * q.y + q.z * q.z),
        );
        Self {
            heading: Radians(heading),
            pitch: Radians(pitch),
            roll: Radians(roll),
        }
    }
}
