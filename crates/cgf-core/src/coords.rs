//! WGS-84, ECEF, geodetic, ENU/NED and local frames (REQ-C-3, D-15, ASM-033, ASM-044).
//!
//! The canonical position is [`Ecef`]: Earth-centred, Earth-fixed, meters, `f64` (D-15). Geodetic
//! latitude/longitude/height ([`Geodetic`]) and local tangent-plane coordinates ([`Enu`], [`Ned`])
//! exist for input, display and local geometry; [`Position`] lets the API edge accept either form
//! and converts to ECEF exactly once (ASM-044).
//!
//! Conventions (ASM-033):
//! - ECEF: +x toward (lat 0, lon 0), +y toward (lat 0, lon 90 E), +z toward the north pole.
//! - Geodetic: latitude and longitude in radians, height in meters above the WGS-84 ellipsoid
//!   (HAE, no geoid). Longitude is wrapped to `(-pi, pi]`, latitude lies in `[-pi/2, pi/2]`.
//! - ENU/NED: axes of the local tangent plane at a [`LocalFrame`] origin.
//! - Orientation: a unit quaternion rotating body FRD (forward, right, down) axes to ECEF axes.
//!
//! References: NIMA TR8350.2 (WGS-84); IOGP Publication 373-7-2, methods 9602 and 9836;
//! Heikkinen (1982) as given by Zhu (1994), IEEE Trans. AES 30(3), for ECEF to geodetic.

use core::ops::{Add, Sub};

use crate::geo::{Euler, Quaternion, Vec3};
use crate::math;
use crate::units::{Meters, Radians};

/// WGS-84 ellipsoid constants (NIMA TR8350.2, 3rd ed., Tables 3.1 and 3.3). SI units.
pub mod wgs84 {
    /// Semi-major axis, meters (defining).
    pub const A: f64 = 6_378_137.0;
    /// Inverse flattening (defining).
    pub const INV_F: f64 = 298.257_223_563;
    /// Flattening, `1 / INV_F`.
    pub const F: f64 = 1.0 / INV_F;
    /// Semi-minor axis, meters, `A * (1 - F)`.
    pub const B: f64 = A * (1.0 - F);
    /// First eccentricity squared, `F * (2 - F)`.
    pub const E2: f64 = F * (2.0 - F);
    /// Second eccentricity squared, `E2 / (1 - E2)`.
    pub const EP2: f64 = E2 / (1.0 - E2);
}

/// Canonical position: Earth-centred, Earth-fixed, meters (D-15).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ecef(pub Vec3);

impl Ecef {
    /// Converts to WGS-84 geodetic coordinates.
    ///
    /// Closed form (Heikkinen 1982 as given by Zhu 1994): no iteration, so the cost and the
    /// result do not depend on a convergence test. The polar axis and the region around the
    /// Earth's centre (where the closed form is singular) are handled explicitly. Infallible, and
    /// finite for every finite input. Beyond 1e12 m from the centre (far outside any physical
    /// use) a spherical approximation is returned, because the closed form's intermediates
    /// overflow `f64` for huge coordinates.
    #[must_use]
    pub fn to_geodetic(self) -> Geodetic {
        let Vec3 { x, y, z } = self.0;
        let r = math::hypot(x, y);
        let lon = if r == 0.0 { 0.0 } else { math::atan2(y, x) };
        let (lat, height) = if r.max(z.abs()) > SPHERICAL_ABOVE_M {
            (math::atan2(z, r), math::hypot(r, z) - wgs84::B)
        } else {
            latitude_and_height(r, z)
        };
        Geodetic {
            lat: Radians(lat),
            lon: math::wrap_pi(Radians(lon)),
            height: Meters(height),
        }
    }

    /// Straight-line (chord) distance to `o`.
    #[must_use]
    pub fn distance(self, o: Self) -> Meters {
        Meters((self - o).norm())
    }
}

