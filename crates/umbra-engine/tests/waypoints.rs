//! AT-8 (ported M1 AT-9): entities follow their waypoint lists in the example scenario.
//! Requirements: M2 acceptance 2, REQ-M-2, REQ-B-2 (subset), ASM-029, ASM-061.
//!
//! The example (three entities near 52 N 4 E, square routes of four 500 m legs, arrival radius 5 m,
//! fixed cruise speeds, flat terrain at 0 m, 30 Hz) is run for its full duration of 240 s (7200
//! ticks). Kinematic limits are read from each entity's resolved parameters in the scenario, so a
//! template, override or inheritance change shows up here.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod common;

use core::f64::consts::{PI, TAU};

use common::{Trace, example_engine, load_example, run_traced};
use umbra_core::{Ecef, Euler, KinematicState, LocalFrame};
use umbra_engine::RunUntil;
use umbra_templates::Placement;

const HZ: f64 = 30.0;
const ARRIVAL_M: f64 = 5.0;
/// All waypoints must be reached by this simulation time. The slowest entity (the truck, 1.5 m/s^2
/// braking) finishes in about 150 s; 200 s leaves margin.
const ALL_REACHED_BY_S: f64 = 200.0;

/// The full-duration trace of the example run. Its `snapshots[0]` is the spawn state (taken before
/// the first tick, as in M1), so `snapshots[i]` is the state after tick `i`; `hashes` has no entry
/// for the spawn state and is not used here.
fn full_trace() -> Trace {
    let scenario = load_example();
    let mut engine = example_engine(None, None);
    let mut trace = Trace::default();
    trace.snapshots.push(engine.snapshot());
    run_traced(
        &mut engine,
        RunUntil::Ticks(scenario.duration_ticks),
        &mut trace,
    );
    trace
}

/// The spawn state followed by the post-tick states of entity `index`, and its `route_next` values.
fn states_of(index: usize) -> (Vec<KinematicState>, Vec<Option<(usize, usize)>>) {
    let trace = full_trace();
    let snaps = trace.entity(index);
    (
        snaps.iter().map(|s| s.state).collect(),
        snaps.iter().map(|s| s.route_next).collect(),
    )
}

fn placements() -> Vec<Placement> {
    load_example().entities
}

fn waypoints_of(index: usize) -> Vec<Ecef> {
    placements()[index]
        .route
        .as_ref()
        .expect("every example entity has a route")
        .waypoints
        .iter()
        .map(|p| p.to_ecef())
        .collect()
}

fn speed(s: &KinematicState) -> f64 {
    s.velocity.norm()
}

fn heading(s: &KinematicState) -> f64 {
    LocalFrame::at(s.position.to_geodetic())
        .euler_from_orientation(s.orientation)
        .heading
        .0
}

fn angle_delta(a: f64, b: f64) -> f64 {
    let mut d = (a - b) % TAU;
    if d > PI {
        d -= TAU;
    }
    if d <= -PI {
        d += TAU;
    }
    d
}

/// For each waypoint in order, the first tick (index into `states`) at which the entity is within
/// the arrival radius, searching forward from the previous waypoint's tick.
fn visit_ticks(states: &[KinematicState], waypoints: &[Ecef]) -> Vec<usize> {
    let mut visits = Vec::new();
    for (tick, s) in states.iter().enumerate() {
        let Some(next) = waypoints.get(visits.len()) else {
            break;
        };
        if s.position.distance(*next).0 <= ARRIVAL_M + 1e-6 {
            visits.push(tick);
        }
    }
    visits
}

fn param(p: &Placement, key: &str) -> f64 {
    p.motion
        .params
        .get(key)
        .and_then(serde_json::Value::as_f64)
        .unwrap_or_else(|| panic!("resolved motion params lack `{key}`: {}", p.motion.params))
}

/// ASM-061: the resolved parameters of the three templates (inheritance and the per-entity
/// override), which the limit tests below rely on.
#[test]
fn given_example_then_resolved_motion_params_follow_inheritance_and_the_override() {
    let entities = placements();

    let limits: Vec<(f64, f64, f64)> = entities
        .iter()
        .map(|p| {
            (
                param(p, "max_speed_mps"),
                param(p, "max_turn_rate_dps"),
                param(p, "max_accel_mps2"),
            )
        })
        .collect();

    assert_eq!(
        limits,
        [
            (18.0, 40.0, 2.5), // ground_vehicle.base as is
            (16.0, 30.0, 2.5), // tank.mbt_generic (turn rate) + entity override (speed)
            (22.0, 40.0, 1.5), // truck.cargo (speed, accel)
        ]
    );
}

