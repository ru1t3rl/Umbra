//! M3 WI-2: `TickRate::ticks_for`. Requirements: ASM-078, ASM-066.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

use umbra_core::{Seconds, TickRate};

fn hz30() -> TickRate {
    TickRate::new(30).expect("non-zero")
}

fn ticks(hz: TickRate, s: f64) -> Option<u64> {
    hz.ticks_for(Seconds(s)).map(std::num::NonZeroU64::get)
}

#[test]
fn float_artefact_does_not_add_a_tick() {
    assert_eq!(ticks(hz30(), 0.1), Some(3));
}

#[test]
fn whole_and_fractional_durations() {
    assert_eq!(ticks(hz30(), 2.5), Some(75));
    assert_eq!(ticks(hz30(), 1.0), Some(30));
    assert_eq!(ticks(hz30(), 0.05), Some(2));
}

#[test]
fn tiny_positive_is_one_tick() {
    assert_eq!(ticks(hz30(), 1e-12), Some(1));
    assert_eq!(ticks(hz30(), f64::MIN_POSITIVE), Some(1));
}

#[test]
fn invalid_or_huge_is_none() {
    for s in [
        0.0,
        -0.0,
        -1.0,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        1e300,
        1e18,
    ] {
        assert_eq!(ticks(hz30(), s), None, "{s}");
    }
}

#[test]
fn two_ulp_guard_width_is_pinned() {
    let one_hz = TickRate::new(1).expect("non-zero");
    let three = 3.0_f64;
    assert_eq!(ticks(one_hz, three.next_up().next_up()), Some(3));
    assert_eq!(ticks(one_hz, three.next_up().next_up().next_up()), Some(4));
}

#[test]
fn subnormal_is_one_tick() {
    let one_hz = TickRate::new(1).expect("non-zero");
    assert_eq!(ticks(one_hz, f64::from_bits(1)), Some(1));
}

#[test]
fn u64_boundary() {
    let one_hz = TickRate::new(1).expect("non-zero");
    assert_eq!(
        ticks(one_hz, 18_446_744_073_709_549_568.0),
        Some(18_446_744_073_709_549_568)
    );
    assert_eq!(ticks(one_hz, 18_446_744_073_709_551_616.0), None);
}

#[test]
fn overflowing_product_is_none() {
    assert_eq!(ticks(hz30(), f64::MAX), None);
}
