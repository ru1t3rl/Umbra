//! Events: engine events to wire events for the stream (WI-15).

use serde_json::{Value, json};
use umbra_api_types as api;
use umbra_engine::{ControlAction, EntityId, SimEvent};

/// The wire event for `event`, which happened in `tick` (the engine's events carry no tick, so the
/// bus pairs each with one; ASM-205).
///
/// `detail` of a `custom` event is `{"kind": <component id>, "payload": <the payload>}`; of a
/// `simulation_control` event `{"action": "start" | "pause" | "resume" | "stop" | "step"}` with
/// `"count"` for `step`. An event kind added to the engine later has no ids or detail until it is mapped.
#[must_use]
pub fn event_dto(event: &SimEvent, tick: u64) -> api::EventDto {
    let mut dto = api::EventDto {
        tick,
        kind: event.kind_name().to_owned(),
        entity_id: None,
        target_id: None,
        template: None,
        weapon: None,
        detail: None,
    };
    match event {
        SimEvent::EntityCreated { entity, template } => {
            dto.entity_id = Some(entity.get());
            dto.template = template.as_ref().map(|t| t.as_str().to_owned());
        }
        SimEvent::EntityRemoved { entity } => dto.entity_id = Some(entity.get()),
        SimEvent::Fire {
            shooter,
            target,
            weapon,
        } => {
            dto.entity_id = Some(shooter.get());
            dto.target_id = Some(target.get());
            dto.weapon = Some(weapon.as_str().to_owned());
        }
        SimEvent::Custom {
            source,
            kind,
            payload,
        } => {
            dto.entity_id = source.map(EntityId::get);
            dto.detail = Some(json!({"kind": kind.as_str(), "payload": payload}));
        }
        SimEvent::SimulationControl(action) => dto.detail = Some(control_detail(*action)),
        // An event kind added later (the enum is non-exhaustive) until it is mapped.
        _ => {}
    }
    dto
}

fn control_detail(action: ControlAction) -> Value {
    match action {
        ControlAction::Start => json!({"action": "start"}),
        ControlAction::Pause => json!({"action": "pause"}),
        ControlAction::Resume => json!({"action": "resume"}),
        ControlAction::Stop => json!({"action": "stop"}),
        ControlAction::Step(count) => json!({"action": "step", "count": count}),
        // An action added later (the enum is non-exhaustive) until it is mapped.
        _ => json!({"action": "unknown"}),
    }
}

/// The entities an event concerns, for the stream's entity and bounding-box filters.
#[must_use]
pub fn event_entities(event: &SimEvent) -> Vec<EntityId> {
    match event {
        SimEvent::EntityCreated { entity, .. } | SimEvent::EntityRemoved { entity } => {
            vec![*entity]
        }
        SimEvent::Fire {
            shooter, target, ..
        } => vec![*shooter, *target],
        SimEvent::Custom { source, .. } => source.iter().copied().collect(),
        _ => Vec::new(),
    }
}
