//! Deterministic float functions for plugins (ADR 0059, ADR 0003, REQ-K-4): the plugin-side mirror of
//! `umbra_core::math`.
//!
//! `+ - * /`, `sqrt`, `abs`, `floor` and `to_radians` are IEEE-exact and may be used anywhere.
//! Everything else (trigonometry, exponentials, logarithms, powers, `cbrt`, `hypot`) can differ in
//! the last bit between platform libms, so a plugin calls it only through this module. The workspace
//! `clippy.toml` bans the direct `f64` methods, and plugin authors outside the workspace should follow
//! the same rule.
//!
//! The functions have the names, semantics and implementations of `umbra_core::math` (`wrap_pi` takes
//! and returns plain `f64` radians, because the SDK has no `Radians`), so a plugin gets the engine's
//! results bit for bit on the same OS (A-2). The two modules are the two swap points of ADR 0003, one
//! per side of the ABI; a `libm` switch changes both in one commit, and the parity test
//! `umbra-plugin-host/tests/sdk_math_parity.rs` enforces it.
//!
//! This file is self-contained: no `crate::` or `super::` paths, no `cfg(feature)` and no unit tests,
//! because the host's parity test compiles it from source with `#[path]` next to the core module
//! (ASM-713). All angles are radians.

#![expect(
    clippy::disallowed_methods,
    reason = "ADR 0003, ADR 0059: SDK mirror of umbra_core::math"
)]

use core::f64::consts::{PI, TAU};

/// Sine of `x` (radians).
#[inline]
#[must_use]
pub fn sin(x: f64) -> f64 {
    x.sin()
}

/// Cosine of `x` (radians).
#[inline]
#[must_use]
pub fn cos(x: f64) -> f64 {
    x.cos()
}

/// Sine and cosine of `x` (radians), as `(sin, cos)`.
#[inline]
#[must_use]
pub fn sin_cos(x: f64) -> (f64, f64) {
    x.sin_cos()
}

/// Tangent of `x` (radians).
#[inline]
#[must_use]
pub fn tan(x: f64) -> f64 {
    x.tan()
}

/// Arcsine, result in radians within `[-pi/2, pi/2]`; NaN outside `[-1, 1]`.
#[inline]
#[must_use]
pub fn asin(x: f64) -> f64 {
    x.asin()
}

/// Arccosine, result in radians within `[0, pi]`; NaN outside `[-1, 1]`.
#[inline]
#[must_use]
pub fn acos(x: f64) -> f64 {
    x.acos()
}

/// Arctangent, result in radians within `(-pi/2, pi/2)`.
#[inline]
#[must_use]
pub fn atan(x: f64) -> f64 {
    x.atan()
}

/// Four-quadrant arctangent of `y / x`, result in radians within `[-pi, pi]`.
#[inline]
#[must_use]
pub fn atan2(y: f64, x: f64) -> f64 {
    y.atan2(x)
}

/// Length of the hypotenuse, `sqrt(x^2 + y^2)`, without undue overflow.
#[inline]
#[must_use]
pub fn hypot(x: f64, y: f64) -> f64 {
    x.hypot(y)
}

/// Cube root.
#[inline]
#[must_use]
pub fn cbrt(x: f64) -> f64 {
    x.cbrt()
}

/// `x` raised to the integer power `n`.
///
/// Exponentiation by squaring with a fixed multiplication order (the compiler-rt `__powidf2`
/// algorithm), not the LLVM `powi` intrinsic, whose result can differ between debug and release
/// builds through constant folding. Only exact multiplies and one division are used.
#[inline]
#[must_use]
pub fn powi(x: f64, n: i32) -> f64 {
    let mut exponent = n.unsigned_abs();
    let mut base = x;
    let mut result = 1.0;
    loop {
        if exponent & 1 == 1 {
            result *= base;
        }
        exponent >>= 1;
        if exponent == 0 {
            break;
        }
        base *= base;
    }
    if n < 0 { 1.0 / result } else { result }
}

/// `x` raised to the real power `y`.
#[inline]
#[must_use]
pub fn powf(x: f64, y: f64) -> f64 {
    x.powf(y)
}

/// `e^x`.
#[inline]
#[must_use]
pub fn exp(x: f64) -> f64 {
    x.exp()
}

/// Natural logarithm.
#[inline]
#[must_use]
pub fn ln(x: f64) -> f64 {
    x.ln()
}

/// Wraps an angle (radians) to the half-open interval `(-PI, PI]`.
///
/// Angles already inside the interval are returned unchanged (bit for bit). Uses exact
/// operations only (`+ - * /` and `floor`), no transcendental function. NaN and infinities give
/// NaN.
#[must_use]
pub fn wrap_pi(x: f64) -> f64 {
    if x > -PI && x <= PI {
        return x;
    }
    // [-PI, PI) by floor; rounding can leave the edges, so fix them up.
    let mut r = x - TAU * ((x + PI) / TAU).floor();
    if r <= -PI {
        r += TAU;
    }
    if r > PI {
        r = PI;
    }
    r
}
