//! AT-15 (kernel side) and the WI-6 order-independence guarantee: the Perception phase.
//! Requirements: REQ-K-2, REQ-K-4, REQ-K-5, ADR 0013 (amended), ADR 0015, ASM-079, ASM-081.
//!
//! Step 4 of M3: written before WI-6. `Kernel::spawn_with` is a `todo!()` stub, so tests that
//! spawn entities with sensors are ignored with that work item. Sensors here are range checks
//! (`agents::RangeSensor`); `builtin.visual` is tested in `umbra-engine` (AT-15).

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod agents;
mod common;

use std::sync::{Arc, Mutex};

use umbra_core::{
    Contact, EntityId, Force, Intent, Ownership, RemoteSource, Status, StreamKey, Vec3,
};
use umbra_kernel::{AgentInit, Kernel};

use agents::{RangeSensor, Scripted};
use common::{Behavior as Motion, Stub, at, kernel, owned};

fn observer(sensors: Vec<Arc<RangeSensor>>) -> AgentInit {
    AgentInit {
        sensors: sensors
            .into_iter()
            .map(|s| s as Arc<dyn umbra_core::Sensor>)
            .collect(),
        ..AgentInit::default()
    }
}

fn id(raw: u64) -> EntityId {
    EntityId::new(raw).expect("non-zero")
}

fn contacts_of(k: &Kernel, index: usize) -> Vec<Contact> {
    k.snapshot().entities[index].contacts.clone()
}

/// An entity without sensors has no contacts (live: holds on the M2 spawn path too).
#[test]
fn given_entity_without_sensors_then_snapshot_contacts_are_empty() {
    let model = Stub::new("test.model.still", Motion::Stationary);
    let mut k = kernel(1);
    k.spawn(owned(at(0.0), &model)).expect("spawn");
    k.spawn(owned(at(10.0), &model)).expect("spawn");

    k.tick().expect("tick");

    assert_eq!(contacts_of(&k, 0), Vec::<Contact>::new());
    assert_eq!(contacts_of(&k, 1), Vec::<Contact>::new());
}

/// ADR 0015: contacts hold the detected entities among all others, sorted by entity id, with
/// position, velocity, force and the range the sensor reported.
#[test]
fn given_sensor_when_ticked_then_contacts_are_sorted_by_id_and_carry_target_data() {
    let model = Stub::new("test.model.still", Motion::Stationary);
    let mut k = kernel(1);
    k.spawn_with(
        owned(at(0.0), &model),
        observer(vec![Arc::new(RangeSensor::new("test.sensor.range", 100.0))]),
    )
    .expect("spawn_with");
    let red = AgentInit {
        force: Some(Force::new("red").expect("valid force")),
        ..AgentInit::default()
    };
    k.spawn_with(owned(at(60.0), &model), red)
        .expect("spawn_with");
    k.spawn(owned(at(30.0), &model)).expect("spawn");
    k.spawn(owned(at(900.0), &model)).expect("spawn");

    k.tick().expect("tick");
    let contacts = contacts_of(&k, 0);

    assert_eq!(
        contacts.iter().map(|c| c.entity).collect::<Vec<_>>(),
        [id(2), id(3)]
    );
    assert_eq!(contacts[0].position, at(60.0));
    assert_eq!(contacts[0].force, Force::new("red").ok());
    assert_eq!(contacts[1].force, None);
    assert!((contacts[0].range.0 - 100.0).abs() < f64::EPSILON);
}

/// ADR 0015: with several sensors, contacts are merged per target and the first sensor in
/// template order wins; a target only the second sensor sees is added; the result is sorted.
#[test]
fn given_two_sensors_when_ticked_then_contacts_merge_first_sensor_wins_and_sorted() {
    let model = Stub::new("test.model.still", Motion::Stationary);
    let near = Arc::new(RangeSensor::new("test.sensor.near", 50.0).tagged(1.0));
    let far = Arc::new(RangeSensor::new("test.sensor.far", 500.0).tagged(2.0));
    let mut k = kernel(1);
    k.spawn_with(owned(at(0.0), &model), observer(vec![near, far]))
        .expect("spawn_with");
    k.spawn(owned(at(300.0), &model)).expect("spawn");
    k.spawn(owned(at(20.0), &model)).expect("spawn");

    k.tick().expect("tick");
    let contacts = contacts_of(&k, 0);

    assert_eq!(
        contacts.iter().map(|c| c.entity).collect::<Vec<_>>(),
        [id(2), id(3)]
    );
    assert!(
        (contacts[0].range.0 - 2.0).abs() < f64::EPSILON,
        "only the far sensor sees 2"
    );
    assert!(
        (contacts[1].range.0 - 1.0).abs() < f64::EPSILON,
        "the near sensor wins for 3"
    );
}

