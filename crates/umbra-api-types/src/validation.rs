//! Behavior graph validation (REQ-X-11, ADR 0026).

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// `POST /behaviors/validate`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ValidateGraphRequest {
    /// The name diagnostics use for the document; default `document`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The graph file text.
    pub document: String,
}

/// A secondary location of a diagnostic.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct DiagnosticNote {
    /// The note text.
    pub message: String,
    /// The file, relative to the scenario root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// 1-based line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    /// 1-based column.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column: Option<u32>,
}

/// How serious a diagnostic is (added in API 1.1.0).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// The input is rejected.
    #[default]
    Error,
    /// The input is accepted; the author may want to look.
    Warning,
}

/// A located error or warning from the loader or validator.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Diagnostic {
    /// `error` or `warning`; a client that does not know the field reads `error` (added in API 1.1.0).
    #[serde(default)]
    pub severity: Severity,
    /// The stable diagnostic code.
    pub code: String,
    /// The message.
    pub message: String,
    /// The file, relative to the scenario root, or the document name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// 1-based line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    /// 1-based column.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column: Option<u32>,
    /// Extra locations.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<DiagnosticNote>,
}

/// The result of validation: the diagnostics are the result, not an error.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ValidationResult {
    /// Whether there are no diagnostics.
    pub valid: bool,
    /// Every diagnostic, sorted.
    pub diagnostics: Vec<Diagnostic>,
}
