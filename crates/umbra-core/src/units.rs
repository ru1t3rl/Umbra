//! SI unit newtypes (REQ-C-4, ASM-015).
//!
//! Each unit is a public-field `f64` newtype. Only `+ - * /` and `to_radians`/`to_degrees`
//! (a single multiplication each) are used, so results are bit-reproducible (ADR 0003).
//! Degrees exist only as conversion functions; the stored value is always SI.
//!
//! Values are not validated here (ASM-015, REQ-C-4): NaN and infinity are rejected at the edges
//! (templates, network, API), not inside the types. The derived `PartialEq` and `PartialOrd` follow
//! `f64`: `NaN != NaN` and a NaN compares as `None`.
//!
//! Mixing units does not compile:
//!
//! ```compile_fail
//! use umbra_core::{Meters, Seconds};
//! let _ = Meters(1.0) + Seconds(1.0);
//! ```
//!
//! ```compile_fail
//! use umbra_core::Seconds;
//! let _ = Seconds(1.0) * Seconds(1.0);
//! ```
//!
//! ```compile_fail
//! use umbra_core::Meters;
//! let _ = Meters(1.0) * Meters(1.0);
//! ```

use core::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};

macro_rules! unit {
    ($($(#[$doc:meta])* $name:ident),* $(,)?) => {$(
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
        pub struct $name(pub f64);
        impl Add for $name {
            type Output = $name;
            fn add(self, rhs: $name) -> $name { $name(self.0 + rhs.0) }
        }
        impl Sub for $name {
            type Output = $name;
            fn sub(self, rhs: $name) -> $name { $name(self.0 - rhs.0) }
        }
        impl Neg for $name {
            type Output = $name;
            fn neg(self) -> $name { $name(-self.0) }
        }
        impl Mul<f64> for $name {
            type Output = $name;
            fn mul(self, rhs: f64) -> $name { $name(self.0 * rhs) }
        }
        impl Div<f64> for $name {
            type Output = $name;
            fn div(self, rhs: f64) -> $name { $name(self.0 / rhs) }
        }
        impl Div<$name> for $name {
            type Output = f64;
            fn div(self, rhs: $name) -> f64 { self.0 / rhs.0 }
        }
        impl AddAssign for $name {
            fn add_assign(&mut self, rhs: $name) { self.0 += rhs.0; }
        }
        impl SubAssign for $name {
            fn sub_assign(&mut self, rhs: $name) { self.0 -= rhs.0; }
        }
    )*};
}

unit!(
    /// Length in meters.
    Meters,
    /// Duration in seconds.
    Seconds,
    /// Angle in radians.
    Radians,
    /// Mass in kilograms.
    Kilograms,
    /// Speed in meters per second.
    MetersPerSecond,
    /// Acceleration in meters per second squared.
    MetersPerSecondSquared,
    /// Angular rate in radians per second.
    RadiansPerSecond,
);

/// Defines `rate * Seconds = quantity` and `quantity / Seconds = rate`.
macro_rules! time_relation {
    ($rate:ident, $qty:ident) => {
        impl Mul<Seconds> for $rate {
            type Output = $qty;
            fn mul(self, rhs: Seconds) -> $qty {
                $qty(self.0 * rhs.0)
            }
        }
        impl Div<Seconds> for $qty {
            type Output = $rate;
            fn div(self, rhs: Seconds) -> $rate {
                $rate(self.0 / rhs.0)
            }
        }
    };
}
time_relation!(MetersPerSecond, Meters);
time_relation!(MetersPerSecondSquared, MetersPerSecond);
time_relation!(RadiansPerSecond, Radians);

impl Radians {
    /// Converts degrees to radians (`deg * (PI / 180)`, one multiplication; deterministic).
    pub fn from_degrees(deg: f64) -> Self {
        Self(deg.to_radians())
    }

    /// Converts to degrees (`rad * (180 / PI)`, one multiplication; deterministic).
    pub fn to_degrees(self) -> f64 {
        self.0.to_degrees()
    }
}

impl RadiansPerSecond {
    /// Converts degrees per second to radians per second.
    pub fn from_degrees_per_second(dps: f64) -> Self {
        Self(dps.to_radians())
    }

    /// Converts to degrees per second.
    pub fn to_degrees_per_second(self) -> f64 {
        self.0.to_degrees()
    }
}
