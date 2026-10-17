//! AT-3 (reference-value part): WGS-84, ECEF, geodetic, ENU/NED and `LocalFrame`.
//! Requirements: REQ-C-3, REQ-C-4, D-15, ASM-033.
//!
//! Reference values and how each was verified (2026-10-17):
//! - NIMA TR8350.2 constants: the defining values (a, 1/f) are exact; `b`, `e2`, `e'2` were
//!   recomputed in Python (`a * (1 - f)`, `2f - f^2`, `e2 / (1 - e2)`) and agree with the published
//!   Table 3.1 values to the digits quoted.
//! - IOGP Publication 373-7-2 (Geomatics Guidance Note 7, part 2), September 2019, downloaded from
//!   iogp.org and read directly: section 4.1.1 (EPSG method 9602) worked example and section 4.1.2
//!   (method 9836) worked example. The plan's numbers were correct; the document gives the
//!   topocentric origin as geocentric X, Y, Z (3652755.3058, 319574.6799, 5201547.3536 m), which
//!   an independent Python Newton solution shows is 55 N, 5 E, h = 199.99999 m, i.e. the plan's
//!   "55 N 5 E 200 m". The document uses 1/f = 298.2572236 (rounded); with the WGS-84 value
//!   298.257223563 the results differ by less than 2e-6 m, far below the 1e-3 m tolerances.
//! - Extra points: the EPSG 9602 forward formula evaluated in Python double precision with the
//!   exact WGS-84 constants (independent of the Rust code); tolerance 1e-6 m. (A first draft used 1/f = 298.2572236 and was
//!   off by up to 6e-7 m: the values here use the exact 298.257223563.)
//! - Analytic points (equator, poles) follow from the definitions.

#![forbid(unsafe_code)]

use core::f64::consts::{FRAC_PI_2, PI};

use cgf_core::coords::wgs84;
use cgf_core::math;
use cgf_core::{
    CoordError, Ecef, Enu, Euler, Geodetic, LocalFrame, Meters, Ned, Position, Quaternion, Radians,
    Vec3,
};

const A: f64 = 6_378_137.0;
/// `a * (1 - 1/298.257223563)`, computed in Python.
const B: f64 = 6_356_752.314_245_179;

#[expect(clippy::expect_used, reason = "test helper")]
fn geo(lat_deg: f64, lon_deg: f64, h: f64) -> Geodetic {
    Geodetic::from_degrees(lat_deg, lon_deg, h).expect("valid geodetic")
}

fn ecef(x: f64, y: f64, z: f64) -> Ecef {
    Ecef(Vec3 { x, y, z })
}

fn assert_ecef_near(got: Ecef, want: (f64, f64, f64), tol: f64) {
    let (dx, dy, dz) = (got.0.x - want.0, got.0.y - want.1, got.0.z - want.2);
    assert!(
        dx.abs() <= tol && dy.abs() <= tol && dz.abs() <= tol,
        "got {:?}, want {want:?}, tol {tol}",
        got.0
    );
}

fn assert_geodetic_near(got: Geodetic, want: (f64, f64, f64), lat_lon_tol: f64, h_tol: f64) {
    let dlat = (got.lat().0 - want.0).abs();
    let dlon = (got.lon().0 - want.1).abs();
    let dh = (got.height().0 - want.2).abs();
    assert!(
        dlat <= lat_lon_tol && dlon <= lat_lon_tol && dh <= h_tol,
        "got ({}, {}, {}), want {want:?}",
        got.lat().0,
        got.lon().0,
        got.height().0
    );
}

fn dms(deg: f64, min: f64, sec: f64) -> f64 {
    (deg + min / 60.0 + sec / 3600.0).to_radians()
}