/// ADR 0013 (amended): perception works on the state before the tick. In tick 1 a drifting
/// target is still where it was spawned, however far it moves during that tick.
#[test]
fn given_drifting_target_when_first_tick_then_contact_position_is_the_pre_tick_position() {
    let still = Stub::new("test.model.still", Motion::Stationary);
    let drift = Stub::new(
        "test.model.drift",
        Motion::Drift(Vec3::new(0.0, 900.0, 0.0)),
    );
    let mut k = kernel(1);
    k.spawn_with(
        owned(at(0.0), &still),
        observer(vec![Arc::new(RangeSensor::new(
            "test.sensor.range",
            1000.0,
        ))]),
    )
    .expect("spawn_with");
    k.spawn(owned(at(500.0), &drift)).expect("spawn");

    k.tick().expect("tick");
    let after_one = contacts_of(&k, 0);
    k.tick().expect("tick");
    let after_two = contacts_of(&k, 0);

    assert_eq!(after_one[0].position, at(500.0));
    assert_eq!(
        after_two[0].position,
        at(530.0),
        "30 m moved in tick 1 (900 m/s at 30 Hz)"
    );
}

/// REQ-K-5: Remote entities are perceived by Owned observers, but never observe themselves.
#[test]
fn given_remote_entities_when_ticked_then_they_are_perceived_but_get_no_contacts() {
    let model = Stub::new("test.model.still", Motion::Stationary);
    let sensor = Arc::new(RangeSensor::new("test.sensor.range", 1000.0));
    let mut k = kernel(1);
    k.spawn_with(owned(at(0.0), &model), observer(vec![Arc::clone(&sensor)]))
        .expect("spawn_with");
    let mut remote = owned(at(100.0), &model);
    remote.ownership = Ownership::Remote {
        source: RemoteSource::new("net.test"),
    };
    k.spawn_with(remote, observer(vec![Arc::clone(&sensor)]))
        .expect("spawn_with");

    k.tick().expect("tick");

    assert_eq!(
        contacts_of(&k, 0)
            .iter()
            .map(|c| c.entity)
            .collect::<Vec<_>>(),
        [id(2)]
    );
    assert!(
        contacts_of(&k, 1).is_empty(),
        "a Remote entity is not an observer"
    );
    assert!(
        sensor.records().iter().all(|r| r.observer == id(1)),
        "the sensor ran for the Owned entity only"
    );
}

/// ADR 0015 (amended): the sensor is given all other entities in id order and the observer's own
/// RNG stream (the stream `StreamKey::entity(observer)` of the master seed), not another one.
#[test]
fn given_drawing_sensor_when_ticked_then_it_draws_from_the_observers_own_stream() {
    let model = Stub::new("test.model.still", Motion::Stationary);
    let sensor = Arc::new(RangeSensor::new("test.sensor.range", 1000.0).drawing());
    let mut k = kernel(21);
    k.spawn(owned(at(0.0), &model)).expect("spawn");
    k.spawn_with(owned(at(10.0), &model), observer(vec![Arc::clone(&sensor)]))
        .expect("spawn_with");
    k.spawn(owned(at(20.0), &model)).expect("spawn");

    k.tick().expect("tick");
    let records = sensor.records();

    assert_eq!(records.len(), 1);
    assert_eq!(records[0].observer, id(2));
    assert_eq!(records[0].candidates, [id(1), id(3)]);
    let mut expected = k.rng().stream(StreamKey::entity(id(2)));
    assert_eq!(records[0].draw, Some(expected.next_u64()));
}

/// Perception runs before Behavior in the same tick: the behavior sees this tick's contacts.
#[test]
fn given_sensor_and_behavior_when_first_tick_then_behavior_sees_the_contacts() {
    let model = Stub::new("test.model.still", Motion::Stationary);
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&seen);
    let behavior = Scripted::arc("test.behavior.look", move || {
        let log = Arc::clone(&log);
        Box::new(move |_, view, _| {
            log.lock().expect("log lock").push(view.contacts.len());
            Status::Running
        })
    });
    let mut k = kernel(1);
    let agent = AgentInit {
        behavior: Some(behavior),
        sensors: vec![Arc::new(RangeSensor::new("test.sensor.range", 100.0))],
        ..AgentInit::default()
    };
    k.spawn_with(owned(at(0.0), &model), agent)
        .expect("spawn_with");
    k.spawn(owned(at(10.0), &model)).expect("spawn");

    k.tick().expect("tick");

    assert_eq!(*seen.lock().expect("log lock"), vec![1]);
}

