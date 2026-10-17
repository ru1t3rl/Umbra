//! Terrain queries for motion models (REQ-C-6 partial, ASM-032).
//!
//! `line_of_sight` arrives with perception in M3. Terrain is queried by geodetic position;
//! heights are above the WGS-84 ellipsoid.

use std::sync::Arc;

use crate::coords::{Enu, Geodetic};
use crate::units::Meters;

/// Read-only terrain model. Implementations must be deterministic and thread-safe.
pub trait TerrainProvider: Send + Sync {
    /// Terrain height above the WGS-84 ellipsoid at the point's latitude and longitude; the
    /// point's own height is ignored.
    fn height_at(&self, at: Geodetic) -> Meters;

    /// Unit surface normal in the local ENU frame at `at`.
    fn surface_normal_at(&self, at: Geodetic) -> Enu;
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
}

#[cfg(test)]
mod tests {
    use super::*;

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
