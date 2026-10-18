//! AT-15: `builtin.visual`, the first sensor (ADR 0015, ASM-080): range, field of view, line of
//! sight, eye height, ordering, remote targets, forces.
//! Requirements: Perception (M3 deliverable), REQ-K-5, REQ-C-6, ADR 0015, ASM-079, ASM-080.
//!
//! Step 4 of M3: written before WI-15; run since WI-15, which implemented `builtin.visual`
//! and `standard_registry()`. Entities are placed with a local
//! frame at 52 N 4 E on flat terrain at 0 m (unless a test says otherwise); the observer looks
//! along the heading in its test. The "edge is inclusive" rule cannot be tested at exactly half the
//! field of view (floating-point bearings), so it is tested a hair inside and a hair outside.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod behavior_support;
mod common;

use std::ops::ControlFlow;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use behavior_support::standard;
use cgf_core::{
    Contact, Ecef, EntityId, Enu, Euler, FlatTerrain, Force, Geodetic, KinematicState, LocalFrame,
    Meters, MetersPerSecond, MotionModel, Ownership, Position, Radians, RemoteSource, SenseContext,
    Sensor, SimRng, SimTime, StreamKey, Target, TerrainProvider, TickRate, hostile,
};
use cgf_core::{ContactBuffer, Observer};
use cgf_engine::{AgentInit, ClockMode, Engine, EngineConfig, SpawnSpec};
use cgf_kernel::ManualWallClock;
use cgf_motion::{GroundKinematic, GroundKinematicParams};
use serde_json::{Value, json};

fn origin() -> Geodetic {
    Geodetic::from_degrees(52.0, 4.0, 0.0).expect("origin")
}

fn rate() -> TickRate {
    TickRate::new(30).expect("30 Hz")
}

fn eid(n: u64) -> EntityId {
    EntityId::new(n).expect("non-zero")
}

fn force(s: &str) -> Force {
    Force::new(s).expect("force")
}

/// The point `east` and `north` meters from the origin on the ground (the tangent plane is
/// dropped onto the ellipsoid), `up` meters above the ellipsoid (flat terrain is at 0 m, so this is
/// also the height above ground).
fn at(east: f64, north: f64, up: f64) -> Ecef {
    let ground = LocalFrame::at(origin())
        .from_enu(Enu {
            east,
            north,
            up: 0.0,
        })
        .to_geodetic();
    ground
        .with_height(Meters(up))
        .expect("finite height")
        .to_ecef()
}

/// A stationary state at `position` looking along `heading_deg` (clockwise from north).
fn state(position: Ecef, heading_deg: f64) -> KinematicState {
    let frame = LocalFrame::at(position.to_geodetic());
    KinematicState {
        position,
        velocity: cgf_core::Vec3::ZERO,
        orientation: frame.orientation_from_euler(Euler {
            heading: Radians::from_degrees(heading_deg),
            pitch: Radians(0.0),
            roll: Radians(0.0),
        }),
    }
}

fn visual(params: &Value) -> Arc<dyn Sensor> {
    standard()
        .sensor(&cgf_core::ComponentId::new("builtin.visual").expect("id"))
        .expect("builtin.visual is registered")
        .build(params)
        .expect("builds")
}

/// What one `sense` call returns for an observer at the origin (id 1, optional force) looking along
/// `heading_deg` and the given `(id, position, force, ownership)` candidates, in the given order.
fn sense(
    sensor: &dyn Sensor,
    heading_deg: f64,
    observer_force: Option<&Force>,
    candidates: &[(u64, Ecef, Option<Force>, Ownership)],
    terrain: &dyn TerrainProvider,
) -> Vec<Contact> {
    let me = state(at(0.0, 0.0, 0.0), heading_deg);
    let states: Vec<KinematicState> = candidates.iter().map(|c| state(c.1, 0.0)).collect();
    let targets: Vec<Target<'_>> = candidates
        .iter()
        .zip(&states)
        .map(|(c, s)| Target::new(eid(c.0), s, c.2.as_ref(), &c.3))
        .collect();
    let mut rng = SimRng::new(1).stream(StreamKey::entity(eid(1)));
    let mut ctx = SenseContext::new(SimTime::at_tick(rate(), 0), &mut rng, terrain);
    let mut out = Vec::new();
    sensor.sense(
        &mut ctx,
        &Observer::new(eid(1), &me, observer_force),
        &targets,
        &mut ContactBuffer::new(&mut out),
    );
    out
}

fn flat() -> FlatTerrain {
    FlatTerrain {
        height: Meters(0.0),
    }
}

fn one(id: u64, position: Ecef) -> Vec<(u64, Ecef, Option<Force>, Ownership)> {
    vec![(id, position, None, Ownership::Owned)]
}

