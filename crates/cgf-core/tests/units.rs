//! AT-10: SI unit newtypes. Requirements: REQ-C-4, ASM-015.

#![forbid(unsafe_code)]

use core::f64::consts::PI;

use cgf_core::{
    Kilograms, Meters, MetersPerSecond, MetersPerSecondSquared, Radians, RadiansPerSecond, Seconds,
};
use proptest::prelude::*;

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-12 * a.abs().max(b.abs()).max(1.0)
}

/// REQ-C-4: velocity times time is distance.
#[test]
fn speed_times_seconds_is_meters() {
    assert_eq!(MetersPerSecond(2.0) * Seconds(3.0), Meters(6.0));
}

/// REQ-C-4: distance over time is velocity.
#[test]
fn meters_div_seconds_is_speed() {
    assert_eq!(Meters(6.0) / Seconds(3.0), MetersPerSecond(2.0));
}

/// REQ-C-4: acceleration times time is velocity.
#[test]
fn acceleration_times_seconds_is_speed() {
    assert_eq!(
        MetersPerSecondSquared(2.0) * Seconds(4.0),
        MetersPerSecond(8.0)
    );
}

/// REQ-C-4: velocity over time is acceleration.
#[test]
fn speed_div_seconds_is_acceleration() {
    assert_eq!(
        MetersPerSecond(8.0) / Seconds(4.0),
        MetersPerSecondSquared(2.0)
    );
}

/// REQ-C-4: angular rate times time is an angle.
#[test]
fn angular_rate_times_seconds_is_radians() {
    assert_eq!(RadiansPerSecond(0.5) * Seconds(4.0), Radians(2.0));
}

/// REQ-C-4: angle over time is angular rate.
#[test]
fn radians_div_seconds_is_angular_rate() {
    assert_eq!(Radians(2.0) / Seconds(4.0), RadiansPerSecond(0.5));
}

/// REQ-C-4: same-type arithmetic (add, sub, neg, scale).
#[test]
fn same_type_arithmetic() {
    assert_eq!(Meters(1.5) + Meters(2.0), Meters(3.5));
    assert_eq!(Meters(5.0) - Meters(2.0), Meters(3.0));
    assert_eq!(-Meters(2.0), Meters(-2.0));
    assert_eq!(Meters(2.0) * 3.0, Meters(6.0));
    assert_eq!(Meters(6.0) / 3.0, Meters(2.0));
}

/// REQ-C-4: dividing two quantities of one unit gives a dimensionless ratio.
#[test]
fn same_type_division_is_ratio() {
    let ratio: f64 = Kilograms(6.0) / Kilograms(3.0);

    assert_eq!(ratio, 2.0);
}

/// REQ-C-4: compound assignment.
#[test]
fn assign_operators() {
    let mut s = Seconds(1.0);
    s += Seconds(2.0);
    assert_eq!(s, Seconds(3.0));
    s -= Seconds(0.5);
    assert_eq!(s, Seconds(2.5));
}

/// REQ-C-4: every unit has the same derives: Default is zero, ordering is by value.
#[test]
fn default_is_zero_and_ordering_is_by_value() {
    assert_eq!(Meters::default(), Meters(0.0));
    assert!(Seconds(1.0) < Seconds(2.0));
    assert_eq!(
        Radians(1.0).partial_cmp(&Radians(1.0)),
        Some(core::cmp::Ordering::Equal)
    );
}

/// REQ-C-4: 180 degrees is pi radians, and back.
#[test]
fn radians_from_degrees_180_is_pi() {
    assert_eq!(Radians::from_degrees(180.0).0, PI);
    assert!(close(Radians(PI).to_degrees(), 180.0));
}

/// REQ-C-4: angular rate conversions.
#[test]
fn angular_rate_degree_conversions() {
    assert!(close(
        RadiansPerSecond::from_degrees_per_second(90.0).0,
        PI / 2.0
    ));
    assert!(close(RadiansPerSecond(PI).to_degrees_per_second(), 180.0));
}

proptest! {
    /// REQ-C-4: degrees -> radians -> degrees round-trips within 1e-12 relative error.
    #[test]
    fn degrees_to_radians_and_back_round_trips(deg in -1.0e6f64..1.0e6) {
        prop_assert!(close(Radians::from_degrees(deg).to_degrees(), deg));
    }

    /// REQ-C-4: same for angular rates.
    #[test]
    fn degrees_per_second_round_trips(dps in -1.0e6f64..1.0e6) {
        prop_assert!(close(RadiansPerSecond::from_degrees_per_second(dps).to_degrees_per_second(), dps));
    }
}
