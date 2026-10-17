//! AT-9: an entity follows a waypoint list. Requirements: M1 acceptance 3, REQ-M-2, REQ-B-2
//! (subset), ASM-028, ASM-029.
//!
//! Entity 1 of the demo scenario (square route of four 500 m legs, arrival radius 5 m, ground
//! parameters 18 m/s, 40 deg/s, 2.5 m/s^2 at 30 Hz, flat terrain at 0 m) is run for 240 s.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

use core::f64::consts::{PI, TAU};

use cgf_core::{Ecef, Euler, KinematicState, LocalFrame, Meters, TickRate};
use cgf_engine::{Engine, EngineConfig, RunUntil, demo};
use cgf_kernel::{ClockMode, ManualWallClock};

const HZ: f64 = 30.0;
const TICKS: u64 = 7200;
const ARRIVAL_M: f64 = 5.0;
const MAX_SPEED: f64 = 18.0;
const MAX_ACCEL: f64 = 2.5;
const MAX_TURN: f64 = 40.0 * PI / 180.0;
/// All waypoints must be reached by this simulation time.
///
/// The plan (AT-9) said 120 s. That is infeasible with the specified algorithm: 2000 m at a cruise
/// speed of 14.4 to 18 m/s takes 111 to 139 s before braking for each waypoint (the entity slows
/// to about `sqrt(2 * 2.5 * 5) = 5 m/s` at the arrival radius, then re-accelerates, which costs
/// another 3 to 4 s per waypoint). 200 s leaves margin for every cruise speed in [14.4, 18].
const ALL_REACHED_BY_S: f64 = 200.0;

/// The post-tick state of entity 1 for each tick, preceded by its spawn state.
fn entity_1_states(seed: u64) -> Vec<KinematicState> {
    let config = EngineConfig {
        seed,
        tick_rate: TickRate::new(30).expect("non-zero rate"),
        clock: ClockMode::AsFastAsPossible,
        terrain_height: Meters(0.0),
    };
    let mut engine = Engine::with_wall_clock(config, Box::new(ManualWallClock::new()));
    assert!(demo::spawn_walking_skeleton(&mut engine).is_ok());
    let mut states = vec![engine.snapshot().entities[0].state];
    engine
        .run(RunUntil::Ticks(TICKS), |v| {
            states.push(v.snapshot().entities[0].state);
        })
        .expect("run");
    states
}

