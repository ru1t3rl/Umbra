//! AT-12 (kernel row, M7): `TickReport::lifecycle` carries the `Spawn` and `Despawn` intents of
//! Owned entities for the engine to apply. Requirements: REQ-B-2, REQ-K-5, D-46, ADR 0052,
//! ASM-600, ASM-602, ASM-605.
//!
//! Written in M7 WI-1 against the stub, live since M7 WI-4. The compatibility rows (a bare kernel
//! applies nothing; an empty report when nothing is emitted; lifecycle intents are not hashed)
//! passed from the start and stay green.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod agents;
mod common;

use std::sync::Arc;

use umbra_core::{
    EntityId, Intent, MetersPerSecond, Ownership, Radians, RemoteSource, Status, TemplateId,
    WeaponId,
};
use umbra_kernel::{AgentInit, Kernel};

use agents::Scripted;
use common::{Behavior as Motion, Stub, at, kernel, owned};

fn agent(b: Arc<dyn umbra_core::Behavior>) -> AgentInit {
    AgentInit {
        behavior: Some(b),
        ..AgentInit::default()
    }
}

/// A behavior that appends `intents` every tick and returns Running.
fn emitting(id: &str, intents: Vec<Intent>) -> Arc<dyn umbra_core::Behavior> {
    Scripted::arc(id, move || {
        let intents = intents.clone();
        Box::new(move |_, _, out| {
            for i in &intents {
                out.push(i.clone());
            }
            Status::Running
        })
    })
}

fn spawn_of(template: &str, y: f64) -> Intent {
    Intent::Spawn {
        template: TemplateId::new(template).expect("valid template id"),
        position: at(y),
        heading: Radians(0.25),
    }
}

fn id(n: u64) -> EntityId {
    EntityId::new(n).expect("non-zero")
}

fn add(k: &mut Kernel, y: f64, remote: bool, intents: Vec<Intent>) {
    let model = Stub::new("test.model.still", Motion::Stationary);
    let mut init = owned(at(y), &model);
    if remote {
        init.ownership = Ownership::Remote {
            source: RemoteSource::new("net.test"),
        };
    }
    k.spawn_with(init, agent(emitting("test.behavior.loud", intents)))
        .expect("spawn_with");
}

/// ADR 0052: Owned entities' lifecycle intents appear in id order and, within an entity, in
/// emission order; other intents are not copied; the report holds the tick's intents only.
#[test]
fn given_owned_emitters_when_ticked_then_lifecycle_is_in_id_then_emission_order() {
    let mut k = kernel(3);
    let fire = Intent::Fire {
        target: id(1),
        weapon: WeaponId::new("example.weapon.main_gun").expect("valid weapon id"),
    };
    add(
        &mut k,
        0.0,
        false,
        vec![
            spawn_of("example.a", 1.0),
            Intent::Despawn,
            spawn_of("example.b", 2.0),
        ],
    );
    add(
        &mut k,
        10.0,
        false,
        vec![Intent::Hold, fire, spawn_of("example.c", 3.0)],
    );
    add(&mut k, 20.0, false, vec![Intent::Despawn]);

    let report = k.tick().expect("tick");

    assert_eq!(
        report.lifecycle,
        vec![
            (id(1), spawn_of("example.a", 1.0)),
            (id(1), Intent::Despawn),
            (id(1), spawn_of("example.b", 2.0)),
            (id(2), spawn_of("example.c", 3.0)),
            (id(3), Intent::Despawn),
        ]
    );
}

/// REQ-K-5: a Remote entity is never ticked, so it contributes no lifecycle intents.
#[test]
fn given_a_remote_entity_with_a_behavior_when_ticked_then_it_contributes_nothing() {
    let mut k = kernel(3);
    add(
        &mut k,
        0.0,
        true,
        vec![Intent::Despawn, spawn_of("example.a", 1.0)],
    );
    add(&mut k, 10.0, false, vec![Intent::Despawn]);

    let report = k.tick().expect("tick");

    assert_eq!(report.lifecycle, vec![(id(2), Intent::Despawn)]);
}

/// ADR 0052: the field describes one tick: a behavior that emits only once reports only once.
#[test]
fn given_an_emitter_that_stops_when_ticked_twice_then_the_second_report_is_empty() {
    let mut k = kernel(3);
    let model = Stub::new("test.model.still", Motion::Stationary);
    let once = Scripted::arc("test.behavior.once", || {
        let mut first = true;
        Box::new(move |_, _, out| {
            if std::mem::take(&mut first) {
                out.push(Intent::Despawn);
            }
            Status::Running
        })
    });
    k.spawn_with(owned(at(0.0), &model), agent(once))
        .expect("spawn_with");

    let first = k.tick().expect("tick");
    let second = k.tick().expect("tick");

    assert_eq!(first.lifecycle, vec![(id(1), Intent::Despawn)]);
    assert_eq!(second.lifecycle, Vec::<(EntityId, Intent)>::new());
}

/// Compatibility (frozen M3 rule, ADR 0016/0052): a bare kernel applies neither intent: the entity
/// count is unchanged and the state hash equals that of an entity that emits nothing.
#[test]
fn given_a_bare_kernel_when_lifecycle_intents_are_emitted_then_nothing_is_applied() {
    let mut with = kernel(4);
    let mut without = kernel(4);
    add(
        &mut with,
        0.0,
        false,
        vec![spawn_of("example.a", 5.0), Intent::Despawn],
    );
    add(&mut without, 0.0, false, Vec::new());

    for _ in 0..3 {
        with.tick().expect("tick");
        without.tick().expect("tick");
    }

    assert_eq!(with.entity_count(), 1);
    assert_eq!(with.state_hash(), without.state_hash());
}

/// Compatibility: an entity that emits no lifecycle intent leaves the report's list empty.
#[test]
fn given_no_lifecycle_intents_when_ticked_then_the_list_is_empty() {
    let mut k = kernel(4);
    add(
        &mut k,
        0.0,
        false,
        vec![Intent::SetSpeed(MetersPerSecond(1.0))],
    );

    let report = k.tick().expect("tick");

    assert_eq!(report.lifecycle, Vec::<(EntityId, Intent)>::new());
}

/// ADR 0052, ASM-601: `Kernel::force_of` reads one entity's force without listing all owned
/// entities; `None` for a neutral entity and for an unknown id.
#[test]
fn given_a_force_when_read_by_id_then_it_is_returned_and_neutral_or_unknown_give_none() {
    let mut k = kernel(3);
    let model = Stub::new("test.model.still", Motion::Stationary);
    let blue = umbra_core::Force::new("blue").expect("valid force");
    k.spawn_with(
        owned(at(0.0), &model),
        AgentInit {
            force: Some(blue.clone()),
            ..AgentInit::default()
        },
    )
    .expect("spawn with a force");
    k.spawn_with(owned(at(1.0), &model), AgentInit::default())
        .expect("spawn neutral");

    assert_eq!(k.force_of(id(1)), Some(blue));
    assert_eq!(k.force_of(id(2)), None);
    assert_eq!(k.force_of(id(99)), None);
}
