//! AT-1 (core row): the canonical event model. Requirements: REQ-C-5, ADR 0028.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

use umbra_core::{
    ComponentId, ControlAction, EntityId, ParamValue, SimEvent, TemplateId, WeaponId,
};

fn entity(n: u64) -> EntityId {
    EntityId::new(n).expect("non-zero")
}

/// The kind names are the stream's `event_kinds` vocabulary (ADR 0026, ADR 0027): they never
/// change, one per variant, snake case, whatever the payload.
#[test]
fn kind_names_are_stable() {
    let created = SimEvent::EntityCreated {
        entity: entity(1),
        template: Some(TemplateId::new("example.tank.blue").expect("valid id")),
    };
    let removed = SimEvent::EntityRemoved { entity: entity(1) };
    let fire = SimEvent::Fire {
        shooter: entity(1),
        target: entity(2),
        weapon: WeaponId::new("example.weapon.main_gun").expect("valid id"),
    };
    let custom = SimEvent::Custom {
        source: None,
        kind: ComponentId::new("example.custom.ping").expect("valid id"),
        payload: ParamValue::from(3),
    };
    let control = |a| SimEvent::SimulationControl(a);

    assert_eq!(created.kind_name(), "entity_created");
    assert_eq!(removed.kind_name(), "entity_removed");
    assert_eq!(fire.kind_name(), "fire");
    assert_eq!(custom.kind_name(), "custom");
    for action in [
        ControlAction::Start,
        ControlAction::Pause,
        ControlAction::Resume,
        ControlAction::Stop,
        ControlAction::Step(5),
    ] {
        assert_eq!(control(action).kind_name(), "simulation_control");
    }
}

/// The payload does not change the kind name (a `Custom` with a source is still `custom`).
#[test]
fn kind_name_ignores_the_payload() {
    let kind = ComponentId::new("example.custom.ping").expect("valid id");
    let with = SimEvent::Custom {
        source: Some(entity(7)),
        kind: kind.clone(),
        payload: ParamValue::from(1),
    };
    let without = SimEvent::Custom {
        source: None,
        kind,
        payload: ParamValue::from(2),
    };
    assert_eq!(with.kind_name(), without.kind_name());
    assert_ne!(with, without);
}