/// Builds the same three agents (a, b, c) in the given spawn order. Every agent has a drawing
/// sensor and a behavior that draws and emits what it drew and how many contacts it saw
/// (`"<draw>:<contacts>"` in a `Custom` payload). Returns the kernel and the id of each name.
fn world(order: &[char]) -> (Kernel, Vec<(char, EntityId)>) {
    let model = Stub::new("test.model.drift", Motion::Drift(Vec3::new(0.0, 4.0, 0.0)));
    let mut k = kernel(77);
    let mut ids = Vec::new();
    for &name in order {
        let y = match name {
            'a' => 0.0,
            'b' => 40.0,
            _ => 80.0,
        };
        let behavior = Scripted::arc("test.behavior.draw", || {
            Box::new(|ctx, view, out| {
                let drawn = ctx.rng.next_u64();
                out.push(Intent::Custom {
                    kind: umbra_core::ComponentId::new("test.custom.seen").expect("valid id"),
                    payload: umbra_core::ParamValue::String(format!(
                        "{drawn}:{}",
                        view.contacts.len()
                    )),
                });
                Status::Running
            })
        });
        let agent = AgentInit {
            behavior: Some(behavior),
            sensors: vec![Arc::new(
                RangeSensor::new("test.sensor.range", 60.0).drawing(),
            )],
            ..AgentInit::default()
        };
        let id = k
            .spawn_with(owned(at(y), &model), agent)
            .expect("spawn_with");
        ids.push((name, id));
    }
    (k, ids)
}

/// Per entity name: the names of its contacts (sorted, since ids differ between the kernels) and
/// the contact count its behavior reported.
fn shown(k: &Kernel, ids: &[(char, EntityId)]) -> Vec<(char, Vec<char>, String)> {
    let name_of = |e: EntityId| ids.iter().find(|(_, i)| *i == e).map(|(n, _)| *n);
    let snapshot = k.snapshot();
    let mut out: Vec<(char, Vec<char>, String)> = ids
        .iter()
        .map(|&(name, id)| {
            let e = snapshot
                .entities
                .iter()
                .find(|e| e.id == id)
                .expect("entity present");
            let mut contacts: Vec<char> = e
                .contacts
                .iter()
                .map(|c| name_of(c.entity).expect("known entity"))
                .collect();
            contacts.sort_unstable();
            let Intent::Custom { payload, .. } = &e.intents[0] else {
                panic!("expected the custom intent, got {:?}", e.intents[0]);
            };
            let text = payload.as_str().expect("string payload");
            let count = text.split(':').nth(1).expect("count part").to_string();
            (name, contacts, count)
        })
        .collect();
    out.sort_by_key(|(n, _, _)| *n);
    out
}

/// ADR 0013 (amended 2026-10-17): each entity has its own contact vector, intent buffer and RNG
/// stream and sees only pre-tick state, so the outcome of the Perception and Behavior phases does
/// not depend on the order entities are visited. Two kernels with the same entities spawned in a
/// different order (so visited in a different order) agree per entity, by name, on intents,
/// contacts and the values drawn, for several ticks.
///
/// The RNG stream of an entity is keyed by its id, so the id of an entity differs between the
/// kernels; what is compared is the entity's behavior given its own stream: the draws are compared
/// against the stream of the entity's id in each kernel.
#[test]
fn perception_and_behavior_outcome_does_not_depend_on_entity_visiting_order() {
    let (mut forward, ids_f) = world(&['a', 'b', 'c']);
    let (mut backward, ids_b) = world(&['c', 'b', 'a']);

    for tick in 1..=5 {
        forward.tick().expect("tick");
        backward.tick().expect("tick");
        assert_eq!(
            shown(&forward, &ids_f),
            shown(&backward, &ids_b),
            "contacts and contact counts per entity at tick {tick}"
        );
    }
    // Each agent's own draws match the stream its id selects, in both kernels.
    for (kernel, ids) in [(&forward, &ids_f), (&backward, &ids_b)] {
        for &(_, id) in ids {
            let mut stream = kernel.rng().stream(StreamKey::entity(id));
            let snapshot = kernel.snapshot();
            let e = snapshot
                .entities
                .iter()
                .find(|e| e.id == id)
                .expect("entity");
            // Tick 5: the sensor drew, then the behavior drew; the behavior's value is draw 10.
            for _ in 0..9 {
                let _ = stream.next_u64();
            }
            let tenth = stream.next_u64();
            let Intent::Custom { payload, .. } = &e.intents[0] else {
                panic!("expected the custom intent, got {:?}", e.intents[0]);
            };
            let text = payload.as_str().expect("string payload");
            assert!(text.starts_with(&format!("{tenth}:")), "{text} vs {tenth}");
        }
    }
}