/// Distance beyond which `to_geodetic` falls back to a sphere (closed-form intermediates overflow).
const SPHERICAL_ABOVE_M: f64 = 1.0e12;

/// Latitude (radians) and height (meters) for the cylindrical radius `r >= 0` and `z`.
fn latitude_and_height(r: f64, z: f64) -> (f64, f64) {
    use core::f64::consts::FRAC_PI_2;
    use wgs84::{A, B, E2, EP2};
    if r == 0.0 {
        // Polar axis (and the centre, where the sign of z picks the hemisphere).
        let lat = if z < 0.0 { -FRAC_PI_2 } else { FRAC_PI_2 };
        return (lat, z.abs() - B);
    }
    let (a2, b2) = (A * A, B * B);
    let ff = 54.0 * b2 * z * z;
    let gg = r * r + (1.0 - E2) * z * z - E2 * (a2 - b2);
    if gg <= 0.0 {
        // Within ~42.7 km (e^2 * a) of the centre the closed form is singular: spherical fallback.
        return (math::atan2(z, r), (r * r + z * z).sqrt() - B);
    }
    let cc = E2 * E2 * ff * r * r / (gg * gg * gg);
    let ss = math::cbrt(1.0 + cc + (cc * cc + 2.0 * cc).sqrt());
    let sum = ss + 1.0 / ss + 1.0;
    let pp = ff / (3.0 * sum * sum * gg * gg);
    let qq = (1.0 + 2.0 * E2 * E2 * pp).sqrt();
    let r_zero = -(pp * E2 * r) / (1.0 + qq)
        + (0.5 * a2 * (1.0 + 1.0 / qq)
            - pp * (1.0 - E2) * z * z / (qq * (1.0 + qq))
            - 0.5 * pp * r * r)
            .max(0.0)
            .sqrt();
    let rt = r - E2 * r_zero;
    let big_u = (rt * rt + z * z).sqrt();
    let big_v = (rt * rt + (1.0 - E2) * z * z).sqrt();
    let z_zero = b2 * z / (A * big_v);
    let height = big_u * (1.0 - b2 / (A * big_v));
    let lat = math::atan((z + EP2 * z_zero) / r);
    (lat, height)
}

impl Sub for Ecef {
    type Output = Vec3;
    /// The ECEF displacement vector from `rhs` to `self`, meters.
    fn sub(self, rhs: Ecef) -> Vec3 {
        self.0 - rhs.0
    }
}

impl Add<Vec3> for Ecef {
    type Output = Ecef;
    /// The point displaced by an ECEF vector, meters.
    fn add(self, rhs: Vec3) -> Ecef {
        Ecef(self.0 + rhs)
    }
}

/// WGS-84 geodetic position: latitude, longitude and height above the ellipsoid.
///
/// Fields are private so every value is valid: finite, latitude in `[-pi/2, pi/2]`, longitude in
/// `(-pi, pi]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Geodetic {
    lat: Radians,
    lon: Radians,
    height: Meters,
}

impl Geodetic {
    /// Validates and builds a geodetic position. Longitude is wrapped to `(-pi, pi]`; absurdly
    /// large longitudes (beyond about 1e15 rad) lose precision in the wrap, as any `f64` angle does.
    ///
    /// # Errors
    /// [`CoordError::NonFinite`] for NaN or infinite input; [`CoordError::LatitudeOutOfRange`]
    /// when `|lat| > pi/2`.
    pub fn new(lat: Radians, lon: Radians, height: Meters) -> Result<Self, CoordError> {
        if !(lat.0.is_finite() && lon.0.is_finite() && height.0.is_finite()) {
            return Err(CoordError::NonFinite);
        }
        if lat.0.abs() > core::f64::consts::FRAC_PI_2 {
            return Err(CoordError::LatitudeOutOfRange(lat.0));
        }
        Ok(Self {
            lat,
            lon: math::wrap_pi(lon),
            height,
        })
    }

