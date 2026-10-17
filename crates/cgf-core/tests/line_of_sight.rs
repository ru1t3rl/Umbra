//! M3 WI-3: `TerrainProvider::line_of_sight` default body and `Arc` forwarding.
//! Requirements: REQ-C-6, ADR 0015, ASM-080. Existing implementors compile unchanged.

#![forbid(unsafe_code)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use cgf_core::{Ecef, Enu, FlatTerrain, Geodetic, Meters, TerrainProvider};

fn at(lat: f64, lon: f64, h: f64) -> Ecef {
    Geodetic::from_degrees(lat, lon, h)
        .unwrap_or_else(|e| unreachable!("valid geodetic: {e}"))
        .to_ecef()
}

fn flat() -> FlatTerrain {
    FlatTerrain {
        height: Meters(0.0),
    }
}

/// Terrain written against the M1/M2 trait (no `line_of_sight`): must compile unchanged.
struct LegacyTerrain;

impl TerrainProvider for LegacyTerrain {
    fn height_at(&self, _: Geodetic) -> Meters {
        Meters(0.0)
    }
    fn surface_normal_at(&self, _: Geodetic) -> Enu {
        Enu {
            east: 0.0,
            north: 0.0,
            up: 1.0,
        }
    }
}

/// Flat terrain with a wall (high ground) between lon 0.0 and lon 0.05, counting height queries.
struct Ridge {
    queries: AtomicUsize,
}

impl TerrainProvider for Ridge {
    fn height_at(&self, g: Geodetic) -> Meters {
        self.queries.fetch_add(1, Ordering::Relaxed);
        if g.lon().0.to_degrees() > 0.02 && g.lon().0.to_degrees() < 0.03 {
            Meters(100.0)
        } else {
            Meters(0.0)
        }
    }
    fn surface_normal_at(&self, g: Geodetic) -> Enu {
        LegacyTerrain.surface_normal_at(g)
    }
}

/// ASM-080: two points 2 m up, 3 km apart, see each other over flat terrain.
#[test]
fn given_two_points_2m_up_3km_apart_when_flat_then_visible() {
    let a = at(0.0, 0.0, 2.0);
    let b = at(
        0.0,
        3_000.0 / 6_378_137.0 * 180.0 / std::f64::consts::PI,
        2.0,
    );

    assert!(flat().line_of_sight(a, b));
    assert!(flat().line_of_sight(b, a));
}

/// ASM-080: two points on the ground 10 km apart are blocked by the curvature (sag ~2 m).
#[test]
fn given_two_ground_points_10km_apart_when_flat_then_blocked_by_curvature() {
    let deg = 10_000.0 / 6_378_137.0 * 180.0 / std::f64::consts::PI;
    let a = at(0.0, 0.0, 0.0);
    let b = at(0.0, deg, 0.0);

    assert!(!flat().line_of_sight(a, b));
}

/// ASM-080: legacy implementors get the default body.
#[test]
fn given_legacy_implementor_when_los_then_default_body_applies() {
    let a = at(0.0, 0.0, 2.0);
    let b = at(0.0, 0.01, 2.0);

    assert!(LegacyTerrain.line_of_sight(a, b));
}

/// ASM-080: a ridge between the points blocks the line; endpoints beyond it do not.
#[test]
fn given_ridge_between_points_when_los_then_blocked_only_across_it() {
    let ridge = Ridge {
        queries: AtomicUsize::new(0),
    };
    let (a, b) = (at(0.0, 0.0, 10.0), at(0.0, 0.05, 10.0));
    let (c, d) = (at(0.0, 0.035, 10.0), at(0.0, 0.05, 10.0));

    assert!(!ridge.line_of_sight(a, b));
    assert!(ridge.line_of_sight(c, d));
}

/// ASM-080: sample count is clamp(ceil(d / 20), 2, 512); coincident points still sample twice.
#[test]
fn sample_count_is_bounded() {
    let count = |a: Ecef, b: Ecef| {
        let t = Ridge {
            queries: AtomicUsize::new(0),
        };
        let _ = t.line_of_sight(a, b);
        t.queries.load(Ordering::Relaxed)
    };
    let p = at(0.0, 0.0, 5.0);

    assert_eq!(count(p, p), 2);
    // 90 m apart: ceil(90 / 20) = 5.
    let hundred = at(0.0, 90.0 / 6_378_137.0 * 180.0 / std::f64::consts::PI, 5.0);
    assert_eq!(count(p, hundred), 5);
    // 1000 km apart, 200 km up (clear of the curvature): capped at 512.
    let (high, far) = (at(0.0, 0.0, 200_000.0), at(0.0, 9.0, 200_000.0));
    assert_eq!(count(high, far), 512);
}

/// ASM-080: non-finite endpoints are never visible (and do not panic).
#[test]
fn given_non_finite_endpoint_when_los_then_blocked() {
    let nan = Ecef(cgf_core::Vec3 {
        x: f64::NAN,
        y: 0.0,
        z: 0.0,
    });

    assert!(!flat().line_of_sight(nan, at(0.0, 0.0, 0.0)));
    assert!(!flat().line_of_sight(at(0.0, 0.0, 0.0), nan));
}

/// ASM-080: `Arc<T>` forwards an overriding implementation, not the default body.
#[test]
fn given_arc_of_overriding_provider_when_los_then_forwards() {
    struct NeverVisible;
    impl TerrainProvider for NeverVisible {
        fn height_at(&self, _: Geodetic) -> Meters {
            Meters(0.0)
        }
        fn surface_normal_at(&self, g: Geodetic) -> Enu {
            LegacyTerrain.surface_normal_at(g)
        }
        fn line_of_sight(&self, _: Ecef, _: Ecef) -> bool {
            false
        }
    }
    let (a, b) = (at(0.0, 0.0, 2.0), at(0.0, 0.001, 2.0));

    let arc = Arc::new(NeverVisible);
    let dynamic: Arc<dyn TerrainProvider> = arc.clone();
    let nested = Arc::new(dynamic.clone());

    assert!(!arc.line_of_sight(a, b));
    assert!(!dynamic.line_of_sight(a, b));
    assert!(!nested.line_of_sight(a, b));
    assert!(flat().line_of_sight(a, b));
}
