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

fn east_of(origin_lat_lon: (f64, f64), metres: f64, h: f64) -> Ecef {
    let deg = metres / 6_378_137.0 * 180.0 / std::f64::consts::PI;
    at(origin_lat_lon.0, origin_lat_lon.1 + deg, h)
}

/// ASM-080: 20 km apart at 2 m eye height the curvature blocks the line (sag ~7.8 m).
#[test]
fn given_two_points_2m_up_20km_apart_when_flat_then_blocked_by_curvature() {
    let a = at(0.0, 0.0, 2.0);
    let b = east_of((0.0, 0.0), 20_000.0, 2.0);

    assert!(!flat().line_of_sight(a, b));
}

fn counted(a: Ecef, b: Ecef) -> (usize, bool) {
    let t = Ridge {
        queries: AtomicUsize::new(0),
    };
    // Fully clear of the ridge: both points high above ground.
    let visible = t.line_of_sight(a, b);
    (t.queries.load(Ordering::Relaxed), visible)
}

/// ASM-080: the count is clamp(ceil(d / 20), 2, 512) at the boundaries, and the line is visible.
#[test]
fn sample_count_boundaries_use_chord_length() {
    let up = |lon: f64, h: f64| at(0.0, lon, h);
    let chord = |a: Ecef, b: Ecef| a.distance(b).0;
    let h = 500_000.0;
    let a = up(10.0, h);
    // Find points on the same latitude whose chord is exactly the wanted length by bisection.
    let at_chord = |want: f64| {
        let (mut lo, mut hi) = (0.0_f64, 1.0_f64);
        for _ in 0..80 {
            let mid = f64::midpoint(lo, hi);
            if chord(a, up(10.0 + mid, h)) < want {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        up(10.0 + lo, h)
    };
    for (want, expect) in [(40.0, 2), (60.0, 3), (10_240.0, 512)] {
        let b = at_chord(want);
        assert_eq!(counted(a, b), (expect, true), "chord {want}");
    }
    // Just above 40 m needs a third sample; just above the cap stays at 512.
    assert_eq!(counted(a, at_chord(40.5)), (3, true));
    assert_eq!(counted(a, at_chord(10_300.0)), (512, true));
}

/// ASM-080: infinite endpoints, and inf - inf (NaN displacement), are blocked without panic.
#[test]
fn given_infinite_endpoints_when_los_then_blocked() {
    let inf = Ecef(cgf_core::Vec3 {
        x: f64::INFINITY,
        y: 0.0,
        z: 0.0,
    });
    let p = at(0.0, 0.0, 0.0);

    assert!(!flat().line_of_sight(inf, p));
    assert!(!flat().line_of_sight(p, inf));
    assert!(!flat().line_of_sight(inf, inf));
}

/// ASM-080 (documents the strictness): at 0 m eye height 1 m apart on flat ground the chord dips
/// below the ellipsoid, so the line is blocked. Callers raise endpoints by an eye height.
#[test]
fn given_zero_eye_height_1m_apart_on_flat_ground_then_blocked() {
    let a = at(0.0, 0.0, 0.0);
    let b = east_of((0.0, 0.0), 1.0, 0.0);

    assert!(!flat().line_of_sight(a, b));
}
