//! AT-1 (kernel rows): the Events phase. Requirements: REQ-C-5, REQ-K-2, REQ-K-4, REQ-K-5,
//! ADR 0028, ADR 0016.
//!
//! `Fire` and `Custom` intents of a tick become `SimEvent`s in `TickReport::events`, Owned
//! entities in id order, each entity's intents in emission order. Nothing is hashed and no state
//! changes; `Spawn` and `Despawn` are still recorded only.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod agents;
mod common;

use std::sync::Arc;

use umbra_core::{
    ComponentId, EntityId, Intent, MetersPerSecond, Ownership, ParamValue, Radians, RemoteSource,
    SimEvent, Status, TemplateId, WeaponId,
};
use umbra_kernel::AgentInit;

use agents::Scripted;
use common::{Behavior as Motion, Stub, at, kernel, owned};

fn id(n: u64) -> EntityId {
    EntityId::new(n).expect("non-zero")
}

fn weapon() -> WeaponId {
    WeaponId::new("example.weapon.main_gun").expect("valid id")
}

fn ping() -> ComponentId {
    ComponentId::new("example.custom.ping").expect("valid id")
}

fn fire_at(target: u64) -> Intent {
    Intent::Fire {
        target: id(target),
        weapon: weapon(),
    }
}

fn custom(n: i64) -> Intent {
    Intent::Custom {
        kind: ping(),
        payload: ParamValue::from(n),
    }
}

fn agent(b: Arc<dyn umbra_core::Behavior>) -> AgentInit {
    AgentInit {
        behavior: Some(b),
        ..AgentInit::default()
    }
}

/// A behavior that appends `intents` every tick and returns Running.
fn emitting(intents: Vec<Intent>) -> Arc<dyn umbra_core::Behavior> {
    Scripted::arc("test.behavior.loud", move || {
        let intents = intents.clone();
        Box::new(move |_, _, out| {
            for i in &intents {
                out.push(i.clone());
            }
            Status::Running
        })
    })
}

/// A behavior that emits `intents` on its first tick only.
fn once(intents: Vec<Intent>) -> Arc<dyn umbra_core::Behavior> {
    Scripted::arc("test.behavior.once", move || {
        let intents = intents.clone();
        let mut first = true;
        Box::new(move |_, _, out| {
            if std::mem::take(&mut first) {
                for i in &intents {
                    out.push(i.clone());
                }
            }
            Status::Running
        })
    })
}

fn still() -> Arc<Stub> {
    Stub::new("test.model.still", Motion::Stationary)
}

/// REQ-C-5: a `Fire` intent becomes a `Fire` event naming the emitting entity as shooter.
#[test]
fn fire_intent_becomes_fire_event() {
    let model = still();
    let mut k = kernel(1);
    k.spawn_with(owned(at(0.0), &model), agent(emitting(vec![fire_at(2)])))
        .expect("spawn");

    let report = k.tick().expect("tick");

    assert_eq!(
        report.events,
        vec![SimEvent::Fire {
            shooter: id(1),
            target: id(2),
            weapon: weapon(),
        }]
    );
}

/// REQ-C-5: a `Custom` intent becomes a `Custom` event with the emitting entity as its source.
#[test]
fn custom_intent_becomes_custom_event_with_source() {
    let model = still();
    let mut k = kernel(1);
    k.spawn(owned(at(0.0), &model)).expect("spawn");
    k.spawn_with(owned(at(5.0), &model), agent(emitting(vec![custom(3)])))
        .expect("spawn");

    let report = k.tick().expect("tick");

    assert_eq!(
        report.events,
        vec![SimEvent::Custom {
            source: Some(id(2)),
            kind: ping(),
            payload: ParamValue::from(3),
        }]
    );
}

