//! AT-17 (M7 WI-10, ADR 0059): `umbra_sdk::math` and `umbra_core::math` give bit-identical results for
//! every function, on fixed edge cases (+-0, subnormals, the +-pi boundaries of `wrap_pi`, large
//! arguments, infinities; NaN compared as a class) and on a proptest sample. Both are the swap points
//! of ADR 0003 (one per side of the ABI); a `libm` switch must change both in one commit, and this test
//! fails otherwise.
//!
//! The host may not depend on `umbra-sdk` (section 4 rule table), so the SDK's module is compiled into
//! this test from its source file (ASM-713): the same text, built by the same compiler with the same
//! profile. The file must stay self-contained (no `crate::` paths); a dependency there fails to build
//! here, loudly.

#[path = "../../umbra-sdk/src/math.rs"]
mod sdk;

use proptest::prelude::*;
use umbra_core::Radians;
use umbra_core::math as core;

/// Equal bits, or both NaN (NaN payloads are not part of the contract).
fn same(a: f64, b: f64) -> bool {
    a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan())
}

const PI: f64 = std::f64::consts::PI;

/// Inputs every function is checked on.
fn edge_cases() -> Vec<f64> {
    let mut v = vec![
        0.0,
        -0.0,
        f64::MIN_POSITIVE,
        -f64::MIN_POSITIVE,
        f64::from_bits(1),
        -f64::from_bits(1),
        f64::MIN_POSITIVE / 3.0,
        f64::EPSILON,
        0.5,
        -0.5,
        1.0,
        -1.0,
        1.0 + f64::EPSILON,
        2.0,
        3.0,
        PI,
        -PI,
        PI.next_up(),
        (-PI).next_down(),
        PI.next_down(),
        (-PI).next_up(),
        std::f64::consts::TAU,
        std::f64::consts::FRAC_PI_2,
        1e10,
        -1e10,
        1e22,
        1e300,
        -1e300,
        f64::MAX,
        f64::MIN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NAN,
        6_378_137.0,
        709.7,
        -745.2,
    ];
    v.extend((-20..=20).map(|k| f64::from(k) * 0.37));
    v
}

/// A unary function of either module.
type Unary = fn(f64) -> f64;
/// A binary function of either module.
type Binary = fn(f64, f64) -> f64;
/// A function by name: the core version and the SDK version.
type Pair<F> = (&'static str, F, F);

/// Every unary function of both modules, by name.
fn unary() -> Vec<Pair<Unary>> {
    vec![
        ("sin", core::sin, sdk::sin),
        ("cos", core::cos, sdk::cos),
        ("sin_cos.0", |x| core::sin_cos(x).0, |x| sdk::sin_cos(x).0),
        ("sin_cos.1", |x| core::sin_cos(x).1, |x| sdk::sin_cos(x).1),
        ("tan", core::tan, sdk::tan),
        ("asin", core::asin, sdk::asin),
        ("acos", core::acos, sdk::acos),
        ("atan", core::atan, sdk::atan),
        ("cbrt", core::cbrt, sdk::cbrt),
        ("exp", core::exp, sdk::exp),
        ("ln", core::ln, sdk::ln),
        ("wrap_pi", |x| core::wrap_pi(Radians(x)).0, sdk::wrap_pi),
    ]
}

/// Every binary function of both modules (`powi` with its exponent as an `f64` that fits `i32`).
fn binary() -> Vec<Pair<Binary>> {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the exponents are small integers built by the test"
    )]
    fn exponent(y: f64) -> i32 {
        y.clamp(-64.0, 64.0) as i32
    }
    vec![
        ("atan2", core::atan2, sdk::atan2),
        ("hypot", core::hypot, sdk::hypot),
        ("powf", core::powf, sdk::powf),
        (
            "powi",
            |x, y| core::powi(x, exponent(y)),
            |x, y| sdk::powi(x, exponent(y)),
        ),
    ]
}

fn check_unary(x: f64) -> Vec<String> {
    unary()
        .into_iter()
        .filter(|(_, c, s)| !same(c(x), s(x)))
        .map(|(name, c, s)| format!("{name}({x:e}): core {:e}, sdk {:e}", c(x), s(x)))
        .collect()
}

fn check_binary(x: f64, y: f64) -> Vec<String> {
    binary()
        .into_iter()
        .filter(|(_, c, s)| !same(c(x, y), s(x, y)))
        .map(|(name, c, s)| {
            format!(
                "{name}({x:e}, {y:e}): core {:e}, sdk {:e}",
                c(x, y),
                s(x, y)
            )
        })
        .collect()
}

#[test]
fn every_function_agrees_bit_for_bit_on_the_edge_cases() {
    let cases = edge_cases();
    let mut diffs: Vec<String> = cases.iter().flat_map(|&x| check_unary(x)).collect();
    for &x in &cases {
        for &y in &cases {
            diffs.extend(check_binary(x, y));
        }
    }
    let exponents = [-3.0, -1.0, 0.0, 1.0, 2.0, 3.0, 7.0, 31.0, 64.0, -64.0];
    for &x in &cases {
        for &n in &exponents {
            diffs.extend(check_binary(x, n));
        }
    }

    assert_eq!(diffs, Vec::<String>::new());
}

/// The comparison itself can fail: a perturbed result is caught (guards against a vacuous `same`).
#[test]
fn the_comparison_detects_a_one_ulp_difference_and_treats_nan_as_a_class() {
    assert!(!same(1.0, 1.0_f64.next_up()));
    assert!(!same(0.0, -0.0));
    assert!(same(f64::NAN, -f64::NAN));
    assert!(!same(f64::NAN, 0.0));
}

/// Both `wrap_pi` keep angles inside `(-pi, pi]` bit for bit and agree at the edges.
#[test]
fn wrap_pi_agrees_at_the_interval_edges() {
    for x in [
        PI,
        -PI,
        PI.next_up(),
        (-PI).next_up(),
        3.0 * PI,
        -3.0 * PI,
        1e9,
    ] {
        assert!(
            same(core::wrap_pi(Radians(x)).0, sdk::wrap_pi(x)),
            "wrap_pi({x:e})"
        );
    }
    assert!(same(sdk::wrap_pi(1.25), 1.25), "inside: unchanged");
    assert!(same(sdk::wrap_pi(-PI), PI), "-pi maps to pi");
}

proptest! {
    #[test]
    fn every_unary_function_agrees_on_any_finite_input(x in proptest::num::f64::NORMAL | proptest::num::f64::SUBNORMAL | proptest::num::f64::ZERO) {
        prop_assert_eq!(check_unary(x), Vec::<String>::new());
    }

    #[test]
    fn every_unary_function_agrees_on_moderate_angles(x in -1.0e4_f64..1.0e4) {
        prop_assert_eq!(check_unary(x), Vec::<String>::new());
    }

    #[test]
    fn every_binary_function_agrees(x in -1.0e6_f64..1.0e6, y in -1.0e6_f64..1.0e6) {
        prop_assert_eq!(check_binary(x, y), Vec::<String>::new());
    }

    #[test]
    fn powi_agrees_on_small_exponents(x in -100.0_f64..100.0, n in -40_i32..40) {
        prop_assert!(same(core::powi(x, n), sdk::powi(x, n)), "powi({x:e}, {n})");
    }
}