/// REQ-C-3, D-15: WGS-84 constants (NIMA TR8350.2, 3rd ed., Tables 3.1 and 3.3).
#[test]
fn wgs84_constants_match_nima_tr8350_2() {
    assert_eq!(wgs84::A, 6_378_137.0);
    assert_eq!(wgs84::INV_F, 298.257_223_563);
    assert!((wgs84::F - 1.0 / 298.257_223_563).abs() < 1e-18);
    assert!((wgs84::B - 6_356_752.314_2).abs() < 1e-4);
    assert!((wgs84::E2 - 0.006_694_379_990_14).abs() < 1e-14);
    assert!((wgs84::EP2 - 0.006_739_496_742_28).abs() < 1e-14);
}

// ---- Analytic points -------------------------------------------------------------------------

/// REQ-C-3: geodetic to ECEF at the four axis points and at height.
#[test]
fn geodetic_to_ecef_analytic_points() {
    assert_ecef_near(geo(0.0, 0.0, 0.0).to_ecef(), (A, 0.0, 0.0), 1e-6);
    assert_ecef_near(geo(0.0, 90.0, 0.0).to_ecef(), (0.0, A, 0.0), 1e-6);
    assert_ecef_near(geo(0.0, 180.0, 0.0).to_ecef(), (-A, 0.0, 0.0), 1e-6);
    assert_ecef_near(
        geo(0.0, 0.0, 1000.0).to_ecef(),
        (A + 1000.0, 0.0, 0.0),
        1e-6,
    );
    assert_ecef_near(geo(90.0, 0.0, 0.0).to_ecef(), (0.0, 0.0, B), 1e-6);
    assert_ecef_near(geo(90.0, 123.0, 0.0).to_ecef(), (0.0, 0.0, B), 1e-6);
    assert_ecef_near(geo(-90.0, 0.0, 0.0).to_ecef(), (0.0, 0.0, -B), 1e-6);
}

/// REQ-C-3: ECEF to geodetic at the analytic points (lat/lon 1e-12 rad, h 1e-6 m).
#[test]
fn ecef_to_geodetic_analytic_points() {
    assert_geodetic_near(
        ecef(A, 0.0, 0.0).to_geodetic(),
        (0.0, 0.0, 0.0),
        1e-12,
        1e-6,
    );
    assert_geodetic_near(
        ecef(0.0, A, 0.0).to_geodetic(),
        (0.0, FRAC_PI_2, 0.0),
        1e-12,
        1e-6,
    );
    assert_geodetic_near(
        ecef(-A, 0.0, 0.0).to_geodetic(),
        (0.0, PI, 0.0),
        1e-12,
        1e-6,
    );
    assert_geodetic_near(
        ecef(A + 1000.0, 0.0, 0.0).to_geodetic(),
        (0.0, 0.0, 1000.0),
        1e-12,
        1e-6,
    );
    let north = ecef(0.0, 0.0, B).to_geodetic();
    assert!((north.lat().0 - FRAC_PI_2).abs() < 1e-12 && north.height().0.abs() < 1e-6);
    let south = ecef(0.0, 0.0, -B).to_geodetic();
    assert!((south.lat().0 + FRAC_PI_2).abs() < 1e-12 && south.height().0.abs() < 1e-6);
}

// ---- IOGP 373-7-2 section 4.1.1, EPSG method 9602 --------------------------------------------

/// REQ-C-3: IOGP 373-7-2 (Sept 2019) section 4.1.1 worked example, forward direction:
/// 53 48' 33.820" N, 2 07' 46.380" E, h 73.000 m gives X 3771793.968, Y 140253.342, Z 5124304.349.
#[test]
fn geodetic_to_ecef_iogp_9602_example() {
    let g = Geodetic::new(
        Radians(dms(53.0, 48.0, 33.820)),
        Radians(dms(2.0, 7.0, 46.380)),
        Meters(73.0),
    );

    assert!(g.is_ok_and(|g| {
        let e = g.to_ecef();
        (e.0.x - 3_771_793.968).abs() < 1e-3
            && (e.0.y - 140_253.342).abs() < 1e-3
            && (e.0.z - 5_124_304.349).abs() < 1e-3
    }));
}

