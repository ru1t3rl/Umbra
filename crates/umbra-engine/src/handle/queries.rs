//! The read-only queries (REQ-X-5, REQ-X-11, ADR 0026): entities, templates, components, catalog,
//! plugins and graph validation. Paths in answers are in display form ([`super::shown`]).

use std::collections::BTreeSet;

use umbra_core::{Catalog, ComponentKind, EntityId, Ownership, Provenance, Version};
use umbra_kernel::EntitySnapshot;
use umbra_templates::ResolvedTemplate;

use super::thread::{Run, Worker};
use super::{
    ComponentEntry, EntityPage, EntityQuery, EntityState, HandleError, OwnershipFilter,
    PluginReport,
};
use crate::bus::StopReason;
use crate::scenario::EntityInfo;
use crate::session::SessionError;
use crate::validate::{GraphValidation, validate_graph_document};

/// The version the engine reports for built-in components that carry none of their own.
const ENGINE_VERSION: Version = Version::new(
    decimal(env!("CARGO_PKG_VERSION_MAJOR")),
    decimal(env!("CARGO_PKG_VERSION_MINOR")),
    decimal(env!("CARGO_PKG_VERSION_PATCH")),
);

const fn decimal(digits: &str) -> u64 {
    let bytes = digits.as_bytes();
    let mut n = 0u64;
    let mut i = 0;
    while i < bytes.len() {
        assert!(
            bytes[i].is_ascii_digit(),
            "version component is not a number"
        );
        n = n * 10 + (bytes[i] - b'0') as u64;
        i += 1;
    }
    n
}

impl Run {
    /// The directory entry and the last-tick state of `id`.
    pub(super) fn entity_state(&self, id: EntityId) -> Result<EntityState, HandleError> {
        let info = self
            .session
            .entities()
            .get(&id)
            .ok_or(SessionError::EntityNotFound(id))?;
        let snapshot = self
            .session
            .engine()
            .snapshot()
            .entities
            .into_iter()
            .find(|e| e.id == id)
            .ok_or(SessionError::EntityNotFound(id))?;
        Ok(EntityState {
            info: info.clone(),
            snapshot,
        })
    }
}

/// Whether the entity passes every filter of the query (the paging cursor is not a filter).
fn matches(
    query: &EntityQuery,
    ids: &BTreeSet<EntityId>,
    info: &EntityInfo,
    snapshot: &EntitySnapshot,
) -> bool {
    let wanted = |filter: &Option<String>, value: Option<&str>| {
        filter.as_deref().is_none_or(|f| value == Some(f))
    };
    let in_box = query.bbox.is_none_or(|b| {
        let g = snapshot.geodetic();
        b.contains(g.lat().to_degrees(), g.lon().to_degrees())
    });
    let owner = query.ownership.is_none_or(|o| match o {
        OwnershipFilter::Owned => matches!(snapshot.ownership, Ownership::Owned),
        OwnershipFilter::Remote => matches!(snapshot.ownership, Ownership::Remote { .. }),
    });
    (ids.is_empty() || ids.contains(&snapshot.id))
        && wanted(&query.name, info.name.as_deref())
        && wanted(&query.group, info.group.as_deref())
        && wanted(&query.template, Some(info.template.as_str()))
        && wanted(
            &query.force,
            snapshot.force.as_ref().map(umbra_core::Force::as_str),
        )
        && owner
        && in_box
}

impl Worker {
    fn run_ref(&self) -> Result<&Run, HandleError> {
        self.run.as_ref().ok_or(HandleError::NoSession)
    }

    /// One keyset page (ascending ids above `after_id`); `total` counts the matches of all pages.
    /// The limit is held to `1..=MAX_LIMIT`.
    pub(super) fn entities(&self, query: &EntityQuery) -> Result<EntityPage, HandleError> {
        let run = self.run_ref()?;
        let snapshot = run.session.engine().snapshot();
        let directory = run.session.entities();
        let ids: BTreeSet<EntityId> = query.ids.iter().copied().collect();
        let hits: Vec<EntityState> = snapshot
            .entities
            .into_iter()
            .filter_map(|s| Some((directory.get(&s.id)?.clone(), s)))
            .filter(|(info, s)| matches(query, &ids, info, s))
            .map(|(info, snapshot)| EntityState { info, snapshot })
            .collect();
        let total = hits.len();
        let limit = query.limit.clamp(1, EntityQuery::MAX_LIMIT);
        let mut rest = hits
            .into_iter()
            .filter(|e| query.after_id.is_none_or(|after| e.snapshot.id > after));
        let items: Vec<EntityState> = rest.by_ref().take(limit).collect();
        let next_after_id = rest.next().and(items.last()).map(|last| last.snapshot.id);
        Ok(EntityPage {
            tick: snapshot.time.tick(),
            total,
            items,
            next_after_id,
        })
    }

    pub(super) fn entity(&self, id: EntityId) -> Result<EntityState, HandleError> {
        self.run_ref()?.entity_state(id)
    }

    /// The scenario's templates in id order, their source files in display form.
    pub(super) fn templates(&self) -> Result<Vec<ResolvedTemplate>, HandleError> {
        let run = self.run_ref()?;
        Ok(run
            .session
            .scenario()
            .templates
            .iter()
            .map(|t| {
                let mut shown = t.clone();
                shown.source = self.shown_path(&t.source).into();
                shown
            })
            .collect())
    }

