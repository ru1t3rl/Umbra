//! WI-2: `TickRate::ticks_for`. Requirements: ASM-078, ASM-066.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

use cgf_core::{Seconds, TickRate};

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
