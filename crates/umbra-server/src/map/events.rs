//! Events: engine events to wire events for the stream (WI-15).

use serde_json::{Value, json};
use umbra_api_types as api;
use umbra_engine::{ControlAction, EntityId, PositionDisplay, SimEvent};

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
        SimEvent::Detonation {
            shooter,
            target,
            position,
        } => {
            let place = PositionDisplay::from_ecef(*position);
            dto.entity_id = shooter.map(EntityId::get);
            dto.target_id = target.map(EntityId::get);
            dto.detail = Some(json!({"position": {
                "lat_deg": place.lat_deg,
                "lon_deg": place.lon_deg,
                "alt_m": place.height_m,
            }}));
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
        SimEvent::Detonation {
            shooter, target, ..
        } => shooter.iter().chain(target.iter()).copied().collect(),
        SimEvent::Custom { source, .. } => source.iter().copied().collect(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use umbra_engine::Geodetic;

    use super::*;

    fn id(n: u64) -> EntityId {
        EntityId::new(n).expect("entity id")
    }

    fn detonation(shooter: Option<u64>, target: Option<u64>) -> SimEvent {
        let place = Geodetic::from_degrees(52.25, -4.5, 120.0).expect("position");
        SimEvent::Detonation {
            shooter: shooter.map(id),
            target: target.map(id),
            position: place.to_ecef(),
        }
    }

    #[test]
    fn a_detonation_has_its_ids_and_the_geodetic_position() {
        let dto = event_dto(&detonation(Some(4), Some(9)), 77);

        assert_eq!((dto.kind.as_str(), dto.tick), ("detonation", 77));
        assert_eq!((dto.entity_id, dto.target_id), (Some(4), Some(9)));
        assert!(dto.template.is_none() && dto.weapon.is_none());
        let position = &dto.detail.as_ref().expect("detail")["position"];
        let near = |key: &str, want: f64, tolerance: f64| {
            let got = position[key]
                .as_f64()
                .unwrap_or_else(|| panic!("{key}: {position}"));
            assert!((got - want).abs() < tolerance, "{key}: {got} vs {want}");
        };
        near("lat_deg", 52.25, 1e-9);
        near("lon_deg", -4.5, 1e-9);
        near("alt_m", 120.0, 1e-3);
    }

    #[test]
    fn an_unmapped_shooter_and_target_are_left_out() {
        let dto = event_dto(&detonation(None, None), 1);
        assert_eq!((dto.entity_id, dto.target_id), (None, None));
        let json = serde_json::to_value(&dto).expect("json");
        assert!(json.get("entity_id").is_none() && json.get("target_id").is_none());
        assert!(json["detail"]["position"].is_object());
    }

    #[test]
    fn a_detonation_concerns_its_shooter_and_target() {
        assert_eq!(
            event_entities(&detonation(Some(4), Some(9))),
            [id(4), id(9)]
        );
        assert_eq!(event_entities(&detonation(None, Some(9))), [id(9)]);
        assert_eq!(
            event_entities(&detonation(None, None)),
            Vec::<EntityId>::new()
        );
    }
}
