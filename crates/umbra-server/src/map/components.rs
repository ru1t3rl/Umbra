//! Registered components: engine entries to wire entries, and the `kind` filter (WI-13).

use umbra_api_types as api;
use umbra_engine::handle::ComponentEntry;
use umbra_engine::{ComponentKind, Provenance};

use crate::extract::ApiQuery;
use crate::problem::ApiError;

/// The wire kind for an engine kind; `None` for a kind added to the engine that the wire does not
/// know yet (the enum is non-exhaustive).
fn wire_kind(kind: ComponentKind) -> Option<api::ComponentKind> {
    Some(match kind {
        ComponentKind::MotionModel => api::ComponentKind::MotionModel,
        ComponentKind::Terrain => api::ComponentKind::Terrain,
        ComponentKind::Behavior => api::ComponentKind::Behavior,
        ComponentKind::Sensor => api::ComponentKind::Sensor,
        ComponentKind::Condition => api::ComponentKind::Condition,
        ComponentKind::Action => api::ComponentKind::Action,
        _ => return None,
    })
}

/// The `kind` filter of `GET /components`.
///
/// # Errors
/// `400 bad_request` if `kind` is repeated or is not a component kind.
pub fn component_kind_query(query: &ApiQuery) -> Result<Option<ComponentKind>, ApiError> {
    query
        .text("kind")?
        .map(|kind| {
            serde_json::from_value::<api::ComponentKind>(serde_json::Value::from(kind))
                .map(component_kind_filter)
                .map_err(|_| {
                    ApiError::new(
                        api::ProblemCode::BadRequest,
                        format!("`kind={kind}` is not a component kind"),
                    )
                })
        })
        .transpose()
}

/// The engine kind for the `kind` filter.
#[must_use]
pub fn component_kind_filter(kind: api::ComponentKind) -> ComponentKind {
    match kind {
        api::ComponentKind::MotionModel => ComponentKind::MotionModel,
        api::ComponentKind::Terrain => ComponentKind::Terrain,
        api::ComponentKind::Behavior => ComponentKind::Behavior,
        api::ComponentKind::Sensor => ComponentKind::Sensor,
        api::ComponentKind::Condition => ComponentKind::Condition,
        api::ComponentKind::Action => ComponentKind::Action,
    }
}

/// The wire entry of a component; `None` for a kind the wire does not know.
#[must_use]
pub fn component_entry(entry: &ComponentEntry) -> Option<api::ComponentEntry> {
    Some(api::ComponentEntry {
        kind: wire_kind(entry.kind)?,
        id: entry.id.as_str().to_owned(),
        version: entry.version.to_string(),
        description: entry.description.clone(),
        provenance: match entry.provenance {
            Provenance::Builtin => api::Provenance::Builtin,
            Provenance::File => api::Provenance::File,
            Provenance::Plugin => api::Provenance::Plugin,
        },
        params_schema: entry.params_schema.as_json().clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_engine_kind_has_a_wire_kind() {
        for kind in ComponentKind::ALL {
            assert!(wire_kind(*kind).is_some(), "{kind:?} has no wire kind");
        }
    }

    #[test]
    fn the_filter_round_trips_every_wire_kind() {
        for kind in ComponentKind::ALL {
            let wire = wire_kind(*kind).expect("mapped");
            assert_eq!(component_kind_filter(wire), *kind);
        }
    }

    #[test]
    fn the_kind_query_parses_or_is_400() {
        let none = component_kind_query(&ApiQuery::parse("")).expect("none");
        assert_eq!(none, None);
        let motion = component_kind_query(&ApiQuery::parse("kind=motion_model")).expect("kind");
        assert_eq!(motion, Some(ComponentKind::MotionModel));
        let bad = component_kind_query(&ApiQuery::parse("kind=gadget")).expect_err("bad");
        assert_eq!(bad.code(), api::ProblemCode::BadRequest);
    }
}
