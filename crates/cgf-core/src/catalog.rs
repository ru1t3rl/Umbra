//! Catalog types: provenance, component metadata and catalog entries (REQ-B-13, REQ-B-17,
//! ADR 0011).
//!
//! The catalog is the single data source for editors (REQ-X-10). This module holds the types;
//! `Catalog::from_registry` is added with the registry kinds.

use std::collections::BTreeMap;

use crate::behavior::BbType;
use crate::graph::{NODE_TYPES, Version, VersionReq};
use crate::id::ComponentId;
use crate::params::ParamSchema;
use crate::registry::{ComponentKind, Registry};

/// Where a graph or component came from (REQ-B-13). Closed: the spec names exactly these three.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Provenance {
    /// Shipped with the engine.
    Builtin,
    /// Loaded from a user file.
    File,
    /// Provided by a plugin.
    Plugin,
}

impl Provenance {
    /// Whether editors must not change it: `Builtin` and `Plugin` (REQ-B-13).
    pub const fn read_only(self) -> bool {
        matches!(self, Self::Builtin | Self::Plugin)
    }
}

/// How a leaf parameter that names a blackboard key uses the key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Access {
    /// The leaf reads the key.
    Read,
    /// The leaf writes the key.
    Write,
}

/// A leaf parameter that names a blackboard key (for example `store_as`, `target_key`).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct BlackboardParam {
    /// Parameter name in the leaf's schema.
    pub param: String,
    /// Type the key must have.
    pub ty: BbType,
    /// Whether the leaf reads or writes the key.
    pub access: Access,
}

impl BlackboardParam {
    /// A parameter `param` naming a key of type `ty`.
    pub fn new(param: impl Into<String>, ty: BbType, access: Access) -> Self {
        Self {
            param: param.into(),
            ty,
            access,
        }
    }
}

/// Metadata stored with a behavior, condition or action (ADR 0011).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct ComponentInfo {
    /// Human-readable description (catalog documentation).
    pub description: String,
    /// Where the component came from.
    pub source: Provenance,
    /// Semantic version of the component.
    pub version: Version,
    /// Leaves: which parameters name blackboard keys, in declaration order.
    pub blackboard_params: Vec<BlackboardParam>,
    /// Behaviors: the type of every interface param, binding-only ones included (ADR 0014).
    /// Filled by `register_graphs` from `Interface::types`; plugin behaviors declare it with the
    /// builder. Leaves: empty (their params are literal-only, checked by the param schema).
    pub param_types: BTreeMap<String, BbType>,
    /// Behaviors: output ports by name.
    pub outputs: BTreeMap<String, BbType>,
    /// Behaviors: referenced subgraphs in node order (cycle and depth checks).
    pub subgraphs: Vec<(ComponentId, Option<VersionReq>)>,
    /// Free-text result semantics (REQ-B-10): what Success, Failure and Running mean. Not a port
    /// name.
    pub result: Option<String>,
}

impl ComponentInfo {
    /// Metadata with no blackboard parameters, outputs, subgraphs or result.
    pub fn new(description: impl Into<String>, source: Provenance, version: Version) -> Self {
        Self {
            description: description.into(),
            source,
            version,
            blackboard_params: Vec::new(),
            param_types: BTreeMap::new(),
            outputs: BTreeMap::new(),
            subgraphs: Vec::new(),
            result: None,
        }
    }

    /// Adds a leaf parameter that names a blackboard key.
    #[must_use]
    pub fn with_blackboard_param(mut self, param: BlackboardParam) -> Self {
        self.blackboard_params.push(param);
        self
    }

    /// Declares the type of an interface param.
    #[must_use]
    pub fn with_param_type(mut self, param: impl Into<String>, ty: BbType) -> Self {
        self.param_types.insert(param.into(), ty);
        self
    }

    /// Adds (or replaces) an output port.
    #[must_use]
    pub fn with_output(mut self, port: impl Into<String>, ty: BbType) -> Self {
        self.outputs.insert(port.into(), ty);
        self
    }

    /// Adds a referenced subgraph.
    #[must_use]
    pub fn with_subgraph(mut self, id: ComponentId, version: Option<VersionReq>) -> Self {
        self.subgraphs.push((id, version));
        self
    }

    /// Sets the free-text result semantics.
    #[must_use]
    pub fn with_result(mut self, text: impl Into<String>) -> Self {
        self.result = Some(text.into());
        self
    }
}

/// What a catalog entry describes. The derived order is the catalog's primary sort key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub enum CatalogKind {
    /// A behavior node type (REQ-B-7).
    NodeType,
    /// A condition leaf.
    Condition,
    /// An action leaf.
    Action,
    /// A behavior graph or subgraph.
    Behavior,
}

/// One catalog row (REQ-B-17).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct CatalogEntry {
    /// What the entry describes.
    pub kind: CatalogKind,
    /// Node type name or component ID.
    pub id: String,
    /// Human-readable description.
    pub description: String,
    /// Where it came from.
    pub source: Provenance,
    /// Semantic version.
    pub version: Version,
    /// Parameter (or node settings) schema.
    pub params: ParamSchema,
    /// Behaviors: the type of every interface param, binding-only ones included.
    pub param_types: BTreeMap<String, BbType>,
    /// Behaviors: output ports.
    pub outputs: BTreeMap<String, BbType>,
    /// Free-text result semantics (REQ-B-10).
    pub result: Option<String>,
}

