//! The behavior catalog: engine entries to wire entries (WI-14, REQ-X-10, REQ-B-13, REQ-B-17).

use umbra_api_types as api;
use umbra_engine::{Catalog, CatalogEntry, CatalogKind, Provenance};

use super::components::wire_provenance;

/// The wire kind; `None` for a kind added to the engine that the wire does not know yet (the enum
/// is non-exhaustive).
fn wire_kind(kind: CatalogKind) -> Option<api::CatalogKind> {
    Some(match kind {
        CatalogKind::NodeType => api::CatalogKind::NodeType,
        CatalogKind::Condition => api::CatalogKind::Condition,
        CatalogKind::Action => api::CatalogKind::Action,
        CatalogKind::Behavior => api::CatalogKind::Behavior,
        _ => return None,
    })
}

/// The wire entry; `None` for a kind the wire does not know. Built-in and plugin entries are
/// read-only; a graph loaded from a file is not.
#[must_use]
pub fn catalog_entry(entry: &CatalogEntry) -> Option<api::CatalogEntry> {
    Some(api::CatalogEntry {
        kind: wire_kind(entry.kind)?,
        id: entry.id.clone(),
        description: entry.description.clone(),
        provenance: wire_provenance(entry.source),
        version: entry.version.to_string(),
        read_only: !matches!(entry.source, Provenance::File),
        params_schema: entry.params.as_json().clone(),
        param_types: entry
            .param_types
            .iter()
            .map(|(name, ty)| (name.clone(), ty.to_string()))
            .collect(),
        outputs: entry
            .outputs
            .iter()
            .map(|(name, ty)| (name.clone(), ty.to_string()))
            .collect(),
        result: entry.result.clone(),
    })
}

/// The wire catalog, in the engine's order (kind, then id).
#[must_use]
pub fn behavior_catalog(catalog: &Catalog) -> api::BehaviorCatalog {
    api::BehaviorCatalog {
        entries: catalog.entries().iter().filter_map(catalog_entry).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use umbra_engine::builtins::standard_registry;

    #[test]
    fn builtin_entries_are_read_only_and_every_engine_kind_is_mapped() {
        let registry = standard_registry().expect("registry");
        let catalog = Catalog::from_registry(&registry);
        let wire = behavior_catalog(&catalog);
        assert_eq!(wire.entries.len(), catalog.entries().len(), "none dropped");
        assert!(wire.entries.iter().all(|e| e.read_only));
        assert!(
            wire.entries
                .iter()
                .all(|e| e.provenance == api::Provenance::Builtin)
        );
    }

    #[test]
    fn a_file_entry_is_editable() {
        let registry = standard_registry().expect("registry");
        let mut entry = Catalog::from_registry(&registry).entries()[0].clone();
        entry.source = Provenance::File;
        let wire = catalog_entry(&entry).expect("mapped");
        assert!(!wire.read_only);
        assert_eq!(wire.provenance, api::Provenance::File);
    }
}