fn waypoints() -> Vec<Ecef> {
    demo::route_of(0)
        .into_iter()
        .map(cgf_core::Geodetic::to_ecef)
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

/// ASM-028: the demo spawns three entities with ids 1, 2 and 3, in order.
#[test]
fn given_demo_when_spawned_then_ids_are_1_2_3_in_order() {
    let config = EngineConfig {
        seed: 1,
        tick_rate: TickRate::new(30).expect("non-zero rate"),
        clock: ClockMode::AsFastAsPossible,
        terrain_height: Meters(0.0),
    };
    let mut engine = Engine::with_wall_clock(config, Box::new(ManualWallClock::new()));

    let ids = demo::spawn_walking_skeleton(&mut engine);

    assert_eq!(
        ids.map(|v| v.iter().map(|id| id.get()).collect::<Vec<_>>())
            .ok(),
        Some(vec![1, 2, 3])
    );
    assert_eq!(engine.snapshot().entities.len(), 3);
}

/// ASM-028: entity 1's route is a square of 500 m legs near the demo origin.
#[test]
fn given_demo_entity_1_route_then_legs_are_500_m_near_the_origin() {
    let route = waypoints();

    assert!(
        route.len() >= 4,
        "a square needs at least four waypoints, got {}",
        route.len()
    );
    for pair in route.windows(2) {
        let leg = pair[0].distance(pair[1]).0;
        assert!((leg - 500.0).abs() < 1.0, "leg of {leg} m");
    }
    let (lat, lon) = demo::ORIGIN_DEG;
    let near = cgf_core::Geodetic::from_degrees(lat, lon, 0.0)
        .expect("origin")
        .to_ecef();
    assert!(route[0].distance(near).0 < 2000.0);
}

/// M1 acceptance 3: the entity reaches every waypoint, in order, within the time limit.
#[test]
fn given_demo_entity_1_when_run_then_it_visits_every_waypoint_in_order() {
    let states = entity_1_states(42);
    let route = waypoints();

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

/// M1 acceptance 3 (any cruise speed): the same holds for other seeds, i.e. other cruise speeds.
#[test]
fn given_other_seeds_when_run_then_entity_1_still_visits_every_waypoint() {
    let route = waypoints();

    for seed in [1, 2, 3, 7] {
        let visits = visit_ticks(&entity_1_states(seed), &route);
        assert_eq!(visits.len(), route.len(), "seed {seed}: {visits:?}");
    }
}

/// REQ-B-2 (`Hold`): after the route is complete the entity stops within 10 s and stays within
/// the arrival radius of the last waypoint until the end of the run.
#[test]
fn given_route_complete_then_entity_holds_at_last_waypoint() {
    let states = entity_1_states(42);
    let route = waypoints();
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

/// REQ-M-2: on every tick speed, acceleration (except the arrival snap), turn rate and height
/// stay within the model's limits.
#[test]
fn given_route_then_kinematic_limits_hold_every_tick() {
    let states = entity_1_states(42);
    let dt = 1.0 / HZ;
    let frame_height = |s: &KinematicState| s.position.to_geodetic().height().0;

    for (tick, pair) in states.windows(2).enumerate() {
        let (before, after) = (&pair[0], &pair[1]);
        let (v0, v1) = (speed(before), speed(after));
        assert!(v1 <= MAX_SPEED + 1e-9, "tick {tick}: speed {v1}");
        // The arrival snap (new speed exactly zero) may brake harder, up to twice the limit.
        let accel_limit = if v1 == 0.0 {
            2.0 * MAX_ACCEL
        } else {
            MAX_ACCEL
        };
        assert!(
            ((v1 - v0) / dt).abs() <= accel_limit + 1e-6,
            "tick {tick}: {v0} -> {v1}"
        );
        let turn = angle_delta(heading(after), heading(before)).abs() / dt;
        assert!(turn <= MAX_TURN + 1e-6, "tick {tick}: turn rate {turn}");
        assert!(
            frame_height(after).abs() < 1e-6,
            "tick {tick}: height {}",
            frame_height(after)
        );
    }
}

/// REQ-M-2 (flat terrain): the entity stays level, with pitch and roll zero.
#[test]
fn given_route_then_entity_stays_level() {
    let states = entity_1_states(42);

    for (tick, s) in states.iter().enumerate().step_by(30) {
        let e: Euler =
            LocalFrame::at(s.position.to_geodetic()).euler_from_orientation(s.orientation);
        assert!(
            e.pitch.0.abs() < 1e-9 && e.roll.0.abs() < 1e-9,
            "tick {tick}: {e:?}"
        );
    }
}

/// ASM-028: the other two demo entities move too (their routes differ from entity 1's).
#[test]
fn given_demo_when_run_then_all_three_entities_move() {
    let config = EngineConfig {
        seed: 5,
        tick_rate: TickRate::new(30).expect("non-zero rate"),
        clock: ClockMode::AsFastAsPossible,
        terrain_height: Meters(0.0),
    };
    let mut engine = Engine::with_wall_clock(config, Box::new(ManualWallClock::new()));
    assert!(demo::spawn_walking_skeleton(&mut engine).is_ok());
    let before = engine.snapshot();

    engine.run(RunUntil::Ticks(300), |_| {}).expect("run");

    let after = engine.snapshot();
    for (b, a) in before.entities.iter().zip(&after.entities) {
        assert!(
            a.state.position.distance(b.state.position).0 > 10.0,
            "entity {} did not move",
            a.id
        );
    }
    assert_ne!(demo::route_of(0), demo::route_of(1));
    assert_ne!(demo::route_of(1), demo::route_of(2));
}
