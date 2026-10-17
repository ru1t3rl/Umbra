//! The single site for non-exact floating-point functions (ADR 0003, ASM-034, REQ-K-4).
//!
//! `+ - * /`, `sqrt`, `abs`, `floor` and `to_radians` are IEEE-exact and may be used anywhere.
//! Everything else (trigonometry, exponentials, logarithms, powers, `cbrt`, `hypot`) can differ in
//! the last bit between platform libms, so it is called only from here. `clippy.toml` bans the
//! direct `f64`/`f32` methods workspace-wide, which keeps this module the one swap point for a
//! pure-Rust `libm` should cross-OS determinism ever be required (ADR 0003, option 2).
//!
//! Today every wrapper is the matching `std` function. All angles are radians.

#![expect(
    clippy::disallowed_methods,
    reason = "ADR 0003: the only sanctioned call site of the banned f64 methods"
)]

use core::f64::consts::{PI, TAU};

use crate::units::Radians;

/// Sine of `x` (radians).
#[inline]
pub fn sin(x: f64) -> f64 {
    x.sin()
}

/// Cosine of `x` (radians).
#[inline]
pub fn cos(x: f64) -> f64 {
    x.cos()
}

/// Sine and cosine of `x` (radians), as `(sin, cos)`.
#[inline]
pub fn sin_cos(x: f64) -> (f64, f64) {
    x.sin_cos()
}

/// Tangent of `x` (radians).
#[inline]
pub fn tan(x: f64) -> f64 {
    x.tan()
}

/// Arcsine, result in radians within `[-pi/2, pi/2]`; NaN outside `[-1, 1]`.
#[inline]
pub fn asin(x: f64) -> f64 {
    x.asin()
}

/// Arccosine, result in radians within `[0, pi]`; NaN outside `[-1, 1]`.
#[inline]
pub fn acos(x: f64) -> f64 {
    x.acos()
}

/// Arctangent, result in radians within `(-pi/2, pi/2)`.
#[inline]
pub fn atan(x: f64) -> f64 {
    x.atan()
}

/// Four-quadrant arctangent of `y / x`, result in radians within `[-pi, pi]`.
#[inline]
pub fn atan2(y: f64, x: f64) -> f64 {
    y.atan2(x)
}

/// Length of the hypotenuse, `sqrt(x^2 + y^2)`, without undue overflow.
#[inline]
pub fn hypot(x: f64, y: f64) -> f64 {
    x.hypot(y)
}

/// Cube root.
#[inline]
pub fn cbrt(x: f64) -> f64 {
    x.cbrt()
}

/// `x` raised to the integer power `n`.
#[inline]
pub fn powi(x: f64, n: i32) -> f64 {
    x.powi(n)
}

/// `x` raised to the real power `y`.
#[inline]
pub fn powf(x: f64, y: f64) -> f64 {
    x.powf(y)
}

/// `e^x`.
#[inline]
pub fn exp(x: f64) -> f64 {
    x.exp()
}

/// Natural logarithm.
#[inline]
pub fn ln(x: f64) -> f64 {
    x.ln()
}

/// Wraps an angle to the half-open interval `(-PI, PI]`.
///
/// Angles already inside the interval are returned unchanged (bit for bit). Uses exact
/// operations only (`+ - * /` and `floor`), no transcendental function. NaN and infinities give
/// NaN.
pub fn wrap_pi(a: Radians) -> Radians {
    let x = a.0;
    if x > -PI && x <= PI {
        return a;
    }
    // [-PI, PI) by floor; rounding can leave the edges, so fix them up.
    let mut r = x - TAU * ((x + PI) / TAU).floor();
    if r <= -PI {
        r += TAU;
    }
    if r > PI {
        r = PI;
    }
    Radians(r)
}