/// REQ-C-3: the same example, inverse direction. The document prints latitude and longitude to
/// 0.001 arc second (2.4e-9 rad) and the height to 1 m resolution of the example (73.0 m).
#[test]
fn ecef_to_geodetic_iogp_9602_example() {
    let g = ecef(3_771_793.968, 140_253.342, 5_124_304.349).to_geodetic();

    assert_geodetic_near(
        g,
        (dms(53.0, 48.0, 33.820), dms(2.0, 7.0, 46.380), 73.0),
        2.5e-9,
        1e-3,
    );
}

/// Independent Python (double precision) evaluation of the EPSG 9602 forward formula with the
/// exact WGS-84 constants, for points in the southern and western hemispheres, near the pole,
/// below the ellipsoid and at high altitude: `(lat deg, lon deg, h m, (X, Y, Z))`.
type ExtraPoint = (f64, f64, f64, (f64, f64, f64));
const EXTRA_POINTS: [ExtraPoint; 5] = [
    (
        -33.8688,
        151.2093,
        58.0,
        (
            -4_646_093.477_288_303,
            2_553_229.535_817_071_3,
            -3_534_404.710_910_369,
        ),
    ),
    (
        52.0,
        4.0,
        0.0,
        (
            3_925_375.100_749_011_6,
            274_488.966_477_814_36,
            5_002_803.345_482_635,
        ),
    ),
    (
        -89.0,
        -120.5,
        5000.0,
        (
            -56_730.332_154_311_49,
            -96_308.992_645_493_52,
            -6_360_776.865_115_268,
        ),
    ),
    (
        35.0,
        -170.0,
        -1000.0,
        (
            -5_150_158.196_508_346,
            -908_111.844_961_269_6,
            3_637_293.332_941_744,
        ),
    ),
    (
        0.0,
        179.999,
        300_000.0,
        (-6_678_136.998_982_862, 116.555_478_545_335_96, 0.0),
    ),
];

/// REQ-C-3: forward conversion matches the independent reference at the extra points (1e-6 m).
#[test]
fn geodetic_to_ecef_extra_points_match_python_reference() {
    for (lat, lon, h, want) in EXTRA_POINTS {
        assert_ecef_near(geo(lat, lon, h).to_ecef(), want, 1e-6);
    }
}

/// REQ-C-3: the inverse of the extra points recovers the geodetic input.
#[test]
fn ecef_to_geodetic_extra_points_recover_the_input() {
    for (lat, lon, h, (x, y, z)) in EXTRA_POINTS {
        assert_geodetic_near(
            ecef(x, y, z).to_geodetic(),
            (lat.to_radians(), lon.to_radians(), h),
            1e-11,
            1e-6,
        );
    }
}

// ---- IOGP 373-7-2 section 4.1.2, EPSG method 9836 --------------------------------------------

const TOPO_ORIGIN: (f64, f64, f64) = (3_652_755.305_8, 319_574.679_9, 5_201_547.353_6);
const TOPO_POINT: (f64, f64, f64) = (3_771_793.968, 140_253.342, 5_124_304.349);
/// (U, V, W) = (east, north, up) printed in the document.
const TOPO_ENU: (f64, f64, f64) = (-189_013.869, -128_642.040, -4_220.171);

/// REQ-C-3: IOGP 373-7-2 section 4.1.2 example, forward, origin given as geocentric X, Y, Z
/// exactly as in the document: U -189013.869, V -128642.040, W -4220.171 m (tolerance 1e-3 m).
#[test]
fn ecef_to_enu_iogp_9836_example() {
    let frame = LocalFrame::at(ecef(TOPO_ORIGIN.0, TOPO_ORIGIN.1, TOPO_ORIGIN.2).to_geodetic());

    let enu = frame.to_enu(ecef(TOPO_POINT.0, TOPO_POINT.1, TOPO_POINT.2));

    assert!((enu.east - TOPO_ENU.0).abs() < 1e-3, "{enu:?}");
    assert!((enu.north - TOPO_ENU.1).abs() < 1e-3, "{enu:?}");
    assert!((enu.up - TOPO_ENU.2).abs() < 1e-3, "{enu:?}");
}