fn ids(contacts: &[Contact]) -> Vec<u64> {
    contacts.iter().map(|c| c.entity.get()).collect()
}

/// ASM-080: a target inside range and field of view is a contact, with its position, velocity,
/// force and the 3D range to it.
#[test]
fn detects_inside_range_and_fov() {
    let sensor = visual(&json!({ "range_m": 3000.0, "fov_deg": 120.0 }));
    let candidates = vec![
        (
            2,
            at(1000.0, 0.0, 0.0),
            Some(force("red")),
            Ownership::Owned,
        ),
        (3, at(800.0, 500.0, 0.0), None, Ownership::Owned),
    ];

    let contacts = sense(sensor.as_ref(), 90.0, None, &candidates, &flat());

    assert_eq!(ids(&contacts), [2, 3]);
    assert!(
        (contacts[0].range.0 - 1000.0).abs() < 1.0,
        "range {}",
        contacts[0].range.0
    );
    assert_eq!(contacts[0].position, candidates[0].1);
    assert_eq!(contacts[0].force, Some(force("red")));
    assert_eq!(contacts[1].force, None);
}

/// ASM-080: the range is the 3D distance, `range_m` inclusive: 2900 m is seen, 3100 m is not, and
/// a target 2000 m ahead and 2300 m above is 3049 m away, so not seen.
#[test]
fn misses_outside_range() {
    let sensor = visual(&json!({ "range_m": 3000.0, "fov_deg": 360.0, "eye_height_m": 2.0 }));

    let near = sense(
        sensor.as_ref(),
        90.0,
        None,
        &one(2, at(2900.0, 0.0, 0.0)),
        &flat(),
    );
    let far = sense(
        sensor.as_ref(),
        90.0,
        None,
        &one(2, at(3100.0, 0.0, 0.0)),
        &flat(),
    );
    let high = sense(
        sensor.as_ref(),
        90.0,
        None,
        &one(2, at(2000.0, 0.0, 2300.0)),
        &flat(),
    );

    assert_eq!(ids(&near), [2]);
    assert_eq!(ids(&far), Vec::<u64>::new());
    assert_eq!(ids(&high), Vec::<u64>::new());
}

/// ASM-080: the field of view is horizontal, centered on the heading, half-angle `fov_deg / 2`
/// inclusive: a hair inside the edge is seen, a hair outside is not; 360 degrees sees behind.
#[test]
fn misses_outside_fov_edge_inclusive() {
    let sensor = visual(&json!({ "range_m": 3000.0, "fov_deg": 90.0 }));
    let bearing = |deg: f64| {
        let r = deg.to_radians();
        let (sin, cos) = cgf_core::math::sin_cos(r);
        at(1000.0 * sin, 1000.0 * cos, 0.0)
    };

    let seen = |deg: f64| {
        ids(&sense(
            sensor.as_ref(),
            0.0,
            None,
            &one(2, bearing(deg)),
            &flat(),
        ))
    };

    assert_eq!(seen(44.9), [2]);
    assert_eq!(seen(-44.9), [2]);
    assert_eq!(seen(45.1), Vec::<u64>::new());
    assert_eq!(seen(-45.1), Vec::<u64>::new());
    assert_eq!(seen(180.0), Vec::<u64>::new());
    let all_round = visual(&json!({ "range_m": 3000.0, "fov_deg": 360.0 }));
    assert_eq!(
        ids(&sense(
            all_round.as_ref(),
            0.0,
            None,
            &one(2, bearing(180.0)),
            &flat()
        )),
        [2]
    );
}

/// A stub terrain with a ridge 100 m high between 800 m and 1200 m east of the origin.
struct Ridge;

impl TerrainProvider for Ridge {
    fn height_at(&self, at: Geodetic) -> Meters {
        let east = (at.lon().0.to_degrees() - 4.0) * 68_684.0;
        Meters(if (800.0..1200.0).contains(&east) {
            100.0
        } else {
            0.0
        })
    }

    fn surface_normal_at(&self, _: Geodetic) -> Enu {
        Enu {
            east: 0.0,
            north: 0.0,
            up: 1.0,
        }
    }
}

/// REQ-C-6, ASM-080: terrain between observer and target blocks the contact; the same pair on flat
/// terrain is seen.
#[test]
fn terrain_blocks_line_of_sight() {
    let sensor = visual(&json!({ "range_m": 3000.0, "fov_deg": 120.0 }));
    let target = one(2, at(2000.0, 0.0, 0.0));

    let behind_ridge = sense(sensor.as_ref(), 90.0, None, &target, &Ridge);
    let open = sense(sensor.as_ref(), 90.0, None, &target, &flat());

    assert_eq!(ids(&behind_ridge), Vec::<u64>::new());
    assert_eq!(ids(&open), [2]);
}

