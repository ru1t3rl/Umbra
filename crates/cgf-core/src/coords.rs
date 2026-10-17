//! STUB (WI-4): WGS-84, ECEF, geodetic, ENU/NED and `LocalFrame` (REQ-C-3, D-15, ASM-033).
//!
//! Signatures only; the WGS-84 constants are NaN until WI-4. Acceptance tests:
//! `tests/coords.rs`, `tests/coords_prop.rs` (AT-3).
#![expect(
    missing_docs,
    clippy::todo,
    clippy::return_self_not_must_use,
    reason = "stub until WI-4: signatures only, documented when implemented"
)]

use core::ops::{Add, Sub};

use crate::geo::{Euler, Quaternion, Vec3};
use crate::units::{Meters, Radians};

pub mod wgs84 {
    pub const A: f64 = f64::NAN;
    pub const INV_F: f64 = f64::NAN;
    pub const F: f64 = f64::NAN;
    pub const B: f64 = f64::NAN;
    pub const E2: f64 = f64::NAN;
    pub const EP2: f64 = f64::NAN;
}

/// Canonical position: Earth-centred, Earth-fixed, meters (D-15).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ecef(pub Vec3);

impl Ecef {
    pub fn to_geodetic(self) -> Geodetic {
        todo!("WI-4")
    }
    pub fn distance(self, _o: Self) -> Meters {
        todo!("WI-4")
    }
}
impl Sub for Ecef {
    type Output = Vec3;
    fn sub(self, _rhs: Ecef) -> Vec3 {
        todo!("WI-4")
    }
}
impl Add<Vec3> for Ecef {
    type Output = Ecef;
    fn add(self, _rhs: Vec3) -> Ecef {
        todo!("WI-4")
    }
}

/// WGS-84 geodetic position (private fields: validated on construction).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Geodetic {
    lat: Radians,
    lon: Radians,
    height: Meters,
}

impl Geodetic {
    pub fn new(_lat: Radians, _lon: Radians, _height: Meters) -> Result<Self, CoordError> {
        todo!("WI-4")
    }
    pub fn from_degrees(_lat_deg: f64, _lon_deg: f64, _height_m: f64) -> Result<Self, CoordError> {
        todo!("WI-4")
    }
    pub fn lat(self) -> Radians {
        self.lat
    }
    pub fn lon(self) -> Radians {
        self.lon
    }
    pub fn height(self) -> Meters {
        self.height
    }
    pub fn with_height(self, _h: Meters) -> Self {
        todo!("WI-4")
    }
    pub fn to_ecef(self) -> Ecef {
        todo!("WI-4")
    }
}

/// User-facing position, converted to ECEF once at the API edge (ASM-044).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Position {
    Ecef(Ecef),
    Geodetic(Geodetic),
}

impl Position {
    pub fn to_ecef(self) -> Ecef {
        todo!("WI-4")
    }
}
impl From<Ecef> for Position {
    fn from(_e: Ecef) -> Self {
        todo!("WI-4")
    }
}
impl From<Geodetic> for Position {
    fn from(_g: Geodetic) -> Self {
        todo!("WI-4")
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Enu {
    pub east: f64,
    pub north: f64,
    pub up: f64,
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ned {
    pub north: f64,
    pub east: f64,
    pub down: f64,
}
impl From<Enu> for Ned {
    fn from(_e: Enu) -> Ned {
        todo!("WI-4")
    }
}
impl From<Ned> for Enu {
    fn from(_n: Ned) -> Enu {
        todo!("WI-4")
    }
}

/// Local tangent plane at an origin.
#[derive(Debug, Clone, PartialEq)]
pub struct LocalFrame {
    _private: (),
}

impl LocalFrame {
    pub fn at(_origin: Geodetic) -> Self {
        todo!("WI-4")
    }
    pub fn origin(&self) -> Ecef {
        todo!("WI-4")
    }
    pub fn to_enu(&self, _p: Ecef) -> Enu {
        todo!("WI-4")
    }
    pub fn from_enu(&self, _e: Enu) -> Ecef {
        todo!("WI-4")
    }
    pub fn to_ned(&self, _p: Ecef) -> Ned {
        todo!("WI-4")
    }
    pub fn from_ned(&self, _n: Ned) -> Ecef {
        todo!("WI-4")
    }
    pub fn vector_to_enu(&self, _v: Vec3) -> Enu {
        todo!("WI-4")
    }
    pub fn vector_from_enu(&self, _e: Enu) -> Vec3 {
        todo!("WI-4")
    }
    pub fn orientation_from_euler(&self, _e: Euler) -> Quaternion {
        todo!("WI-4")
    }
    pub fn euler_from_orientation(&self, _q: Quaternion) -> Euler {
        todo!("WI-4")
    }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum CoordError {
    #[error("latitude {0} rad outside [-pi/2, pi/2]")]
    LatitudeOutOfRange(f64),
    #[error("non-finite coordinate")]
    NonFinite,
}