/// REQ-C-3: the same example with the origin stated geodetically (55 N, 5 E, 200 m), as the
/// plan wrote it; verified independently to coincide with the document's geocentric origin.
#[test]
fn ecef_to_enu_iogp_9836_example_from_geodetic_origin() {
    let frame = LocalFrame::at(geo(55.0, 5.0, 200.0));

    let enu = frame.to_enu(ecef(TOPO_POINT.0, TOPO_POINT.1, TOPO_POINT.2));

    assert!((enu.east - TOPO_ENU.0).abs() < 1e-3, "{enu:?}");
    assert!((enu.north - TOPO_ENU.1).abs() < 1e-3, "{enu:?}");
    assert!((enu.up - TOPO_ENU.2).abs() < 1e-3, "{enu:?}");
}

/// REQ-C-3: IOGP 9836, reverse direction ("a trivial reversal of the forward").
#[test]
fn enu_to_ecef_iogp_9836_example() {
    let frame = LocalFrame::at(geo(55.0, 5.0, 200.0));

    let p = frame.from_enu(Enu {
        east: TOPO_ENU.0,
        north: TOPO_ENU.1,
        up: TOPO_ENU.2,
    });

    assert_ecef_near(p, TOPO_POINT, 1e-3);
}

// ---- Axes and frames -------------------------------------------------------------------------

/// REQ-C-3: at 0 N 0 E, east is ECEF +y, north is +z and up is +x.
#[test]
fn enu_east_at_0n_0e_is_ecef_y() {
    let frame = LocalFrame::at(geo(0.0, 0.0, 0.0));

    let east = frame.to_enu(ecef(A, 1.0, 0.0));
    let north = frame.to_enu(ecef(A, 0.0, 1.0));
    let up = frame.to_enu(ecef(A + 1.0, 0.0, 0.0));

    assert!((east.east - 1.0).abs() < 1e-9 && east.north.abs() < 1e-9 && east.up.abs() < 1e-9);
    assert!(north.east.abs() < 1e-9 && (north.north - 1.0).abs() < 1e-9 && north.up.abs() < 1e-9);
    assert!(up.east.abs() < 1e-9 && up.north.abs() < 1e-9 && (up.up - 1.0).abs() < 1e-9);
}

/// REQ-C-3: at the north pole, up is ECEF +z.
#[test]
fn enu_up_at_north_pole_is_ecef_z() {
    let frame = LocalFrame::at(geo(90.0, 0.0, 0.0));

    let enu = frame.to_enu(ecef(0.0, 0.0, B + 1.0));

    assert!(enu.east.abs() < 1e-9 && enu.north.abs() < 1e-9 && (enu.up - 1.0).abs() < 1e-9);
}

/// REQ-C-3: NED is ENU with the axes swapped and down negated; the swap is exact.
#[test]
fn ned_is_enu_with_axes_swapped() {
    let enu = Enu {
        east: 1.5,
        north: -2.25,
        up: 3.125,
    };

    let ned = Ned::from(enu);

    assert_eq!(
        ned,
        Ned {
            north: -2.25,
            east: 1.5,
            down: -3.125
        }
    );
    assert_eq!(Enu::from(ned), enu);
}

/// REQ-C-3: `to_ned` agrees with `to_enu` through the exact swap.
#[test]
fn to_ned_equals_swapped_to_enu() {
    let frame = LocalFrame::at(geo(52.0, 4.0, 10.0));
    let p = ecef(3_925_900.0, 274_600.0, 5_003_000.0);

    let ned = frame.to_ned(p);
    let enu = frame.to_enu(p);

    assert!((ned.north - enu.north).abs() < 1e-9);
    assert!((ned.east - enu.east).abs() < 1e-9);
    assert!((ned.down + enu.up).abs() < 1e-9);
}