/// ASM-080: on smooth ground the horizon hides distant targets; raising both eyes by
/// `eye_height_m` lifts the line over the curvature. At 3 km the chord sags 0.18 m below the
/// surface: 0.05 m eyes cannot see, 0.2 m and 2 m eyes can.
#[test]
fn eye_height_lifts_over_curvature() {
    let seen_with = |eye: f64| {
        let sensor = visual(&json!({ "range_m": 5000.0, "fov_deg": 120.0, "eye_height_m": eye }));
        ids(&sense(
            sensor.as_ref(),
            90.0,
            None,
            &one(2, at(3000.0, 0.0, 0.0)),
            &flat(),
        ))
    };

    assert_eq!(seen_with(0.05), Vec::<u64>::new());
    assert_eq!(seen_with(0.2), [2]);
    assert_eq!(seen_with(2.0), [2]);
}

/// ASM-080: with `eye_height_m = 0` two entities on smooth ground never see each other, whatever
/// the range: the straight segment dips below the surface.
#[test]
fn eye_height_zero_on_smooth_ground_never_sees() {
    let sensor = visual(&json!({ "range_m": 5000.0, "fov_deg": 360.0, "eye_height_m": 0.0 }));

    for range in [50.0, 500.0, 2000.0, 4500.0] {
        let seen = sense(
            sensor.as_ref(),
            90.0,
            None,
            &one(2, at(range, 0.0, 0.0)),
            &flat(),
        );

        assert_eq!(ids(&seen), Vec::<u64>::new(), "range {range}");
    }
}

/// Counts `line_of_sight` calls and always answers yes.
struct Counting(AtomicUsize);

impl TerrainProvider for Counting {
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

    fn line_of_sight(&self, _: Ecef, _: Ecef) -> bool {
        self.0.fetch_add(1, Ordering::SeqCst);
        true
    }
}

/// ADR 0015 (amended): range and field of view are checked first and `line_of_sight`, the
/// expensive call, only for targets that pass both: of three targets (out of range, behind the
/// observer, in view) exactly one reaches the terrain.
#[test]
fn range_and_fov_are_checked_before_line_of_sight() {
    let sensor = visual(&json!({ "range_m": 3000.0, "fov_deg": 120.0 }));
    let terrain = Counting(AtomicUsize::new(0));
    let candidates = vec![
        (2, at(5000.0, 0.0, 0.0), None, Ownership::Owned),
        (3, at(-500.0, 0.0, 0.0), None, Ownership::Owned),
        (4, at(500.0, 0.0, 0.0), None, Ownership::Owned),
    ];

    let contacts = sense(sensor.as_ref(), 90.0, None, &candidates, &terrain);

    assert_eq!(ids(&contacts), [4]);
    assert_eq!(terrain.0.load(Ordering::SeqCst), 1);
}

fn engine() -> Engine {
    let config = EngineConfig {
        seed: 1,
        tick_rate: rate(),
        clock: ClockMode::Stepped,
    };
    Engine::with_wall_clock(config, Arc::new(flat()), Box::new(ManualWallClock::new()))
}

fn ground() -> Arc<dyn MotionModel> {
    Arc::new(
        GroundKinematic::new(GroundKinematicParams {
            max_speed: MetersPerSecond(18.0),
            max_turn_rate: cgf_core::RadiansPerSecond::from_degrees_per_second(40.0),
            max_accel: cgf_core::MetersPerSecondSquared(2.5),
        })
        .expect("params"),
    )
}

fn spawn(
    engine: &mut Engine,
    east: f64,
    ownership: Ownership,
    sensors: Vec<Arc<dyn Sensor>>,
) -> EntityId {
    let spec = SpawnSpec {
        position: Position::Ecef(at(east, 0.0, 0.0)),
        heading: Radians::from_degrees(90.0),
        speed: MetersPerSecond(0.0),
        ownership,
        motion: ground(),
        route: None,
    };
    engine
        .spawn_with(
            spec,
            AgentInit {
                sensors,
                ..AgentInit::default()
            },
        )
        .expect("spawn_with")
}

fn tick(engine: &mut Engine) -> cgf_engine::Snapshot {
    engine.step(1, |_| ControlFlow::Continue(())).expect("tick");
    engine.snapshot()
}

