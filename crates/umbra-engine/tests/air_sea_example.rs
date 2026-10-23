//! AT-13 (M7, engine row): the `air_sea` example scenario. Requirements: REQ-M-2, REQ-M-1, REQ-C-6,
//! REQ-K-4, ADR 0051, ASM-607..611.
//!
//! Contract with the example (WI-6 writes `examples/scenarios/air_sea/` to it): three entities,
//! `aircraft-1` (template on `builtin.air.kinematic`, a route whose waypoints climb and whose last
//! waypoint is 1500 m above the ellipsoid), `ship-1` (`builtin.sea.kinematic`, `surface_height_m`
//! 0, a route) and `truck-1` (`builtin.ground.kinematic`, a route), 30 Hz, as fast as possible.
//! Written in M7 WI-1, enabled in M7 WI-6 with the example.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod common;
mod m7_support;

use core::ops::ControlFlow;
use core::time::Duration;

use common::options;
use m7_support::example;
use umbra_core::{Geodetic, LocalFrame};
use umbra_engine::builtins::standard_registry;
use umbra_engine::scenario::{ScenarioRun, instantiate, load_scenario};
use umbra_engine::{EntityInfo, RunUntil, validate_path_with_plugins};

const LAST_WAYPOINT_ALT_M: f64 = 1500.0;

fn run() -> ScenarioRun {
    let registry = standard_registry().expect("the standard registry builds");
    let scenario = load_scenario(&registry, &example("air_sea")).expect("load air_sea");
    instantiate(
        &registry,
        &scenario,
        options(Duration::from_micros(500), None, None),
    )
    .expect("instantiate")
}

/// The last waypoint and the arrival radius of `aircraft-1`'s route, read from the loaded scenario.
fn aircraft_route_end() -> (Geodetic, f64) {
    let registry = standard_registry().expect("the standard registry builds");
    let scenario = load_scenario(&registry, &example("air_sea")).expect("load air_sea");
    let route = scenario
        .entities
        .iter()
        .find(|p| p.name.as_deref() == Some("aircraft-1"))
        .and_then(|p| p.route.as_ref())
        .expect("aircraft-1 has a route");
    let last = route.waypoints.last().expect("a waypoint");
    (last.to_ecef().to_geodetic(), route.arrival_radius.0)
}

fn id_of(entities: &[EntityInfo], name: &str) -> umbra_core::EntityId {
    entities
        .iter()
        .find(|e| e.name.as_deref() == Some(name))
        .unwrap_or_else(|| panic!("no entity named {name}"))
        .id
}

/// REQ-T-6: the example validates with no error diagnostics.
#[test]
fn air_sea_validates_without_errors() {
    let report = validate_path_with_plugins(
        &standard_registry().expect("registry"),
        &example("air_sea"),
        &[],
    )
    .expect("validate");

    assert!(
        report
            .diagnostics
            .iter()
            .all(|d| d.severity != umbra_engine::Severity::Error),
        "{report:?}"
    );
}

/// REQ-M-2: each entity uses its own model, and the scenario runs to its end.
#[test]
fn air_sea_runs_with_one_model_per_domain() {
    let mut run = run();
    let ids = (
        id_of(&run.entities, "aircraft-1"),
        id_of(&run.entities, "ship-1"),
        id_of(&run.entities, "truck-1"),
    );
    let ticks = run.duration_ticks;

    run.engine
        .run(RunUntil::Ticks(ticks), |_| ControlFlow::Continue(()))
        .expect("run");

    let snapshot = run.engine.snapshot();
    let model = |id| {
        snapshot
            .entities
            .iter()
            .find(|e| e.id == id)
            .expect("entity")
            .motion_model
            .to_string()
    };
    assert_eq!(model(ids.0), "builtin.air.kinematic");
    assert_eq!(model(ids.1), "builtin.sea.kinematic");
    assert_eq!(model(ids.2), "builtin.ground.kinematic");
}

/// REQ-M-2: the aircraft reaches its last waypoint (within the route's arrival radius, at
/// 1500 m within 1 m, at the first tick it is there), every entity ends on its last route index,
/// the ship stays on the surface (height 0 within a millimetre) and ends stopped like the truck.
#[test]
fn air_sea_waypoints_are_reached_with_altitude_within_one_meter() {
    let mut run = run();
    let ids = (
        id_of(&run.entities, "aircraft-1"),
        id_of(&run.entities, "ship-1"),
        id_of(&run.entities, "truck-1"),
    );
    let ticks = run.duration_ticks;
    let (last, arrival_radius) = aircraft_route_end();
    let frame = LocalFrame::at(last);
    let mut reached: Option<(f64, f64)> = None;
    run.engine
        .run(RunUntil::Ticks(ticks), |v| {
            let snapshot = v.snapshot();
            let aircraft = snapshot.entities.iter().find(|e| e.id == ids.0);
            if let (None, Some(e)) = (reached, aircraft) {
                let off = frame.to_enu(e.state.position);
                let miss = umbra_core::math::hypot(off.east, off.north);
                if miss <= arrival_radius {
                    let height = Geodetic::height(e.state.position.to_geodetic()).0;
                    reached = Some((miss, height));
                }
            }
            ControlFlow::Continue(())
        })
        .expect("run");

    let (_, height) = reached.expect("the aircraft came within the arrival radius of waypoint 3");
    assert!(
        (height - LAST_WAYPOINT_ALT_M).abs() < 1.0,
        "height {height} when it reached the last waypoint"
    );
    let snapshot = run.engine.snapshot();
    let entity = |id| {
        snapshot
            .entities
            .iter()
            .find(|e| e.id == id)
            .expect("entity")
    };
    for id in [ids.0, ids.1, ids.2] {
        // The follower never advances past the last index (`route::steer`): `next + 1 == len` only
        // says the last waypoint is the target, which the aircraft check above proves it reached.
        let (next, len) = entity(id).route_next.expect("a route");
        assert_eq!(next + 1, len, "entity {id} is on its last waypoint");
    }
    // `Hold` is issued on arrival: the ship and the truck have stopped. The aircraft cannot stop
    // (minimum speed 40 m/s): after `Hold` it slows toward its minimum speed, leaves the arrival
    // radius and the follower steers it back, so it keeps flying around the last waypoint (ASM-704).
    let speed = |id| entity(id).state.velocity.norm();
    assert!(speed(ids.1) < 1e-6 && speed(ids.2) < 1e-6);
    // the WI-1 assertion, kept: the aircraft ends at the last waypoint's height (it loiters at it)
    let height = |p: umbra_core::Ecef| -> f64 { Geodetic::height(p.to_geodetic()).0 };
    assert!((height(entity(ids.0).state.position) - LAST_WAYPOINT_ALT_M).abs() < 1.0);
    assert!(height(entity(ids.1).state.position).abs() < 1e-3);
}

/// REQ-K-4: two runs of the example hash alike on every tick.
#[test]
fn air_sea_is_deterministic_tick_by_tick() {
    let hashes = || {
        let mut run = run();
        let mut hashes = Vec::new();
        let ticks = run.duration_ticks;
        run.engine
            .run(RunUntil::Ticks(ticks), |v| {
                hashes.push(v.state_hash());
                ControlFlow::Continue(())
            })
            .expect("run");
        hashes
    };

    assert_eq!(hashes(), hashes());
}