/// REQ-C-3: the origin of a frame maps to (0, 0, 0) and back.
#[test]
fn local_frame_origin_maps_to_zero() {
    for (lat, lon, h) in [
        (0.0, 0.0, 0.0),
        (52.0, 4.0, 100.0),
        (-33.0, 151.0, -50.0),
        (89.0, -170.0, 5.0),
    ] {
        let frame = LocalFrame::at(geo(lat, lon, h));

        let enu = frame.to_enu(frame.origin());
        let back = frame.from_enu(Enu {
            east: 0.0,
            north: 0.0,
            up: 0.0,
        });

        assert!(
            enu.east.abs() < 1e-9 && enu.north.abs() < 1e-9 && enu.up.abs() < 1e-9,
            "{enu:?}"
        );
        assert!((back - frame.origin()).norm() < 1e-9);
    }
}

/// REQ-C-3: the frame origin equals the geodetic origin converted to ECEF.
#[test]
fn local_frame_origin_equals_geodetic_to_ecef() {
    let g = geo(52.0, 4.0, 100.0);

    let origin = LocalFrame::at(g).origin();

    assert!((origin - g.to_ecef()).norm() < 1e-9);
}

/// REQ-C-3: vector rotation is rotation only (no translation).
#[test]
fn vector_to_enu_is_rotation_only() {
    let frame = LocalFrame::at(geo(0.0, 0.0, 0.0));

    let enu = frame.vector_to_enu(Vec3 {
        x: 0.0,
        y: 1.0,
        z: 0.0,
    });
    let back = frame.vector_from_enu(Enu {
        east: 1.0,
        north: 0.0,
        up: 0.0,
    });
    let zero = frame.vector_to_enu(Vec3::ZERO);

    assert!((enu.east - 1.0).abs() < 1e-12 && enu.north.abs() < 1e-12 && enu.up.abs() < 1e-12);
    assert!(
        (back
            - Vec3 {
                x: 0.0,
                y: 1.0,
                z: 0.0
            })
        .norm()
            < 1e-12
    );
    assert_eq!(
        zero,
        Enu {
            east: 0.0,
            north: 0.0,
            up: 0.0
        }
    );
}

// ---- Edges -----------------------------------------------------------------------------------

/// REQ-C-3: the closed form is finite on the polar axis and at the Earth's centre.
#[test]
fn ecef_to_geodetic_on_polar_axis_and_centre_is_finite() {
    for z in [B + 100.0, B, 1.0, 0.0, -1.0, -B, -B - 100.0] {
        let g = ecef(0.0, 0.0, z).to_geodetic();
        assert!(
            g.lat().0.is_finite() && g.lon().0.is_finite() && g.height().0.is_finite(),
            "z = {z}: {g:?}"
        );
    }
    let pole = ecef(0.0, 0.0, B + 100.0).to_geodetic();
    assert!((pole.lat().0 - FRAC_PI_2).abs() < 1e-12 && (pole.height().0 - 100.0).abs() < 1e-6);
    let south = ecef(0.0, 0.0, -B - 100.0).to_geodetic();
    assert!((south.lat().0 + FRAC_PI_2).abs() < 1e-12 && (south.height().0 - 100.0).abs() < 1e-6);
}

/// REQ-C-3: longitude is wrapped into (-pi, pi]; +180 stays pi and -180 becomes pi.
#[test]
fn longitude_is_wrapped_to_minus_pi_pi() {
    assert!((geo(0.0, 190.0, 0.0).lon().0 - (-170.0_f64).to_radians()).abs() < 1e-12);
    assert!((geo(0.0, -190.0, 0.0).lon().0 - 170.0_f64.to_radians()).abs() < 1e-12);
    assert!((geo(0.0, 180.0, 0.0).lon().0 - PI).abs() < 1e-12);
    assert!((geo(0.0, -180.0, 0.0).lon().0 - PI).abs() < 1e-12);
    assert!((geo(0.0, 540.0, 0.0).lon().0 - PI).abs() < 1e-12);
}

