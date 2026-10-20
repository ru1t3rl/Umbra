//! Problem details (REQ-X-9, ADR 0026): every non-2xx response is `application/problem+json` with a
//! stable code, and every code has exactly one HTTP status.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::validation::Diagnostic;

macro_rules! problem_codes {
    ($($variant:ident => ($text:literal, $status:literal, $title:literal)),+ $(,)?) => {
        /// A stable problem code. The list is closed per minor version; new codes are a minor bump.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
        // `rename_all` is for the generated schema: `ToSchema` does not read the macro-built
        // per-variant `rename`, and a test pins that every spelling agrees.
        #[serde(rename_all = "snake_case")]
        pub enum ProblemCode {
            $(
                #[doc = $title]
                #[serde(rename = $text)]
                $variant,
            )+
        }

        impl ProblemCode {
            /// Every code, in declaration order.
            pub const ALL: &'static [ProblemCode] = &[$(ProblemCode::$variant),+];

            /// The `snake_case` spelling used in `code` and in the `type` URN.
            #[must_use]
            pub const fn as_str(self) -> &'static str {
                match self { $(ProblemCode::$variant => $text),+ }
            }

            /// The HTTP status of this code (ADR 0026, amended).
            #[must_use]
            pub const fn status(self) -> u16 {
                match self { $(ProblemCode::$variant => $status),+ }
            }

            /// A short, stable title.
            #[must_use]
            pub const fn title(self) -> &'static str {
                match self { $(ProblemCode::$variant => $title),+ }
            }
        }
    };
}

problem_codes! {
    BadRequest => ("bad_request", 400, "A query or path parameter is invalid"),
    InvalidBody => ("invalid_body", 400, "The request body is not parseable JSON"),
    UnsupportedEncoding => ("unsupported_encoding", 400, "The stream encoding is not supported"),
    PathNotAllowed => ("path_not_allowed", 403, "The path is outside the allowed directories"),
    HostNotAllowed => ("host_not_allowed", 403, "The Host header is not allowed"),
    OriginNotAllowed => ("origin_not_allowed", 403, "The Origin header is not allowed"),
    NotFound => ("not_found", 404, "No such route"),
    EntityNotFound => ("entity_not_found", 404, "No such entity"),
    TemplateNotFound => ("template_not_found", 404, "No such template"),
    MethodNotAllowed => ("method_not_allowed", 405, "The method is not allowed on this route"),
    NoScenario => ("no_scenario", 409, "No scenario is loaded"),
    InvalidState => ("invalid_state", 409, "The command is not valid in the current run state"),
    NotStepped => ("not_stepped", 409, "The clock is not in stepped mode"),
    StepInProgress => ("step_in_progress", 409, "Another step is still pending"),
    StepCancelled => ("step_cancelled", 409, "The step was cancelled before it completed"),
    DuplicateName => ("duplicate_name", 409, "An entity with this name exists"),
    PayloadTooLarge => ("payload_too_large", 413, "The request body is too large"),
    UnsupportedMediaType => ("unsupported_media_type", 415, "The request needs Content-Type application/json"),
    ValidationFailed => ("validation_failed", 422, "The request is well formed but invalid"),
    InvalidId => ("invalid_id", 422, "An id is not valid"),
    InvalidSpawn => ("invalid_spawn", 422, "The spawn parameters are invalid"),
    ScenarioInvalid => ("scenario_invalid", 422, "The scenario has errors"),
    ScenarioLoadFailed => ("scenario_load_failed", 422, "The scenario or its plugins could not be loaded"),
    Internal => ("internal", 500, "Internal error"),
    EngineBusy => ("engine_busy", 503, "The engine command queue is full"),
    EngineUnavailable => ("engine_unavailable", 503, "The engine is not running"),
    TooManyConnections => ("too_many_connections", 503, "Too many stream connections"),
}

/// A problem-details body (`application/problem+json`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Problem {
    /// `urn:umbra:problem:<code>`.
    pub r#type: String,
    /// The code's short title.
    pub title: String,
    /// The HTTP status; the one status of `code`.
    pub status: u16,
    /// What went wrong, for a person.
    pub detail: String,
    /// The stable code.
    pub code: ProblemCode,
    /// Located diagnostics, for `scenario_invalid` and similar.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostics: Option<Vec<Diagnostic>>,
}

impl Problem {
    /// A problem for `code` with the code's type, title and status.
    #[must_use]
    pub fn new(code: ProblemCode, detail: impl Into<String>) -> Self {
        Self {
            r#type: format!("urn:umbra:problem:{}", code.as_str()),
            title: code.title().to_string(),
            status: code.status(),
            detail: detail.into(),
            code,
            diagnostics: None,
        }
    }

    /// Adds located diagnostics.
    #[must_use]
    pub fn with_diagnostics(mut self, diagnostics: Vec<Diagnostic>) -> Self {
        self.diagnostics = Some(diagnostics);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use utoipa::OpenApi;

    #[test]
    fn serde_and_schema_spell_every_code_as_its_text() {
        let document = serde_json::to_value(crate::ApiSchemas::openapi()).expect("document");
        let listed: Vec<&str> = document["components"]["schemas"]["ProblemCode"]["enum"]
            .as_array()
            .expect("the schema lists the codes")
            .iter()
            .map(|v| v.as_str().expect("text"))
            .collect();
        let texts: Vec<&str> = ProblemCode::ALL.iter().map(|c| c.as_str()).collect();
        assert_eq!(listed, texts, "the schema spells the codes as they travel");
        for code in ProblemCode::ALL {
            assert_eq!(
                serde_json::to_value(code).expect("code"),
                serde_json::Value::from(code.as_str())
            );
        }
    }
}
