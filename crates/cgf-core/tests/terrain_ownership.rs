//! WI-7 domain types: `Intent`, `Ownership`/`RemoteSource`, `FlatTerrain`.
//! Requirements: REQ-B-2 (subset), REQ-K-5, REQ-C-6 (partial), ASM-030, ASM-032, ASM-041.

#![forbid(unsafe_code)]

use cgf_core::{
    Ecef, Enu, FlatTerrain, Geodetic, Intent, Meters, MetersPerSecond, Ownership, RemoteSource,
    TerrainProvider, Vec3,
};

#[expect(clippy::expect_used, reason = "test helper")]
fn geo(lat: f64, lon: f64) -> Geodetic {
    Geodetic::from_degrees(lat, lon, 0.0).expect("valid geodetic")
}

/// REQ-C-6, ASM-032: flat terrain reports its configured height everywhere.
#[test]
fn given_flat_terrain_when_height_queried_then_returns_configured_height() {
    let t = FlatTerrain {
        height: Meters(120.0),
    };

    assert_eq!(t.height_at(geo(0.0, 0.0)), Meters(120.0));
    assert_eq!(t.height_at(geo(52.0, 4.0)), Meters(120.0));
    assert_eq!(t.height_at(geo(-89.0, 179.0)), Meters(120.0));
}

/// REQ-C-6, ASM-032: the height of the queried point is ignored.
#[test]
fn given_flat_terrain_when_queried_at_other_height_then_ignores_it() {
    let t = FlatTerrain {
        height: Meters(5.0),
    };

    let at = geo(10.0, 10.0).with_height(Meters(9_000.0));

    assert_eq!(t.height_at(at), Meters(5.0));
}

/// REQ-C-6, ASM-032: the surface normal of flat terrain is local up in ENU.
#[test]
fn given_flat_terrain_when_normal_queried_then_is_enu_up() {
    let t = FlatTerrain {
        height: Meters(120.0),
    };

    assert_eq!(
        t.surface_normal_at(geo(52.0, 4.0)),
        Enu {
            east: 0.0,
            north: 0.0,
            up: 1.0
        }
    );
}

/// REQ-K-5: only `Owned` is owned.
#[test]
fn given_ownership_when_asked_is_owned_then_only_owned_is_true() {
    let remote = Ownership::Remote {
        source: RemoteSource::new("dis.exercise.1"),
    };

    assert!(Ownership::Owned.is_owned());
    assert!(!remote.is_owned());
}

/// ASM-041: the remote source is an opaque label that round-trips.
#[test]
fn given_remote_source_when_read_back_then_label_is_unchanged() {
    assert_eq!(
        RemoteSource::new("dis.exercise.1").as_str(),
        "dis.exercise.1"
    );
    assert!(RemoteSource::new("a") < RemoteSource::new("b"));
}

/// REQ-B-2, ASM-030: the intent subset compares by value and clones.
#[test]
fn given_intents_when_compared_then_equality_is_by_value() {
    let dest = Ecef(Vec3 {
        x: 1.0,
        y: 2.0,
        z: 3.0,
    });
    let a = Intent::MoveTo { destination: dest };

    assert_eq!(a.clone(), Intent::MoveTo { destination: dest });
    assert_ne!(a, Intent::Hold);
    assert_eq!(
        Intent::SetSpeed(MetersPerSecond(3.0)),
        Intent::SetSpeed(MetersPerSecond(3.0))
    );
}

/// ASM-030: `Intent` is `#[non_exhaustive]`; downstream matches need a wildcard (this compiles
/// only with one, which is the point: adding variants in M3 must not break consumers).
#[test]
fn given_intent_when_matched_with_wildcard_then_unknown_variants_are_ignorable() {
    let describe = |i: &Intent| match i {
        Intent::Hold => "hold",
        Intent::MoveTo { .. } => "move",
        _ => "other",
    };

    assert_eq!(describe(&Intent::Hold), "hold");
    assert_eq!(describe(&Intent::SetSpeed(MetersPerSecond(1.0))), "other");
}
