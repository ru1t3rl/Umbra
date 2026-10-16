//! SI unit newtypes (REQ-C-4, ASM-015). STUB (M0 step 4): signatures only, bodies are `todo!()` until WI-5.
#![allow(clippy::todo, clippy::needless_pass_by_value, missing_docs)]

use core::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};

macro_rules! unit_stub {
    ($($name:ident),*) => {$(
        #[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
        pub struct $name(pub f64);
        impl Add for $name { type Output = $name; fn add(self, _: $name) -> $name { todo!() } }
        impl Sub for $name { type Output = $name; fn sub(self, _: $name) -> $name { todo!() } }
        impl Neg for $name { type Output = $name; fn neg(self) -> $name { todo!() } }
        impl Mul<f64> for $name { type Output = $name; fn mul(self, _: f64) -> $name { todo!() } }
        impl Div<f64> for $name { type Output = $name; fn div(self, _: f64) -> $name { todo!() } }
        impl Div<$name> for $name { type Output = f64; fn div(self, _: $name) -> f64 { todo!() } }
        impl AddAssign for $name { fn add_assign(&mut self, _: $name) { todo!() } }
        impl SubAssign for $name { fn sub_assign(&mut self, _: $name) { todo!() } }
    )*};
}
unit_stub!(
    Meters,
    Seconds,
    Radians,
    Kilograms,
    MetersPerSecond,
    MetersPerSecondSquared,
    RadiansPerSecond
);

impl Mul<Seconds> for MetersPerSecond {
    type Output = Meters;
    fn mul(self, _: Seconds) -> Meters {
        todo!()
    }
}
impl Div<Seconds> for Meters {
    type Output = MetersPerSecond;
    fn div(self, _: Seconds) -> MetersPerSecond {
        todo!()
    }
}
impl Mul<Seconds> for MetersPerSecondSquared {
    type Output = MetersPerSecond;
    fn mul(self, _: Seconds) -> MetersPerSecond {
        todo!()
    }
}
impl Div<Seconds> for MetersPerSecond {
    type Output = MetersPerSecondSquared;
    fn div(self, _: Seconds) -> MetersPerSecondSquared {
        todo!()
    }
}
impl Mul<Seconds> for RadiansPerSecond {
    type Output = Radians;
    fn mul(self, _: Seconds) -> Radians {
        todo!()
    }
}
impl Div<Seconds> for Radians {
    type Output = RadiansPerSecond;
    fn div(self, _: Seconds) -> RadiansPerSecond {
        todo!()
    }
}

impl Radians {
    pub fn from_degrees(deg: f64) -> Self {
        let _ = deg;
        todo!()
    }
    pub fn to_degrees(self) -> f64 {
        todo!()
    }
}
impl RadiansPerSecond {
    pub fn from_degrees_per_second(dps: f64) -> Self {
        let _ = dps;
        todo!()
    }
    pub fn to_degrees_per_second(self) -> f64 {
        todo!()
    }
}