/// ASM-061: each leg of every example route is 500 m, and the squares start near 52 N 4 E.
#[test]
fn given_example_routes_then_legs_are_500_m_near_the_origin() {
    for index in 0..3 {
        let route = waypoints_of(index);
        assert_eq!(route.len(), 4, "entity {}", index + 1);
        let start = placements()[index].position.to_ecef();
        let mut previous = start;
        for (leg, wp) in route.iter().enumerate() {
            let length = previous.distance(*wp).0;
            assert!(
                (length - 500.0).abs() < 1.0,
                "entity {} leg {leg}: {length} m",
                index + 1
            );
            previous = *wp;
        }
        assert!(start.to_geodetic().lat().to_degrees() > 51.9);
    }
}

/// M2 acceptance 2: entity 1 reaches every waypoint, in order, within the time limit.
#[test]
fn given_example_entity_1_then_it_visits_every_waypoint_in_order() {
    let (states, _) = states_of(0);
    let route = waypoints_of(0);

    let visits = visit_ticks(&states, &route);

    assert_eq!(visits.len(), route.len(), "waypoints reached: {visits:?}");
    assert!(
        visits.windows(2).all(|w| w[0] < w[1]),
        "strictly increasing: {visits:?}"
    );
    let last_s = f64::from(u32::try_from(visits[visits.len() - 1]).expect("tick fits")) / HZ;
    assert!(
        last_s < ALL_REACHED_BY_S,
        "last waypoint reached at {last_s} s"
    );
}

/// The same holds for all three entities, within the scenario's own duration.
#[test]
fn every_example_entity_completes_its_route_within_duration() {
    let trace = full_trace();
    let duration = load_example().duration_ticks;

    for index in 0..3 {
        let states: Vec<KinematicState> = trace.entity(index).iter().map(|s| s.state).collect();
        let route = waypoints_of(index);
        let visits = visit_ticks(&states, &route);
        assert_eq!(
            visits.len(),
            route.len(),
            "entity {}: {visits:?}",
            index + 1
        );
        let last = u64::try_from(visits[visits.len() - 1]).expect("tick fits");
        assert!(
            last < duration,
            "entity {} finished at tick {last} of {duration}",
            index + 1
        );
        assert!(
            visits.windows(2).all(|w| w[0] < w[1]),
            "entity {}: {visits:?}",
            index + 1
        );
    }
}

/// REQ-B-2 (`Hold`): after the route is complete the entity stops within 10 s and stays within the
/// arrival radius of the last waypoint until the end of the run.
#[test]
fn given_example_entity_1_when_route_complete_then_it_holds_at_last_waypoint() {
    let (states, _) = states_of(0);
    let route = waypoints_of(0);
    let visits = visit_ticks(&states, &route);
    let last = route[route.len() - 1];
    let reached = visits[visits.len() - 1];
    let ten_seconds = 300;

    let stopped_by = reached + ten_seconds;

    assert!(
        speed(&states[stopped_by]) < 1e-6,
        "speed {} 10 s after arrival",
        speed(&states[stopped_by])
    );
    for (tick, s) in states.iter().enumerate().skip(reached) {
        assert!(
            s.position.distance(last).0 <= ARRIVAL_M + 1e-6,
            "tick {tick}: left the last waypoint"
        );
    }
}

/// ASM-029: at the tick the entity first comes within the arrival radius of waypoint `k`, the route
/// still points at `k`; one tick later it points at `k + 1` (and stays on the last waypoint).
#[test]
fn given_example_entity_1_visits_waypoint_then_route_next_advances_by_one() {
    let (states, nexts) = states_of(0);
    let route = waypoints_of(0);
    let visits = visit_ticks(&states, &route);

    for (k, tick) in visits.iter().enumerate() {
        assert_eq!(nexts[*tick], Some((k, 4)), "at the visit of waypoint {k}");
        let after = if k + 1 < route.len() { k + 1 } else { k };
        assert_eq!(nexts[tick + 1], Some((after, 4)), "after waypoint {k}");
    }
    assert_eq!(nexts[nexts.len() - 1], Some((3, 4)));
}

