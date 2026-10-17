//! AT-8: simulation time. Requirements: REQ-C-1, D-5, ASM-016.

#![forbid(unsafe_code)]

use cgf_core::{Seconds, SimTime, TickRate, TimeError};
use proptest::prelude::*;

#[expect(clippy::expect_used, reason = "test helper")]
fn rate(hz: u32) -> TickRate {
    TickRate::new(hz).expect("non-zero rate")
}

/// REQ-C-1 / D-5: a zero tick rate is not representable.
#[test]
fn tick_rate_zero_is_rejected() {
    assert_eq!(TickRate::new(0), None);
}

/// REQ-C-1: a positive rate round-trips through `hz()`.
#[test]
fn tick_rate_new_keeps_hz() {
    assert_eq!(TickRate::new(30).map(TickRate::hz), Some(30));
}

/// REQ-C-1: `dt` is `1/hz`.
#[test]
fn tick_rate_dt_is_one_over_hz() {
    assert_eq!(rate(50).dt(), Seconds(1.0 / 50.0));
}

/// REQ-C-1: time starts at tick 0, i.e. 0 seconds.
#[test]
fn sim_time_start_is_tick_zero_and_zero_seconds() {
    let t = SimTime::start(rate(30));

    assert_eq!(t.tick(), 0);
    assert_eq!(t.as_seconds(), Seconds(0.0));
}

/// REQ-C-1: `at_tick` keeps tick and rate.
#[test]
fn at_tick_keeps_tick_and_rate() {
    let t = SimTime::at_tick(rate(60), 7);

    assert_eq!((t.tick(), t.rate()), (7, rate(60)));
}

/// ASM-016: seconds are `tick / hz` (30 Hz, tick 90 -> 3.0 s), never accumulated.
#[test]
fn as_seconds_equals_tick_over_hz() {
    assert_eq!(SimTime::at_tick(rate(30), 90).as_seconds(), Seconds(3.0));
    // 3/10 is 0.3, whereas 3 * (1/10) is 0.30000000000000004.
    assert_eq!(SimTime::at_tick(rate(10), 3).as_seconds(), Seconds(0.3));
}

/// REQ-C-1: `SimTime::dt` matches the rate's `dt`.
#[test]
fn sim_time_dt_equals_rate_dt() {
    let t = SimTime::at_tick(rate(20), 5);

    assert_eq!(t.dt(), rate(20).dt());
}

/// REQ-C-1: `checked_next` advances by exactly one tick.
#[test]
fn checked_next_advances_one_tick() {
    let t = SimTime::at_tick(rate(30), 41);

    assert_eq!(t.checked_next().map(SimTime::tick), Some(42));
}

/// REQ-C-1: overflow of the tick counter is reported, not wrapped.
#[test]
fn checked_next_at_u64_max_is_none() {
    assert_eq!(SimTime::at_tick(rate(30), u64::MAX).checked_next(), None);
}

/// REQ-C-1: `checked_advance` adds ticks and detects overflow.
#[test]
fn checked_advance_adds_ticks_and_detects_overflow() {
    let t = SimTime::at_tick(rate(30), 10);

    assert_eq!(t.checked_advance(5).map(SimTime::tick), Some(15));
    assert_eq!(t.checked_advance(u64::MAX), None);
}

/// REQ-C-1: interval between two times of equal rate is a tick count.
#[test]
fn ticks_since_with_same_rate_is_tick_difference() {
    let (a, b) = (
        SimTime::at_tick(rate(30), 10),
        SimTime::at_tick(rate(30), 25),
    );

    assert_eq!(b.ticks_since(a), Ok(15));
    assert_eq!(a.ticks_since(a), Ok(0));
}

/// REQ-C-1: mixing rates is an error that names both rates.
#[test]
fn ticks_since_with_different_rates_is_rate_mismatch() {
    let (a, b) = (
        SimTime::at_tick(rate(30), 10),
        SimTime::at_tick(rate(60), 25),
    );

    assert_eq!(
        b.ticks_since(a),
        Err(TimeError::RateMismatch {
            left: 60,
            right: 30
        })
    );
}

/// REQ-C-1: asking for an interval to a later time is an error.
#[test]
fn ticks_since_a_later_time_is_negative_interval() {
    let (a, b) = (
        SimTime::at_tick(rate(30), 10),
        SimTime::at_tick(rate(30), 25),
    );

    assert_eq!(
        a.ticks_since(b),
        Err(TimeError::NegativeInterval { from: 25, to: 10 })
    );
}

/// REQ-C-1: equal rates are totally ordered by tick.
#[test]
fn partial_cmp_with_same_rate_orders_by_tick() {
    let (a, b) = (SimTime::at_tick(rate(30), 1), SimTime::at_tick(rate(30), 2));

    assert_eq!(a.partial_cmp(&b), Some(core::cmp::Ordering::Less));
    assert_eq!(b.partial_cmp(&a), Some(core::cmp::Ordering::Greater));
    assert_eq!(a.partial_cmp(&a), Some(core::cmp::Ordering::Equal));
}

/// REQ-C-1: no order across different rates.
#[test]
fn partial_cmp_with_different_rates_is_none() {
    let (a, b) = (SimTime::at_tick(rate(30), 1), SimTime::at_tick(rate(60), 1));

    assert_eq!(a.partial_cmp(&b), None);
}

/// `TimeError` messages are stable (user-visible).
#[test]
fn time_error_messages_are_stable() {
    assert_eq!(
        TimeError::RateMismatch {
            left: 30,
            right: 60
        }
        .to_string(),
        "tick rates differ: 30 Hz vs 60 Hz"
    );
    assert_eq!(
        TimeError::NegativeInterval { from: 9, to: 3 }.to_string(),
        "time 3 is before 9"
    );
}

proptest! {
    /// ASM-016: `as_seconds` is exactly `tick / hz`, bit for bit (not `tick * dt`).
    #[test]
    fn as_seconds_is_bitwise_tick_over_hz(hz in 1u32..=1000, tick in 0u64..(1u64 << 53)) {
        let got = SimTime::at_tick(rate(hz), tick).as_seconds().0;

        #[expect(clippy::cast_precision_loss, reason = "tick < 2^53 is exact")]
        let want = tick as f64 / f64::from(hz);
        prop_assert_eq!(got.to_bits(), want.to_bits());
    }

    /// Clock-monotonicity groundwork (section 15): ticks strictly increase, seconds never decrease.
    #[test]
    fn next_strictly_increases_tick_and_never_decreases_seconds(
        hz in 1u32..=1000,
        tick in 0u64..(1u64 << 53),
    ) {
        let t = SimTime::at_tick(rate(hz), tick);
        let n = t.checked_next().expect("below 2^53 cannot overflow");

        prop_assert!(n.tick() > t.tick());
        prop_assert!(n.as_seconds() >= t.as_seconds());
    }
}