    /// Like [`Geodetic::new`] with degrees for latitude and longitude and meters for height.
    ///
    /// # Errors
    /// See [`Geodetic::new`].
    pub fn from_degrees(lat_deg: f64, lon_deg: f64, height_m: f64) -> Result<Self, CoordError> {
        Self::new(
            Radians::from_degrees(lat_deg),
            Radians::from_degrees(lon_deg),
            Meters(height_m),
        )
    }

    /// Latitude, radians in `[-pi/2, pi/2]`.
    #[must_use]
    pub fn lat(self) -> Radians {
        self.lat
    }

    /// Longitude, radians in `(-pi, pi]`.
    #[must_use]
    pub fn lon(self) -> Radians {
        self.lon
    }

    /// Height above the WGS-84 ellipsoid, meters.
    #[must_use]
    pub fn height(self) -> Meters {
        self.height
    }

    /// The same latitude and longitude at another height.
    ///
    /// # Errors
    /// [`CoordError::NonFinite`] when `h` is NaN or infinite.
    pub fn with_height(self, h: Meters) -> Result<Self, CoordError> {
        Self::new(self.lat, self.lon, h)
    }

    /// Converts to ECEF (EPSG method 9602, closed form).
    #[must_use]
    pub fn to_ecef(self) -> Ecef {
        use wgs84::{A, E2};
        let (sin_lat, cos_lat) = math::sin_cos(self.lat.0);
        let (sin_lon, cos_lon) = math::sin_cos(self.lon.0);
        let n = A / (1.0 - E2 * sin_lat * sin_lat).sqrt();
        let h = self.height.0;
        Ecef(Vec3::new(
            (n + h) * cos_lat * cos_lon,
            (n + h) * cos_lat * sin_lon,
            (n * (1.0 - E2) + h) * sin_lat,
        ))
    }
}

/// User-facing position: ECEF or WGS-84 geodetic (ASM-044).
///
/// Either form is accepted at the API edge and converted to ECEF exactly once. Behind the edge
/// only ECEF is stored; geodetic is derived on demand for terrain queries and display.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum Position {
    /// Already ECEF, meters.
    Ecef(Ecef),
    /// Latitude, longitude, height above the ellipsoid.
    Geodetic(Geodetic),
}

impl Position {
    /// The canonical ECEF position (the identity for [`Position::Ecef`]).
    #[must_use]
    pub fn to_ecef(self) -> Ecef {
        match self {
            Self::Ecef(e) => e,
            Self::Geodetic(g) => g.to_ecef(),
        }
    }
}

impl From<Ecef> for Position {
    fn from(e: Ecef) -> Self {
        Self::Ecef(e)
    }
}

impl From<Geodetic> for Position {
    fn from(g: Geodetic) -> Self {
        Self::Geodetic(g)
    }
}

/// Local tangent-plane coordinates, East-North-Up, meters (or the vector's own unit).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Enu {
    /// Toward local east.
    pub east: f64,
    /// Toward local north.
    pub north: f64,
    /// Along the ellipsoid normal, away from the Earth.
    pub up: f64,
}

/// Local tangent-plane coordinates, North-East-Down, meters (or the vector's own unit).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ned {
    /// Toward local north.
    pub north: f64,
    /// Toward local east.
    pub east: f64,
    /// Along the ellipsoid normal, toward the Earth.
    pub down: f64,
}

impl From<Enu> for Ned {
    /// Exact axis swap with `down = -up`.
    fn from(e: Enu) -> Ned {
        Ned {
            north: e.north,
            east: e.east,
            down: -e.up,
        }
    }
}

impl From<Ned> for Enu {
    /// Exact axis swap with `up = -down`.
    fn from(n: Ned) -> Enu {
        Enu {
            east: n.east,
            north: n.north,
            up: -n.down,
        }
    }
}

