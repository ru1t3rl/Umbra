//! WI-6 rows of AT-14 / REQ-K-4: `AgentInit` and `Kernel::spawn_with` (ADR 0011).
//! Requirements: REQ-B-1, REQ-B-5, REQ-K-4, ADR 0011, ASM-081.
//!
//! Step 4 of M3: `spawn_with` is a `todo!()` stub until WI-6, so tests that call it are ignored
//! with that work item. The default-value test is live.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod agents;
mod common;

use cgf_core::{Force, Intent, MetersPerSecond, Status, Vec3};
use cgf_kernel::{AgentInit, EntitySnapshot, KernelError};

use agents::Scripted;
use common::{Behavior as Motion, Stub, at, kernel, owned, route};

fn force(s: &str) -> Force {
    Force::new(s).expect("valid force")
}

/// ADR 0011: the default agent adds nothing (so `spawn` can be `spawn_with(init, default)`).
#[test]
fn given_default_agent_init_then_it_has_no_force_behavior_or_sensors() {
    let agent = AgentInit::default();

    assert!(agent.force.is_none());
    assert!(agent.behavior.is_none());
    assert!(agent.sensors.is_empty());
}

/// ADR 0011, ASM-082: `spawn_with(.., default)` is `spawn`: identical hashes and snapshots, tick by
/// tick, for entities with and without routes.
#[test]
#[ignore = "WI-6"]
fn given_default_agent_when_spawn_with_then_hashes_and_snapshots_match_plain_spawn() {
    let model = Stub::new("test.model.drift", Motion::Drift(Vec3::new(0.0, 3.0, 0.0)));
    let mut plain = kernel(7);
    let mut with = kernel(7);
    for k in [&mut plain, &mut with] {
        assert!(k.spawn(owned(at(0.0), &model)).is_ok());
    }
    let mut routed = owned(at(500.0), &model);
    routed.route = Some(route(vec![at(900.0), at(100.0)], 10.0));
    assert!(plain.spawn(routed.clone()).is_ok());
    assert!(with.spawn_with(routed, AgentInit::default()).is_ok());

    for tick in 1..=40 {
        plain.tick().expect("tick");
        with.tick().expect("tick");
        assert_eq!(plain.state_hash(), with.state_hash(), "hash at tick {tick}");
        assert_eq!(plain.snapshot(), with.snapshot(), "snapshot at tick {tick}");
    }
}

/// ADR 0013 amendment: a behavior whose `instantiate` fails is `KernelError::Instantiate`, and the
/// kernel is unchanged: no entity, no id consumed.
#[test]
#[ignore = "WI-6"]
fn given_failing_instantiate_when_spawn_with_then_error_and_no_id_consumed() {
    let model = Stub::new("test.model.still", Motion::Stationary);
    let mut k = kernel(1);
    let agent = AgentInit {
        behavior: Some(Scripted::failing("test.behavior.failing")),
        ..AgentInit::default()
    };

    let err = k.spawn_with(owned(at(0.0), &model), agent).err();

    assert!(matches!(err, Some(KernelError::Instantiate(_))), "{err:?}");
    assert_eq!(k.entity_count(), 0);
    let next = k.spawn(owned(at(0.0), &model)).expect("spawn");
    assert_eq!(next.get(), 1, "the failed spawn consumed no id");
}

/// REQ-B-5: the snapshot names the behavior and the force of an agent.
#[test]
#[ignore = "WI-6"]
fn given_agent_with_force_and_behavior_when_snapshot_then_both_are_reported() {
    let model = Stub::new("test.model.still", Motion::Stationary);
    let mut k = kernel(1);
    let agent = AgentInit {
        force: Some(force("blue")),
        behavior: Some(Scripted::arc("test.behavior.idle", || {
            Box::new(|_, _, _| Status::Success)
        })),
        ..AgentInit::default()
    };
    k.spawn_with(owned(at(0.0), &model), agent).expect("spawn");
    k.spawn(owned(at(10.0), &model)).expect("spawn");

    k.tick().expect("tick");
    let snapshot = k.snapshot();

    let agent: &EntitySnapshot = &snapshot.entities[0];
    assert_eq!(agent.force, Some(force("blue")));
    assert_eq!(
        agent.behavior.as_ref().map(cgf_core::BehaviorId::as_str),
        Some("test.behavior.idle")
    );
    assert_eq!(agent.status, Some(Status::Success));
    let plain = &snapshot.entities[1];
    assert_eq!(plain.force, None);
    assert!(plain.behavior.is_none());
    assert_eq!(plain.status, None);
}

/// ADR 0016: the entity without a behavior keeps the M1 stand-in; the snapshot shows its intents.
#[test]
#[ignore = "WI-6"]
fn given_entity_without_behavior_when_snapshot_then_standin_intents_are_visible() {
    let model = Stub::new("test.model.still", Motion::Stationary);
    let mut k = kernel(1);
    let mut init = owned(at(0.0), &model);
    init.route = Some(route(vec![at(100.0)], 10.0));
    k.spawn_with(init, AgentInit::default()).expect("spawn");

    k.tick().expect("tick");
    let intents = k.snapshot().entities[0].intents.clone();

    assert_eq!(intents.len(), 2);
    assert_eq!(intents[0], Intent::SetSpeed(MetersPerSecond(10.0)));
    assert!(matches!(intents[1], Intent::MoveTo { .. }));
}
