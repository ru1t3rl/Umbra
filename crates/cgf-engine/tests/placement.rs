//! AT-15: placement by ECEF or lat/lon/alt. Requirements: D-15, REQ-C-3, ASM-044.
//!
//! Positions are ECEF internally; geodetic input is converted once at spawn and geodetic output is
//! a display conversion of the snapshot.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

use std::ops::ControlFlow;
use std::sync::Arc;

use cgf_core::{
    Enu, Geodetic, LocalFrame, Meters, MetersPerSecond, MotionModel, Ownership, Position, Radians,
    TickRate,
};
use cgf_engine::{Engine, EngineConfig, RouteSpec, RunUntil, SpawnSpec, demo};
use cgf_kernel::{ClockMode, ManualWallClock};
use cgf_motion::GroundKinematic;

fn engine() -> Engine {
    let config = EngineConfig {
        seed: 1,
        tick_rate: TickRate::new(30).expect("non-zero rate"),
        clock: ClockMode::AsFastAsPossible,
    };
    Engine::with_wall_clock(
        config,
        Arc::new(cgf_core::FlatTerrain {
            height: Meters(0.0),
        }),
        Box::new(ManualWallClock::new()),
    )
}

fn model() -> Arc<dyn MotionModel> {
    Arc::new(GroundKinematic::new(demo::ground_params()).expect("valid params"))
}

fn geo(lat: f64, lon: f64, h: f64) -> Geodetic {
    Geodetic::from_degrees(lat, lon, h).expect("valid geodetic")
}

fn spec(position: Position, heading_deg: f64, speed: f64) -> SpawnSpec {
    SpawnSpec {
        position,
        heading: Radians::from_degrees(heading_deg),
        speed: MetersPerSecond(speed),
        ownership: Ownership::Owned,
        motion: model(),
        route: None,
    }
}

/// ASM-044: an entity placed by lat/lon/alt has the ECEF state of that point.
#[test]
fn given_entity_placed_by_lat_lon_alt_when_snapshotted_then_state_is_ecef_of_that_point() {
    let mut e = engine();
    let g = geo(52.3, 4.9, 120.0);

    assert!(e.spawn(spec(Position::Geodetic(g), 0.0, 0.0)).is_ok());

    let p = e.snapshot().entities[0].state.position;
    assert!((p.0 - g.to_ecef().0).norm() < 1e-6);
}

/// ASM-044, REQ-K-4: the same point given as ECEF or as geodetic yields identical state hashes.
#[test]
fn given_same_point_by_ecef_and_by_geodetic_then_state_hashes_equal() {
    let g = geo(52.3, 4.9, 120.0);
    let (mut a, mut b) = (engine(), engine());

    assert!(a.spawn(spec(Position::Geodetic(g), 30.0, 5.0)).is_ok());
    assert!(
        b.spawn(spec(Position::Ecef(g.to_ecef()), 30.0, 5.0))
            .is_ok()
    );

    assert_eq!(a.state_hash(), b.state_hash());
}

/// ASM-044: route waypoints may be given by lat/lon/alt, and the entity reaches each in order.
#[test]
fn given_geodetic_route_then_entity_reaches_each_waypoint() {
    let mut e = engine();
    let start = geo(52.0, 4.0, 0.0);
    let frame = LocalFrame::at(start);
    let at = |east: f64, north: f64| {
        frame
            .from_enu(Enu {
                east,
                north,
                up: 0.0,
            })
            .to_geodetic()
            .with_height(Meters(0.0))
            .expect("finite height")
    };
    let waypoints = [at(200.0, 0.0), at(200.0, 200.0), at(0.0, 200.0)];
    let mut s = spec(Position::Geodetic(start), 90.0, 0.0);
    s.route = Some(RouteSpec {
        waypoints: waypoints.iter().map(|g| Position::Geodetic(*g)).collect(),
        arrival_radius: Meters(5.0),
        cruise_speed: MetersPerSecond(10.0),
    });
    assert!(e.spawn(s).is_ok());
    let targets: Vec<_> = waypoints.iter().map(|g| g.to_ecef()).collect();
    let mut reached = 0;

    e.run(RunUntil::Ticks(30 * 120), |v| {
        let p = v.snapshot().entities[0].state.position;
        if reached < targets.len() && p.distance(targets[reached]).0 <= 5.0 + 1e-6 {
            reached += 1;
        }
        ControlFlow::Continue(())
    })
    .expect("run");

    assert_eq!(reached, targets.len());
}

/// ASM-044: heading 90 degrees at a lat/lon means clockwise from true north: east in local ENU.
#[test]
fn given_heading_90_deg_at_lat_lon_then_velocity_points_east_in_enu() {
    let mut e = engine();
    let g = geo(52.3, 4.9, 0.0);

    assert!(e.spawn(spec(Position::Geodetic(g), 90.0, 10.0)).is_ok());

    let v = e.snapshot().entities[0].state.velocity;
    let enu = LocalFrame::at(g).vector_to_enu(v);
    assert!((enu.east - 10.0).abs() < 1e-9, "{enu:?}");
    let heading = LocalFrame::at(g)
        .euler_from_orientation(e.snapshot().entities[0].state.orientation)
        .heading;
    assert!(
        (heading.0 - core::f64::consts::FRAC_PI_2).abs() < 1e-9,
        "{heading:?}"
    );
    assert!(enu.north.abs() < 1e-9 && enu.up.abs() < 1e-9, "{enu:?}");
}

/// ASM-044: the snapshot's display conversion returns the spawn lat/lon/alt (1e-9 deg, 1e-6 m).
#[test]
fn snapshot_geodetic_round_trips_spawn_lat_lon_alt() {
    let mut e = engine();
    let g = geo(-33.8688, 151.2093, 58.0);

    assert!(e.spawn(spec(Position::Geodetic(g), 0.0, 0.0)).is_ok());

    let back = e.snapshot().entities[0].geodetic();
    assert!((back.lat().to_degrees() - g.lat().to_degrees()).abs() < 1e-9);
    assert!((back.lon().to_degrees() - g.lon().to_degrees()).abs() < 1e-9);
    assert!((back.height().0 - 58.0).abs() < 1e-6);
}
