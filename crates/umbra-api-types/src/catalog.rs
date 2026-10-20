//! The behavior catalog (REQ-X-10, REQ-B-13, REQ-B-17).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

use crate::components::Provenance;

/// What a catalog entry describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CatalogKind {
    /// A behavior node type.
    NodeType,
    /// A condition leaf.
    Condition,
    /// An action leaf.
    Action,
    /// A behavior graph or subgraph.
    Behavior,
}

/// One catalog row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct CatalogEntry {
    /// What it describes.
    pub kind: CatalogKind,
    /// The node type name or component id.
    pub id: String,
    /// A description.
    pub description: String,
    /// Where it came from.
    pub provenance: Provenance,
    /// Its semantic version.
    pub version: String,
    /// Whether editors must not change it (built-in and plugin entries).
    pub read_only: bool,
    /// The JSON Schema of its parameters.
    #[schema(value_type = Object)]
    pub params_schema: Value,
    /// Behaviors: the type of every interface parameter.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub param_types: BTreeMap<String, String>,
    /// Behaviors: the output ports and their types.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub outputs: BTreeMap<String, String>,
    /// Free-text result semantics.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
}

/// `GET /catalog/behavior`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct BehaviorCatalog {
    /// The entries, sorted by kind then id.
    pub entries: Vec<CatalogEntry>,
}
