//! Registered components (REQ-X-5, REQ-X-10).

use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

/// The kind of a registered component.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ComponentKind {
    /// A motion model.
    MotionModel,
    /// A terrain provider.
    Terrain,
    /// A behavior.
    Behavior,
    /// A sensor.
    Sensor,
    /// A behavior-tree condition leaf.
    Condition,
    /// A behavior-tree action leaf.
    Action,
}

/// Where a component or catalog entry came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Provenance {
    /// Shipped with the engine.
    Builtin,
    /// Loaded from a user file.
    File,
    /// Provided by a plugin.
    Plugin,
}

/// A component in `GET /components`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ComponentEntry {
    /// The kind.
    pub kind: ComponentKind,
    /// The component id.
    pub id: String,
    /// The component's semantic version.
    pub version: String,
    /// A description.
    pub description: String,
    /// Where it came from.
    pub provenance: Provenance,
    /// The JSON Schema of its parameters.
    #[schema(value_type = Object)]
    pub params_schema: Value,
}