/// REQ-M-2: the entity stays near the straight leg it is flying: cross-track error below 50 m.
#[test]
fn given_example_entity_1_then_cross_track_error_stays_below_50_m() {
    let (states, _) = states_of(0);
    let route = waypoints_of(0);
    let visits = visit_ticks(&states, &route);
    let mut leg_start = states[0].position;

    for (k, end) in route.iter().enumerate() {
        let (from, to) = (if k == 0 { 0 } else { visits[k - 1] }, visits[k]);
        let axis = *end - leg_start;
        for s in &states[from..=to] {
            let rel = s.position - leg_start;
            let t = (rel.dot(axis) / axis.dot(axis)).clamp(0.0, 1.0);
            let cross = (rel - axis * t).norm();
            assert!(cross < 50.0, "leg {k}: cross-track {cross} m");
        }
        leg_start = *end;
    }
}

/// REQ-M-2: on every tick, for every entity, speed, acceleration (except the arrival snap), turn
/// rate and height stay within the limits of the entity's resolved parameters.
#[test]
fn given_example_then_kinematic_limits_hold_every_tick_for_every_entity() {
    let trace = full_trace();
    let dt = 1.0 / HZ;

    for (index, p) in placements().iter().enumerate() {
        let (max_speed, max_accel) = (param(p, "max_speed_mps"), param(p, "max_accel_mps2"));
        let max_turn = param(p, "max_turn_rate_dps") * PI / 180.0;
        let states: Vec<KinematicState> = trace.entity(index).iter().map(|s| s.state).collect();
        for (tick, pair) in states.windows(2).enumerate() {
            let (before, after) = (&pair[0], &pair[1]);
            let (v0, v1) = (speed(before), speed(after));
            let who = format!("entity {} tick {tick}", index + 1);
            assert!(
                v1 <= max_speed + 1e-9,
                "{who}: speed {v1} above {max_speed}"
            );
            // The arrival snap (new speed exactly zero) may brake harder, up to twice the limit.
            let accel_limit = if v1 == 0.0 {
                2.0 * max_accel
            } else {
                max_accel
            };
            assert!(
                ((v1 - v0) / dt).abs() <= accel_limit + 1e-6,
                "{who}: {v0} -> {v1}"
            );
            let turn = angle_delta(heading(after), heading(before)).abs() / dt;
            assert!(
                turn <= max_turn + 1e-6,
                "{who}: turn rate {turn} above {max_turn}"
            );
            let h = after.position.to_geodetic().height().0;
            assert!(h.abs() < 1e-6, "{who}: height {h}");
        }
    }
}

/// REQ-M-2 (flat terrain): entities stay level, with pitch and roll zero.
#[test]
fn given_example_then_entities_stay_level() {
    let trace = full_trace();

    for index in 0..3 {
        for (tick, snap) in trace.entity(index).iter().enumerate().step_by(30) {
            let s = &snap.state;
            let e: Euler =
                LocalFrame::at(s.position.to_geodetic()).euler_from_orientation(s.orientation);
            assert!(
                e.pitch.0.abs() < 1e-9 && e.roll.0.abs() < 1e-9,
                "entity {} tick {tick}: {e:?}",
                index + 1
            );
        }
    }
}

/// The three example entities move, and their routes differ.
#[test]
fn given_example_when_run_then_all_three_entities_move_along_different_routes() {
    let mut engine = example_engine(None, None);
    let before = engine.snapshot();

    common::run_silent(&mut engine, RunUntil::Ticks(300));

    let after = engine.snapshot();
    for (b, a) in before.entities.iter().zip(&after.entities) {
        assert!(
            a.state.position.distance(b.state.position).0 > 10.0,
            "entity {} did not move",
            a.id
        );
    }
    assert_ne!(waypoints_of(0), waypoints_of(1));
    assert_ne!(waypoints_of(1), waypoints_of(2));
}

/// ASM-061: the tank's per-entity override (16 m/s) is binding: it never exceeds it, though its
/// template allows 18 m/s.
#[test]
fn given_example_tank_override_then_speed_never_exceeds_16_mps() {
    let trace = full_trace();

    let fastest = trace
        .entity(1)
        .iter()
        .map(|s| speed(&s.state))
        .fold(0.0_f64, f64::max);

    assert!(fastest <= 16.0 + 1e-9, "tank reached {fastest} m/s");
    assert!(
        fastest > 14.0,
        "the tank cruises at 14.5 m/s, got {fastest}"
    );
}