/// ADR 0028, REQ-K-4: events are ordered by entity id, then by emission order within an entity,
/// whatever other intents sit among them (non-event intents produce nothing).
#[test]
fn events_in_entity_id_then_emission_order() {
    let model = still();
    let mut k = kernel(1);
    let speed = Intent::SetSpeed(MetersPerSecond(1.0));
    k.spawn_with(
        owned(at(0.0), &model),
        agent(emitting(vec![
            custom(1),
            speed.clone(),
            fire_at(3),
            custom(2),
        ])),
    )
    .expect("spawn");
    k.spawn_with(owned(at(5.0), &model), agent(emitting(vec![fire_at(1)])))
        .expect("spawn");
    k.spawn_with(
        owned(at(10.0), &model),
        agent(emitting(vec![custom(9), speed])),
    )
    .expect("spawn");

    let report = k.tick().expect("tick");

    let custom_ev = |source: u64, n: i64| SimEvent::Custom {
        source: Some(id(source)),
        kind: ping(),
        payload: ParamValue::from(n),
    };
    let fire_ev = |shooter: u64, target: u64| SimEvent::Fire {
        shooter: id(shooter),
        target: id(target),
        weapon: weapon(),
    };
    assert_eq!(
        report.events,
        vec![
            custom_ev(1, 1),
            fire_ev(1, 3),
            custom_ev(1, 2),
            fire_ev(2, 1),
            custom_ev(3, 9),
        ]
    );
}

/// REQ-K-5, end to end: a Remote entity emits no events. This cannot reach the ownership guard of
/// the Events system, because the Behavior phase gives Remote entities no intents; the guard is
/// covered by the unit test in `systems/events.rs`.
#[test]
fn remote_entities_emit_no_events() {
    let model = still();
    let mut k = kernel(1);
    let mut remote = owned(at(0.0), &model);
    remote.ownership = Ownership::Remote {
        source: RemoteSource::new("net.test"),
    };
    k.spawn_with(remote, agent(emitting(vec![fire_at(2), custom(1)])))
        .expect("spawn");

    for _ in 0..3 {
        let report = k.tick().expect("tick");
        assert!(report.events.is_empty(), "{:?}", report.events);
    }
}

/// Events are per tick: an intent emitted once yields one event once, and a tick with no such
/// intents reports none.
#[test]
fn events_do_not_repeat_in_later_ticks() {
    let model = still();
    let mut k = kernel(1);
    k.spawn_with(owned(at(0.0), &model), agent(once(vec![fire_at(1)])))
        .expect("spawn");

    assert_eq!(k.tick().expect("tick").events.len(), 1);
    assert_eq!(k.tick().expect("tick").events.len(), 0);
    assert_eq!(k.tick().expect("tick").events.len(), 0);
}

/// ADR 0028, REQ-K-4: events are not hashed and change no state. A kernel whose entity fires and
/// signals every tick has, tick by tick, the same state hash as one whose
/// entity emits no events.
#[test]
fn events_change_no_state_hash() {
    let model = still();
    let mut loud = kernel(4);
    let mut quiet = kernel(4);
    loud.spawn_with(
        owned(at(0.0), &model),
        agent(emitting(vec![fire_at(1), custom(3)])),
    )
    .expect("spawn");
    quiet
        .spawn_with(owned(at(0.0), &model), agent(emitting(Vec::new())))
        .expect("spawn");

    for tick in 1..=5 {
        let report = loud.tick().expect("tick");
        quiet.tick().expect("tick");
        assert_eq!(report.events.len(), 2, "events at tick {tick}");
        assert_eq!(loud.state_hash(), quiet.state_hash(), "hash at tick {tick}");
    }
}

/// ADR 0028 (option 1), ADR 0016: `Spawn` and `Despawn` intents are recorded and applied by
/// nobody until M7; they produce no event and no entity change.
#[test]
fn spawn_and_despawn_intents_are_still_not_applied() {
    let model = still();
    let mut k = kernel(2);
    k.spawn_with(
        owned(at(0.0), &model),
        agent(emitting(vec![
            Intent::Spawn {
                template: TemplateId::new("example.tank.blue").expect("valid id"),
                position: at(5.0),
                heading: Radians(0.5),
            },
            Intent::Despawn,
        ])),
    )
    .expect("spawn");

    for _ in 0..3 {
        let report = k.tick().expect("tick");
        assert!(report.events.is_empty(), "{:?}", report.events);
    }

    assert_eq!(k.entity_count(), 1);
    assert_eq!(k.snapshot().entities[0].intents.len(), 2, "still recorded");
}