/// REQ-C-3, REQ-C-4: construction rejects latitude outside [-pi/2, pi/2] and non-finite values.
#[test]
fn geodetic_new_rejects_out_of_range_and_non_finite() {
    let ok = Radians(0.0);

    assert!(matches!(
        Geodetic::new(Radians(FRAC_PI_2 + 0.01), ok, Meters(0.0)),
        Err(CoordError::LatitudeOutOfRange(_))
    ));
    assert!(matches!(
        Geodetic::new(Radians(-FRAC_PI_2 - 0.01), ok, Meters(0.0)),
        Err(CoordError::LatitudeOutOfRange(_))
    ));
    assert!(matches!(
        Geodetic::new(Radians(f64::NAN), ok, Meters(0.0)),
        Err(CoordError::NonFinite)
    ));
    assert!(matches!(
        Geodetic::new(ok, Radians(f64::INFINITY), Meters(0.0)),
        Err(CoordError::NonFinite)
    ));
    assert!(matches!(
        Geodetic::new(ok, ok, Meters(f64::NAN)),
        Err(CoordError::NonFinite)
    ));
    assert!(Geodetic::from_degrees(91.0, 0.0, 0.0).is_err());
    assert!(Geodetic::from_degrees(0.0, f64::NAN, 0.0).is_err());
}

/// REQ-C-3: the poles themselves are valid latitudes.
#[test]
fn geodetic_new_accepts_the_poles() {
    assert!(Geodetic::new(Radians(FRAC_PI_2), Radians(0.0), Meters(0.0)).is_ok());
    assert!(Geodetic::new(Radians(-FRAC_PI_2), Radians(0.0), Meters(0.0)).is_ok());
}

/// REQ-C-3: `with_height` changes only the height.
#[test]
fn with_height_changes_only_the_height() {
    let g = geo(52.0, 4.0, 0.0);

    let h = g.with_height(Meters(120.0));

    assert_eq!(
        (h.lat(), h.lon(), h.height()),
        (g.lat(), g.lon(), Meters(120.0))
    );
}

/// REQ-C-3: `Ecef` arithmetic and distance.
#[test]
fn ecef_sub_add_and_distance() {
    let a = ecef(1.0, 2.0, 3.0);
    let b = ecef(4.0, 6.0, 3.0);

    assert_eq!(
        b - a,
        Vec3 {
            x: 3.0,
            y: 4.0,
            z: 0.0
        }
    );
    assert_eq!(
        a + Vec3 {
            x: 3.0,
            y: 4.0,
            z: 0.0
        },
        b
    );
    assert_eq!(a.distance(b), Meters(5.0));
}

// ---- Position (ASM-044) ----------------------------------------------------------------------

/// ASM-044: a geodetic `Position` converts to exactly the ECEF of that geodetic point.
#[test]
fn position_geodetic_to_ecef_matches_geodetic_to_ecef() {
    for (lat, lon, h) in [(52.0, 4.0, 120.0), (-33.9, 151.2, 58.0), (90.0, 0.0, 0.0)] {
        let g = geo(lat, lon, h);

        assert_eq!(Position::Geodetic(g).to_ecef(), g.to_ecef());
        assert_eq!(Position::from(g), Position::Geodetic(g));
    }
}

/// ASM-044: an ECEF `Position` is the identity.
#[test]
fn position_ecef_is_identity() {
    let e = ecef(3_925_375.1, 274_488.9, 5_002_803.3);

    assert_eq!(Position::Ecef(e).to_ecef(), e);
    assert_eq!(Position::from(e), Position::Ecef(e));
}

// ---- Orientation (body FRD -> ECEF, Euler relative to local NED) -----------------------------

fn euler(heading_deg: f64, pitch_deg: f64, roll_deg: f64) -> Euler {
    Euler {
        heading: Radians::from_degrees(heading_deg),
        pitch: Radians::from_degrees(pitch_deg),
        roll: Radians::from_degrees(roll_deg),
    }
}

fn near(a: Vec3, want: (f64, f64, f64)) -> bool {
    (a.x - want.0).abs() < 1e-9 && (a.y - want.1).abs() < 1e-9 && (a.z - want.2).abs() < 1e-9
}

const FWD: Vec3 = Vec3 {
    x: 1.0,
    y: 0.0,
    z: 0.0,
};
const RIGHT: Vec3 = Vec3 {
    x: 0.0,
    y: 1.0,
    z: 0.0,
};
const DOWN: Vec3 = Vec3 {
    x: 0.0,
    y: 0.0,
    z: 1.0,
};

