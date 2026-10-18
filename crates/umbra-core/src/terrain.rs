//! Terrain queries for motion models (REQ-C-6 partial, ASM-032).
//!
//! Terrain is queried by geodetic position; heights are above the WGS-84 ellipsoid.
//! [`TerrainProvider::line_of_sight`] has a default body (ADR 0015, ASM-080).

use std::sync::Arc;

use crate::coords::{Ecef, Enu, Geodetic};
use crate::units::Meters;

/// Read-only terrain model. Implementations must be deterministic and thread-safe.
pub trait TerrainProvider: Send + Sync {
    /// Terrain height above the WGS-84 ellipsoid at the point's latitude and longitude; the
    /// point's own height is ignored.
    fn height_at(&self, at: Geodetic) -> Meters;

    /// Unit surface normal in the local ENU frame at `at`.
    fn surface_normal_at(&self, at: Geodetic) -> Enu;

    /// Whether `to` is visible from `from` (ECEF points, already raised to eye height).
    ///
    /// The default samples the straight segment at `n = clamp(ceil(d / 20 m), 2, 512)` evenly
    /// spaced interior points and is blocked iff a sample's height is strictly below
    /// [`TerrainProvider::height_at`] there (ASM-080). Non-finite endpoints are never visible.
    /// Override it for terrain with a cheaper or exact test.
    fn line_of_sight(&self, from: Ecef, to: Ecef) -> bool {
        default_line_of_sight(self, from, to)
    }
}

/// Spacing of default line-of-sight samples, meters.
const LOS_SPACING_M: f64 = 20.0;
/// Bounds on the number of default line-of-sight samples.
const LOS_MIN_SAMPLES: u32 = 2;
const LOS_MAX_SAMPLES: u32 = 512;

/// `clamp(ceil(d / 20 m), 2, 512)`.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "d is finite and >= 0 (asserted); the value is clamped to [2, 512] in f64 first, so the cast is exact"
)]
fn sample_count(d: f64) -> u32 {
    debug_assert!(d.is_finite() && d >= 0.0);
    (d / LOS_SPACING_M)
        .ceil()
        .clamp(f64::from(LOS_MIN_SAMPLES), f64::from(LOS_MAX_SAMPLES)) as u32
}

fn default_line_of_sight<T: TerrainProvider + ?Sized>(terrain: &T, from: Ecef, to: Ecef) -> bool {
    let step = to - from;
    let d = step.norm();
    if !d.is_finite() {
        return false;
    }
    let n = sample_count(d);
    (1..=n).all(|i| {
        let t = f64::from(i) / f64::from(n + 1);
        let g = (from + step * t).to_geodetic();
        g.height().0 >= terrain.height_at(g).0
    })
}

/// Terrain of constant ellipsoidal height with the normal pointing straight up.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlatTerrain {
    /// Height above the ellipsoid, meters, everywhere.
    pub height: Meters,
}

impl TerrainProvider for FlatTerrain {
    fn height_at(&self, _at: Geodetic) -> Meters {
        self.height
    }

    fn surface_normal_at(&self, _at: Geodetic) -> Enu {
        Enu {
            east: 0.0,
            north: 0.0,
            up: 1.0,
        }
    }
}

/// Lets a factory-built `Arc<dyn TerrainProvider>` be passed where a provider is owned (ADR 0008).
impl<T: TerrainProvider + ?Sized> TerrainProvider for Arc<T> {
    fn height_at(&self, at: Geodetic) -> Meters {
        (**self).height_at(at)
    }

    fn surface_normal_at(&self, at: Geodetic) -> Enu {
        (**self).surface_normal_at(at)
    }

    fn line_of_sight(&self, from: Ecef, to: Ecef) -> bool {
        (**self).line_of_sight(from, to)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Tilted;

    impl TerrainProvider for Tilted {
        fn height_at(&self, _: Geodetic) -> Meters {
            Meters(7.0)
        }
        fn surface_normal_at(&self, _: Geodetic) -> Enu {
            Enu {
                east: 0.6,
                north: 0.0,
                up: 0.8,
            }
        }
    }

    #[test]
    fn arc_forwards_both_queries() {
        let at = Geodetic::new(
            crate::units::Radians(0.0),
            crate::units::Radians(0.0),
            Meters(0.0),
        )
        .unwrap();
        let arc: Arc<dyn TerrainProvider> = Arc::new(Tilted);
        let n = arc.surface_normal_at(at);
        assert_eq!((n.east, n.north, n.up), (0.6, 0.0, 0.8));
        assert_eq!(arc.height_at(at), Meters(7.0));
        let nested = Arc::new(arc);
        assert_eq!(nested.surface_normal_at(at).east, 0.6);
    }

    #[test]
    fn arc_of_provider_is_a_provider() {
        let flat = Arc::new(FlatTerrain {
            height: Meters(3.0),
        });
        let dynamic: Arc<dyn TerrainProvider> = flat.clone();
        let at = Geodetic::new(
            crate::units::Radians(0.0),
            crate::units::Radians(0.0),
            Meters(0.0),
        )
        .unwrap();
        assert_eq!(flat.height_at(at), Meters(3.0));
        assert_eq!(dynamic.height_at(at), Meters(3.0));
        let boxed: Box<dyn TerrainProvider> = Box::new(dynamic);
        assert_eq!(boxed.surface_normal_at(at).up, 1.0);
    }
}
