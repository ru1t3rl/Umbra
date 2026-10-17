//! STUB (WI-3): vectors, quaternions and Euler helpers (REQ-C-3).
//!
//! Signatures only. Acceptance tests: `tests/geo.rs`, `tests/coords_prop.rs` (AT-3).
#![expect(
    missing_docs,
    clippy::todo,
    clippy::return_self_not_must_use,
    clippy::should_implement_trait,
    reason = "stub until WI-3: signatures only, documented when implemented"
)]

use core::ops::{Add, Div, Mul, Neg, Sub};

use crate::units::Radians;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Vec3 {
    pub const ZERO: Self = Self {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
    pub fn new(_x: f64, _y: f64, _z: f64) -> Self {
        todo!("WI-3")
    }
    pub fn dot(self, _o: Self) -> f64 {
        todo!("WI-3")
    }
    pub fn cross(self, _o: Self) -> Self {
        todo!("WI-3")
    }
    pub fn norm(self) -> f64 {
        todo!("WI-3")
    }
    pub fn normalized(self) -> Option<Self> {
        todo!("WI-3")
    }
}

impl Add for Vec3 {
    type Output = Vec3;
    fn add(self, _rhs: Vec3) -> Vec3 {
        todo!("WI-3")
    }
}
impl Sub for Vec3 {
    type Output = Vec3;
    fn sub(self, _rhs: Vec3) -> Vec3 {
        todo!("WI-3")
    }
}
impl Neg for Vec3 {
    type Output = Vec3;
    fn neg(self) -> Vec3 {
        todo!("WI-3")
    }
}
impl Mul<f64> for Vec3 {
    type Output = Vec3;
    fn mul(self, _rhs: f64) -> Vec3 {
        todo!("WI-3")
    }
}
impl Div<f64> for Vec3 {
    type Output = Vec3;
    fn div(self, _rhs: f64) -> Vec3 {
        todo!("WI-3")
    }
}

/// Unit quaternion (w, x, y, z), Hamilton convention, active rotation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quaternion {
    pub w: f64,
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Quaternion {
    pub const IDENTITY: Self = Self {
        w: 1.0,
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
    pub fn from_axis_angle(_axis: Vec3, _angle: Radians) -> Option<Self> {
        todo!("WI-3")
    }
    pub fn mul(self, _rhs: Self) -> Self {
        todo!("WI-3")
    }
    pub fn conjugate(self) -> Self {
        todo!("WI-3")
    }
    pub fn rotate(self, _v: Vec3) -> Vec3 {
        todo!("WI-3")
    }
    pub fn normalized(self) -> Option<Self> {
        todo!("WI-3")
    }
    pub fn from_rotation_matrix(_m: [[f64; 3]; 3]) -> Self {
        todo!("WI-3")
    }
}

/// Heading, pitch, roll relative to the local NED frame (ZYX).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Euler {
    pub heading: Radians,
    pub pitch: Radians,
    pub roll: Radians,
}