/// Local tangent plane at an origin (ENU/NED axes at that point).
///
/// Built once (one `sin_cos` per angle) and reused for many points.
#[derive(Debug, Clone, PartialEq)]
pub struct LocalFrame {
    origin: Ecef,
    /// Row-major; the columns are the ECEF coordinates of the east, north and up unit vectors.
    ecef_from_enu: [[f64; 3]; 3],
    /// Rotation taking NED axes to ECEF axes (columns north, east, down), computed once.
    ned_to_ecef: Quaternion,
}

impl LocalFrame {
    /// The frame whose origin is `origin`.
    #[must_use]
    pub fn at(origin: Geodetic) -> Self {
        let (sin_lat, cos_lat) = math::sin_cos(origin.lat.0);
        let (sin_lon, cos_lon) = math::sin_cos(origin.lon.0);
        let m = [
            [-sin_lon, -sin_lat * cos_lon, cos_lat * cos_lon],
            [cos_lon, -sin_lat * sin_lon, cos_lat * sin_lon],
            [0.0, cos_lat, sin_lat],
        ];
        Self {
            origin: origin.to_ecef(),
            ecef_from_enu: m,
            ned_to_ecef: Quaternion::from_rotation_matrix([
                [m[0][1], m[0][0], -m[0][2]],
                [m[1][1], m[1][0], -m[1][2]],
                [m[2][1], m[2][0], -m[2][2]],
            ]),
        }
    }

    /// The origin in ECEF.
    #[must_use]
    pub fn origin(&self) -> Ecef {
        self.origin
    }

    /// ECEF point to ENU coordinates relative to the origin.
    #[must_use]
    pub fn to_enu(&self, p: Ecef) -> Enu {
        self.vector_to_enu(p - self.origin)
    }

    /// ENU coordinates relative to the origin to an ECEF point.
    #[must_use]
    pub fn from_enu(&self, e: Enu) -> Ecef {
        self.origin + self.vector_from_enu(e)
    }

    /// ECEF point to NED coordinates relative to the origin.
    #[must_use]
    pub fn to_ned(&self, p: Ecef) -> Ned {
        Ned::from(self.to_enu(p))
    }

    /// NED coordinates relative to the origin to an ECEF point.
    #[must_use]
    pub fn from_ned(&self, n: Ned) -> Ecef {
        self.from_enu(Enu::from(n))
    }

    /// Rotates an ECEF vector into ENU components (rotation only, no translation).
    #[must_use]
    pub fn vector_to_enu(&self, v: Vec3) -> Enu {
        let m = &self.ecef_from_enu;
        let col = |j: usize| m[0][j] * v.x + m[1][j] * v.y + m[2][j] * v.z;
        Enu {
            east: col(0),
            north: col(1),
            up: col(2),
        }
    }

    /// Rotates ENU components into an ECEF vector (rotation only, no translation).
    #[must_use]
    pub fn vector_from_enu(&self, e: Enu) -> Vec3 {
        let m = &self.ecef_from_enu;
        let row = |i: usize| m[i][0] * e.east + m[i][1] * e.north + m[i][2] * e.up;
        Vec3::new(row(0), row(1), row(2))
    }

    /// The rotation taking body FRD axes to ECEF axes for the given Euler angles at this frame.
    #[must_use]
    pub fn orientation_from_euler(&self, e: Euler) -> Quaternion {
        self.ned_to_ecef.mul(e.to_quaternion())
    }

    /// The Euler angles (relative to this frame's NED axes) of a body FRD to ECEF orientation.
    #[must_use]
    pub fn euler_from_orientation(&self, q: Quaternion) -> Euler {
        Euler::from_quaternion(self.ned_to_ecef.conjugate().mul(q))
    }
}

/// Invalid coordinate input.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum CoordError {
    /// Latitude outside `[-pi/2, pi/2]`.
    #[error("latitude {0} rad outside [-pi/2, pi/2]")]
    LatitudeOutOfRange(f64),
    /// NaN or infinite coordinate.
    #[error("non-finite coordinate")]
    NonFinite,
}
