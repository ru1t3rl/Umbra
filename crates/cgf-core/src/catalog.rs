//! Catalog types: provenance, component metadata and catalog entries (REQ-B-13, REQ-B-17,
//! ADR 0011).
//!
//! The catalog is the single data source for editors (REQ-X-10). This module holds the types;
//! `Catalog::from_registry` is added with the registry kinds.

use std::collections::BTreeMap;

use crate::behavior::BbType;
use crate::graph::{Version, VersionReq};
use crate::id::ComponentId;
use crate::params::ParamSchema;

/// Where a graph or component came from (REQ-B-13).
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
pub enum Access {
    /// The leaf reads the key.
    Read,
    /// The leaf writes the key.
    Write,
}

/// A leaf parameter that names a blackboard key (for example `store_as`, `target_key`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlackboardParam {
    /// Parameter name in the leaf's schema.
    pub param: String,
    /// Type the key must have.
    pub ty: BbType,
    /// Whether the leaf reads or writes the key.
    pub access: Access,
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
    /// Behaviors: output ports by name.
    pub outputs: BTreeMap<String, BbType>,
    /// Behaviors: referenced subgraphs in node order (cycle and depth checks).
    pub subgraphs: Vec<(ComponentId, Option<VersionReq>)>,
    /// Behaviors: the output port that carries the result, if any.
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

    /// Sets the result port.
    #[must_use]
    pub fn with_result(mut self, port: impl Into<String>) -> Self {
        self.result = Some(port.into());
        self
    }
}

/// What a catalog entry describes. The derived order is the catalog's primary sort key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
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
    /// Behaviors: output ports.
    pub outputs: BTreeMap<String, BbType>,
    /// Whether editors must not change it ([`Provenance::read_only`]).
    pub read_only: bool,
}

/// Node types, conditions, actions and behaviors with schemas, versions and provenance.
///
/// Entries are sorted by (kind, id), so iteration order is deterministic.
#[derive(Debug, Clone, PartialEq)]
pub struct Catalog {
    entries: Vec<CatalogEntry>,
}

impl Catalog {
    /// Builds a catalog from entries in any order; they are sorted by (kind, id) (stable, so
    /// equal keys keep their input order).
    pub fn from_entries(mut entries: Vec<CatalogEntry>) -> Self {
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
