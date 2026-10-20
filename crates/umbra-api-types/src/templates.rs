//! Templates (REQ-X-5, REQ-T-3).

use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

/// A component and its merged parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ComponentRef {
    /// The component id.
    pub id: String,
    /// Its parameters: any JSON value.
    #[schema(value_type = Object)]
    pub params: Value,
}

/// A template in `GET /templates`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct TemplateSummary {
    /// The template id.
    pub id: String,
    /// Its description (not inherited).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The motion model id.
    pub motion: String,
    /// The behavior id, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub behavior: Option<String>,
    /// The defining file, relative to the scenario root.
    pub source: String,
}

/// A template in `GET /templates/{id}`, inheritance applied.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct TemplateDetail {
    /// The template id.
    pub id: String,
    /// The inheritance chain, the template first and the root last.
    pub chain: Vec<String>,
    /// Its description (not inherited).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The motion model with its merged parameters.
    pub motion: ComponentRef,
    /// The behavior with its merged parameters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub behavior: Option<ComponentRef>,
    /// The sensors in template order.
    pub sensors: Vec<ComponentRef>,
    /// The defining file, relative to the scenario root.
    pub source: String,
}