/// REQ-K-5: Remote entities are perceived (they are candidates), but only Owned entities observe:
/// a Remote entity with a sensor gets no contacts.
#[test]
fn remote_entities_are_perceived_observers_are_owned_only() {
    let sensor = visual(&json!({ "range_m": 3000.0, "fov_deg": 360.0 }));
    let remote = Ownership::Remote {
        source: RemoteSource::new("net.test"),
    };
    let mut engine = engine();
    let observer = spawn(
        &mut engine,
        0.0,
        Ownership::Owned,
        vec![Arc::clone(&sensor)],
    );
    let target = spawn(&mut engine, 1000.0, remote.clone(), Vec::new());
    let remote_observer = spawn(&mut engine, 1500.0, remote, vec![sensor]);

    let snapshot = tick(&mut engine);

    let contacts = |id: EntityId| -> Vec<EntityId> {
        snapshot
            .entities
            .iter()
            .find(|e| e.id == id)
            .map(|e| e.contacts.iter().map(|c| c.entity).collect())
            .expect("entity")
    };
    assert!(
        contacts(observer).contains(&target),
        "the Remote target is perceived"
    );
    assert!(contacts(observer).contains(&remote_observer));
    assert_eq!(
        contacts(remote_observer),
        Vec::<EntityId>::new(),
        "a Remote entity does not observe"
    );
}

/// ADR 0015: contacts are sorted by entity ID, and with several sensors the contacts are merged per
/// target: each target appears once.
#[test]
fn contacts_sorted_by_id_and_merged_across_sensors() {
    let short = visual(&json!({ "range_m": 500.0, "fov_deg": 360.0 }));
    let long = visual(&json!({ "range_m": 3000.0, "fov_deg": 360.0 }));
    let mut engine = engine();
    let observer = spawn(&mut engine, 0.0, Ownership::Owned, vec![short, long]);
    let near = spawn(&mut engine, 400.0, Ownership::Owned, Vec::new());
    let far = spawn(&mut engine, 1500.0, Ownership::Owned, Vec::new());
    let closest = spawn(&mut engine, 100.0, Ownership::Owned, Vec::new());

    let snapshot = tick(&mut engine);

    let seen: Vec<EntityId> = snapshot
        .entities
        .iter()
        .find(|e| e.id == observer)
        .map(|e| e.contacts.iter().map(|c| c.entity).collect())
        .expect("observer");
    assert_eq!(
        seen,
        [near, far, closest],
        "ascending IDs, one contact per target"
    );
}

/// ADR 0015 (added after the WI-16 review, AT-19 mutation 2): with ten targets a random contact
/// order passes only about 1 time in 3.6 million, so any map or hash iteration in perception shows
/// up here. Targets are spawned from far to near, so ID order is the reverse of range order.
#[test]
fn ten_contacts_come_out_in_ascending_id_order() {
    let sensor = visual(&json!({ "range_m": 3000.0, "fov_deg": 360.0 }));
    let mut engine = engine();
    let observer = spawn(&mut engine, 0.0, Ownership::Owned, vec![sensor]);
    let targets: Vec<EntityId> = (0..10_u32)
        .map(|i| {
            let metres = 2000.0 - 150.0 * f64::from(i);
            spawn(&mut engine, metres, Ownership::Owned, Vec::new())
        })
        .collect();

    let snapshot = tick(&mut engine);

    let seen: Vec<EntityId> = snapshot
        .entities
        .iter()
        .find(|e| e.id == observer)
        .map(|e| e.contacts.iter().map(|c| c.entity).collect())
        .expect("observer");
    assert_eq!(seen, targets);
}

/// ASM-079: a contact carries the target's force; same-force and neutral contacts are not hostile
/// (to a blue observer, or to a neutral one), a red contact is hostile to a blue observer only.
#[test]
fn neutral_and_same_force_not_hostile() {
    let sensor = visual(&json!({ "range_m": 3000.0, "fov_deg": 360.0 }));
    let candidates = vec![
        (
            2,
            at(300.0, 0.0, 0.0),
            Some(force("blue")),
            Ownership::Owned,
        ),
        (3, at(600.0, 0.0, 0.0), None, Ownership::Owned),
        (4, at(900.0, 0.0, 0.0), Some(force("red")), Ownership::Owned),
    ];
    let blue = force("blue");

    let by_blue = sense(sensor.as_ref(), 90.0, Some(&blue), &candidates, &flat());
    let by_neutral = sense(sensor.as_ref(), 90.0, None, &candidates, &flat());

    let hostile_ids = |contacts: &[Contact], me: Option<&Force>| -> Vec<u64> {
        contacts
            .iter()
            .filter(|c| hostile(me, c.force.as_ref()))
            .map(|c| c.entity.get())
            .collect()
    };
    assert_eq!(
        ids(&by_blue),
        [2, 3, 4],
        "perfect detection: everything in view is a contact"
    );
    assert_eq!(hostile_ids(&by_blue, Some(&blue)), [4]);
    assert_eq!(hostile_ids(&by_neutral, None), Vec::<u64>::new());
}