    /// The registered components (the session's, or the base registry's while idle), optionally of
    /// one kind, by kind then id. Kinds without metadata (motion models, terrain, sensors) have an
    /// empty description; they are `Plugin` with the capability's version when a plugin of the
    /// session provides them, else `Builtin` with the engine's version.
    pub(super) fn components(&self, kind: Option<ComponentKind>) -> Vec<ComponentEntry> {
        let registry = self
            .run
            .as_ref()
            .map_or(&self.settings.base, |r| r.session.graph_registry());
        let capabilities: Vec<_> = self
            .run
            .iter()
            .flat_map(|r| r.session.plugins().plugins())
            .flat_map(|p| &p.capabilities)
            .collect();
        let mut entries = Vec::new();
        for &k in ComponentKind::ALL
            .iter()
            .filter(|&&k| kind.is_none_or(|w| w == k))
        {
            for id in registry.ids(k) {
                let Some(schema) = registry.param_schema(k, id) else {
                    continue;
                };
                let (version, description, provenance) = if let Some(info) = registry.info(id) {
                    (info.version.clone(), info.description.clone(), info.source)
                } else if let Some(c) = capabilities.iter().find(|c| c.kind == k && &c.id == id) {
                    (c.version.clone(), String::new(), Provenance::Plugin)
                } else {
                    (ENGINE_VERSION, String::new(), Provenance::Builtin)
                };
                entries.push(ComponentEntry {
                    kind: k,
                    id: id.clone(),
                    version,
                    description,
                    provenance,
                    params_schema: schema.clone(),
                });
            }
        }
        entries
    }

    /// The behavior catalog of the session (with its file graphs), or of the base registry.
    pub(super) fn catalog(&self) -> Catalog {
        Catalog::from_registry(
            self.run
                .as_ref()
                .map_or(&self.settings.base, |r| r.session.graph_registry()),
        )
    }

    /// The session's plugins (origins in display form) and the fault that stopped the run.
    pub(super) fn plugins(&self) -> PluginReport {
        let Some(run) = &self.run else {
            return PluginReport {
                plugins: Vec::new(),
                fault: None,
            };
        };
        let plugins = run
            .session
            .plugins()
            .plugins()
            .iter()
            .map(|p| {
                let mut shown = p.clone();
                shown.origin = self.shown_path(std::path::Path::new(&p.origin));
                shown
            })
            .collect();
        let fault = match &run.stop_reason {
            Some(StopReason::PluginFault(fault)) => Some(fault.clone()),
            _ => None,
        };
        PluginReport { plugins, fault }
    }

    /// REQ-X-11: inside the session (its plugin build scope and file graphs) when there is one,
    /// else against the base registry. Diagnostics are the result, not an error.
    pub(super) fn validate_graph(
        &self,
        name: &str,
        document: &str,
    ) -> Result<GraphValidation, HandleError> {
        let mut validation = match &self.run {
            Some(run) => run.session.validate_graph(name, document),
            None => validate_graph_document(&self.settings.base, name, document),
        }
        .map_err(SessionError::from)?;
        validation.diagnostics = validation
            .diagnostics
            .into_iter()
            .map(|d| self.shown_diagnostic_keeping(d, super::shown::plain_relative_name(name)))
            .collect();
        Ok(validation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handle::GeoBox;

    fn bbox(min_lon: f64, max_lon: f64) -> GeoBox {
        GeoBox {
            min_lat_deg: -10.0,
            min_lon_deg: min_lon,
            max_lat_deg: 10.0,
            max_lon_deg: max_lon,
        }
    }

    #[test]
    fn plain_box_includes_its_edges_and_excludes_outside() {
        let b = bbox(-20.0, 20.0);
        assert!(b.contains(0.0, 0.0));
        assert!(b.contains(-10.0, -20.0) && b.contains(10.0, 20.0));
        assert!(!b.contains(10.1, 0.0) && !b.contains(0.0, 20.1) && !b.contains(0.0, -20.1));
    }

    #[test]
    fn antimeridian_box_holds_both_sides_and_not_the_middle() {
        let b = bbox(170.0, -170.0);
        assert!(b.contains(0.0, 175.0));
        assert!(b.contains(0.0, -175.0));
        assert!(b.contains(0.0, 180.0));
        assert!(b.contains(0.0, 170.0) && b.contains(0.0, -170.0));
        assert!(!b.contains(0.0, 0.0));
        assert!(!b.contains(0.0, 169.9) && !b.contains(0.0, -169.9));
        assert!(!b.contains(11.0, 175.0), "latitude still applies");
    }

    #[test]
    fn a_minus_180_edge_does_not_hold_longitude_180() {
        // Longitudes are reported in (-180, 180], so write that edge as 180.
        let b = bbox(-180.0, -170.0);
        assert!(!b.contains(0.0, 180.0));
        assert!(b.contains(0.0, -180.0) && b.contains(0.0, -175.0));
        assert!(bbox(170.0, 180.0).contains(0.0, 180.0));
    }

    #[test]
    fn engine_version_is_the_package_version() {
        assert_eq!(ENGINE_VERSION.to_string(), env!("CARGO_PKG_VERSION"),);
    }
}
