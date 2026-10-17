//! STUB (WI-2): the single site for non-exact `f64` functions (ADR 0003, ASM-034).
//!
//! Signatures only; bodies land in WI-2. Acceptance tests: `tests/math.rs` (AT-4).
#![expect(
    missing_docs,
    clippy::todo,
    reason = "stub until WI-2: signatures only, documented when implemented"
)]

use crate::units::Radians;

pub fn sin(_x: f64) -> f64 {
    todo!("WI-2")
}
pub fn cos(_x: f64) -> f64 {
    todo!("WI-2")
}
pub fn sin_cos(_x: f64) -> (f64, f64) {
    todo!("WI-2")
}
pub fn tan(_x: f64) -> f64 {
    todo!("WI-2")
}
pub fn asin(_x: f64) -> f64 {
    todo!("WI-2")
}
pub fn acos(_x: f64) -> f64 {
    todo!("WI-2")
}
pub fn atan(_x: f64) -> f64 {
    todo!("WI-2")
}
pub fn atan2(_y: f64, _x: f64) -> f64 {
    todo!("WI-2")
}
pub fn hypot(_x: f64, _y: f64) -> f64 {
    todo!("WI-2")
}
pub fn cbrt(_x: f64) -> f64 {
    todo!("WI-2")
}
pub fn powi(_x: f64, _n: i32) -> f64 {
    todo!("WI-2")
}
pub fn powf(_x: f64, _y: f64) -> f64 {
    todo!("WI-2")
}
pub fn exp(_x: f64) -> f64 {
    todo!("WI-2")
}
pub fn ln(_x: f64) -> f64 {
    todo!("WI-2")
}
/// Wraps an angle to (-PI, PI].
pub fn wrap_pi(_a: Radians) -> Radians {
    todo!("WI-2")
}
