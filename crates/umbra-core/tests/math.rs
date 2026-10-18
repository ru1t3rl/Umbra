//! AT-4: the `umbra_core::math` module. Requirements: ADR 0003, REQ-K-4, ASM-034.
//!
//! The wrappers must be thin: on this platform they equal the `std` functions bit for bit, so a
//! later `libm` swap changes exactly one file. This is the only test file that calls the banned
//! `f64` methods directly (the workspace ban in `clippy.toml`).

#![forbid(unsafe_code)]
#![expect(
    clippy::disallowed_methods,
    reason = "ADR 0003: this test compares the wrappers against the std implementation"
)]

use core::f64::consts::{FRAC_PI_2, PI, TAU};

use proptest::prelude::*;
use umbra_core::Radians;
use umbra_core::math;

/// Bit equality that also treats every NaN as equal to every NaN (NaN propagation).
fn same(a: f64, b: f64) -> bool {
    (a.is_nan() && b.is_nan()) || a.to_bits() == b.to_bits()
}

const SAMPLES: [f64; 14] = [
    0.0,
    -0.0,
    1.0,
    -1.0,
    0.5,
    PI,
    -PI,
    FRAC_PI_2,
    TAU,
    1.0e6,
    -1.0e6,
    1.0e-300,
    f64::INFINITY,
    f64::NAN,
];

/// ADR 0003, ASM-034: every unary wrapper returns exactly what `std` returns.
#[test]
fn wrappers_equal_std_bitwise() {
    for x in SAMPLES {
        assert!(same(math::sin(x), x.sin()), "sin({x:?})");
        assert!(same(math::cos(x), x.cos()), "cos({x:?})");
        assert!(same(math::tan(x), x.tan()), "tan({x:?})");
        assert!(same(math::asin(x), x.asin()), "asin({x:?})");
        assert!(same(math::acos(x), x.acos()), "acos({x:?})");
        assert!(same(math::atan(x), x.atan()), "atan({x:?})");
        assert!(same(math::cbrt(x), x.cbrt()), "cbrt({x:?})");
        assert!(same(math::exp(x), x.exp()), "exp({x:?})");
        assert!(same(math::ln(x), x.ln()), "ln({x:?})");
        let (s, c) = math::sin_cos(x);
        let (es, ec) = x.sin_cos();
        assert!(same(s, es) && same(c, ec), "sin_cos({x:?})");
    }
}

/// ADR 0003, ASM-034: every binary wrapper returns exactly what `std` returns.
#[test]
fn binary_wrappers_equal_std_bitwise() {
    for y in SAMPLES {
        for x in SAMPLES {
            assert!(same(math::atan2(y, x), y.atan2(x)), "atan2({y:?}, {x:?})");
            assert!(same(math::hypot(y, x), y.hypot(x)), "hypot({y:?}, {x:?})");
            assert!(same(math::powf(y, x), y.powf(x)), "powf({y:?}, {x:?})");
        }
    }
    for x in SAMPLES {
        for n in [-3, 0, 1, 2, 7] {
            assert!(same(math::powi(x, n), x.powi(n)), "powi({x:?}, {n})");
        }
    }
}

/// ASM-034: NaN in gives NaN out (no panic, no silent clamp).
#[test]
fn nan_propagates_through_wrappers() {
    assert!(math::sin(f64::NAN).is_nan());
    assert!(math::atan2(f64::NAN, 1.0).is_nan());
    assert!(math::wrap_pi(Radians(f64::NAN)).0.is_nan());
}

/// ADR 0003: `wrap_pi` of exactly pi is pi, and of exactly minus pi is pi (half-open interval).
#[test]
fn wrap_pi_of_pi_is_pi_and_of_minus_pi_is_pi() {
    assert_eq!(math::wrap_pi(Radians(PI)), Radians(PI));
    assert_eq!(math::wrap_pi(Radians(-PI)), Radians(PI));
}

/// ADR 0003: inside the interval `wrap_pi` is the identity, bit for bit (exact operations only).
#[test]
fn wrap_pi_is_identity_inside_interval() {
    for x in [0.0, 1.0, -1.0, 3.0, -3.0, PI] {
        assert_eq!(math::wrap_pi(Radians(x)).0.to_bits(), x.to_bits(), "{x}");
    }
}

/// ADR 0003: whole turns wrap to the equivalent angle.
#[test]
fn wrap_pi_of_whole_turns_is_the_equivalent_angle() {
    assert!(math::wrap_pi(Radians(TAU)).0.abs() < 1e-12);
    assert!((math::wrap_pi(Radians(3.0 * PI)).0 - PI).abs() < 1e-12);
    assert!((math::wrap_pi(Radians(-3.0 * PI)).0 - PI).abs() < 1e-12);
    assert!((math::wrap_pi(Radians(PI + 0.5)).0 - (-PI + 0.5)).abs() < 1e-12);
}

proptest! {
    /// ADR 0003: the result is always in (-PI, PI] and differs from the input by whole turns.
    #[test]
    fn wrap_pi_maps_into_half_open_interval(a in -1000.0_f64..1000.0) {
        let r = math::wrap_pi(Radians(a)).0;
        prop_assert!(r > -PI && r <= PI, "{a} -> {r}");
        let turns = (r - a) / TAU;
        prop_assert!((turns - turns.round()).abs() < 1e-9, "{a} -> {r}");
    }
}

/// ADR 0003, ADR 0007: `powi` is a fixed-order product, exact on small integer cases.
#[test]
fn powi_known_values() {
    assert_eq!(math::powi(2.0, 10), 1024.0);
    assert_eq!(math::powi(2.0, -2), 0.25);
    assert_eq!(math::powi(-3.0, 3), -27.0);
    assert_eq!(math::powi(5.0, 0), 1.0);
    assert_eq!(math::powi(0.0, 0), 1.0);
    assert_eq!(math::powi(10.0, 6), 1.0e6);
    assert!(math::powi(0.0, -1).is_infinite());
    assert_eq!(math::powi(1.0, i32::MIN), 1.0);
}