impl CatalogEntry {
    /// An entry with no param types, outputs or result.
    pub fn new(
        kind: CatalogKind,
        id: impl Into<String>,
        description: impl Into<String>,
        source: Provenance,
        version: Version,
        params: ParamSchema,
    ) -> Self {
        Self {
            kind,
            id: id.into(),
            description: description.into(),
            source,
            version,
            params,
            param_types: BTreeMap::new(),
            outputs: BTreeMap::new(),
            result: None,
        }
    }

    /// Sets the param types.
    #[must_use]
    pub fn with_param_types(mut self, t: BTreeMap<String, BbType>) -> Self {
        self.param_types = t;
        self
    }

    /// Sets the output ports.
    #[must_use]
    pub fn with_outputs(mut self, o: BTreeMap<String, BbType>) -> Self {
        self.outputs = o;
        self
    }

    /// Sets the free-text result semantics.
    #[must_use]
    pub fn with_result(mut self, text: impl Into<String>) -> Self {
        self.result = Some(text.into());
        self
    }

    fn with_result_opt(mut self, text: Option<String>) -> Self {
        self.result = text;
        self
    }

    /// Whether editors must not change it; derived from `source` (REQ-B-13), so no stored flag can
    /// disagree.
    pub fn read_only(&self) -> bool {
        self.source.read_only()
    }
}

/// Why a [`Catalog`] could not be built.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum CatalogError {
    /// Two entries share a (kind, id).
    #[error("duplicate catalog entry {kind:?} `{id}`")]
    Duplicate {
        /// Kind of the duplicate.
        kind: CatalogKind,
        /// ID of the duplicate.
        id: String,
    },
}

/// Node types, conditions, actions and behaviors with schemas, versions and provenance.
///
/// Entries are sorted by (kind, id) and unique per (kind, id), so iteration order is
/// deterministic.
#[derive(Debug, Clone, PartialEq)]
pub struct Catalog {
    entries: Vec<CatalogEntry>,
}

impl Catalog {
    /// Builds a catalog from entries in any order; they are sorted by (kind, id).
    ///
    /// # Errors
    /// [`CatalogError::Duplicate`] when two entries share a (kind, id). Equal ids of different
    /// kinds are allowed.
    pub fn from_entries(mut entries: Vec<CatalogEntry>) -> Result<Self, CatalogError> {
        entries.sort_by(|a, b| (a.kind, &a.id).cmp(&(b.kind, &b.id)));
        if let Some(w) = entries
            .windows(2)
            .find(|w| w[0].kind == w[1].kind && w[0].id == w[1].id)
        {
            return Err(CatalogError::Duplicate {
                kind: w[1].kind,
                id: w[1].id.clone(),
            });
        }
        Ok(Self { entries })
    }

    /// The catalog of a registry: the node types, then every registered condition, action and
    /// behavior. Motion models, terrain providers and sensors are not catalog material.
    ///
    /// Infallible: IDs are unique per kind in a registry and node type names are distinct, so no
    /// (kind, id) repeats. Node types carry [`Provenance::Builtin`] and the `cgf-core` package
    /// version.
    pub fn from_registry(registry: &Registry) -> Self {
        let mut entries = node_type_entries();
        for (kind, component) in [
            (CatalogKind::Condition, ComponentKind::Condition),
            (CatalogKind::Action, ComponentKind::Action),
            (CatalogKind::Behavior, ComponentKind::Behavior),
        ] {
            for id in registry.ids(component) {
                let Some(info) = registry.info(id) else {
                    continue;
                };
                let Some(params) = registry.param_schema(component, id) else {
                    continue;
                };
                entries.push(
                    CatalogEntry::new(
                        kind,
                        id.as_str(),
                        info.description.clone(),
                        info.source,
                        info.version.clone(),
                        params.clone(),
                    )
                    .with_param_types(info.param_types.clone())
                    .with_outputs(info.outputs.clone())
                    .with_result_opt(info.result.clone()),
                );
            }
        }
        entries.sort_by(|a, b| (a.kind, &a.id).cmp(&(b.kind, &b.id)));
        Self { entries }
    }

    /// All entries, sorted by (kind, id).
    pub fn entries(&self) -> &[CatalogEntry] {
        &self.entries
    }

    /// The entry of `kind` with `id`, if any.
    pub fn get(&self, kind: CatalogKind, id: &str) -> Option<&CatalogEntry> {
        self.entries
            .binary_search_by(|e| (e.kind, e.id.as_str()).cmp(&(kind, id)))
            .ok()
            .map(|i| &self.entries[i])
    }
}

/// One entry per node type. A schema that fails to parse would skip its entry; a `cgf-core`
/// test guarantees all of them parse.
fn node_type_entries() -> Vec<CatalogEntry> {
    let version =
        Version::parse(env!("CARGO_PKG_VERSION")).unwrap_or_else(|_| Version::new(0, 0, 0));
    NODE_TYPES
        .iter()
        .filter_map(|n| {
            let params = ParamSchema::from_json_str(n.settings_schema).ok()?;
            Some(CatalogEntry::new(
                CatalogKind::NodeType,
                n.name,
                n.description,
                Provenance::Builtin,
                version.clone(),
                params,
            ))
        })
        .collect()
}