/// REQ-C-3 (ASM-033): heading 0 puts body forward along true north; at 0 N 0 E north is ECEF +z,
/// east is +y and down is -x.
#[test]
fn heading_0_body_forward_is_north() {
    let frame = LocalFrame::at(geo(0.0, 0.0, 0.0));

    let q = frame.orientation_from_euler(euler(0.0, 0.0, 0.0));

    assert!(near(q.rotate(FWD), (0.0, 0.0, 1.0)), "forward");
    assert!(near(q.rotate(RIGHT), (0.0, 1.0, 0.0)), "right");
    assert!(near(q.rotate(DOWN), (-1.0, 0.0, 0.0)), "down");
}

/// REQ-C-3 (ASM-033): heading is clockwise from north, so heading 90 degrees faces east.
#[test]
fn heading_90_body_forward_is_east() {
    let frame = LocalFrame::at(geo(0.0, 0.0, 0.0));

    let q = frame.orientation_from_euler(euler(90.0, 0.0, 0.0));

    assert!(near(q.rotate(FWD), (0.0, 1.0, 0.0)), "forward");
    // Right wing now points south: ECEF -z at 0 N 0 E.
    assert!(near(q.rotate(RIGHT), (0.0, 0.0, -1.0)), "right");
}

/// REQ-C-3 (ASM-033): positive pitch raises the nose (forward gains an up component).
#[test]
fn pitch_positive_is_nose_up() {
    let frame = LocalFrame::at(geo(0.0, 0.0, 0.0));
    let (s, c) = math::sin_cos(30.0_f64.to_radians());

    let q = frame.orientation_from_euler(euler(0.0, 30.0, 0.0));

    // up = +x, north = +z at 0 N 0 E.
    assert!(near(q.rotate(FWD), (s, 0.0, c)));
}

/// REQ-C-3 (ASM-033): positive roll lowers the right wing (aerospace convention).
#[test]
fn roll_positive_is_right_wing_down() {
    let frame = LocalFrame::at(geo(0.0, 0.0, 0.0));

    let q = frame.orientation_from_euler(euler(0.0, 0.0, 90.0));

    // Right wing -> down = -x; belly (body down) -> west = -y.
    assert!(near(q.rotate(RIGHT), (-1.0, 0.0, 0.0)), "right");
    assert!(near(q.rotate(DOWN), (0.0, -1.0, 0.0)), "down");
}

/// REQ-C-3: the orientation depends on where the frame is: at the north pole (lon 0) north is
/// ECEF -x, so heading 0 points forward along -x.
#[test]
fn heading_0_at_north_pole_points_along_minus_x() {
    let frame = LocalFrame::at(geo(90.0, 0.0, 0.0));

    let q = frame.orientation_from_euler(euler(0.0, 0.0, 0.0));

    assert!(near(q.rotate(FWD), (-1.0, 0.0, 0.0)));
}

/// REQ-C-3: `euler_from_orientation` inverts `orientation_from_euler` on a worked case.
#[test]
fn euler_from_orientation_recovers_a_worked_case() {
    let frame = LocalFrame::at(geo(52.0, 4.0, 0.0));
    let e = euler(45.0, 10.0, -5.0);

    let got = frame.euler_from_orientation(frame.orientation_from_euler(e));

    assert!((got.heading.0 - e.heading.0).abs() < 1e-9);
    assert!((got.pitch.0 - e.pitch.0).abs() < 1e-9);
    assert!((got.roll.0 - e.roll.0).abs() < 1e-9);
}

/// REQ-C-3: the orientation is a unit quaternion.
#[test]
fn orientation_from_euler_is_a_unit_quaternion() {
    let frame = LocalFrame::at(geo(-33.0, 151.0, 0.0));

    let q: Quaternion = frame.orientation_from_euler(euler(123.0, -20.0, 30.0));

    let norm = (q.w * q.w + q.x * q.x + q.y * q.y + q.z * q.z).sqrt();
    assert!((norm - 1.0).abs() < 1e-12);
}
