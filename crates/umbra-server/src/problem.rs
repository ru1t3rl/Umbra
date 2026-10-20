//! Problem details for every non-2xx response (REQ-X-9, ADR 0026). The code table lives in
//! `umbra-api-types`; this module turns failures into [`Problem`] bodies.

use axum::http::header::CONTENT_TYPE;
use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use umbra_api_types::{Diagnostic, Problem, ProblemCode};
use umbra_engine::handle::HandleError;
use umbra_engine::{ConfineError, Confinement, EngineError, ScenarioError, SessionError};

use crate::map;

/// The media type of every error body.
pub const PROBLEM_JSON: &str = "application/problem+json";

/// A failure that becomes an `application/problem+json` response.
#[derive(Debug, Clone, PartialEq)]
pub struct ApiError {
    code: ProblemCode,
    detail: String,
    diagnostics: Vec<Diagnostic>,
}

impl ApiError {
    /// An error with `code` and a detail for a person.
    #[must_use]
    pub fn new(code: ProblemCode, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
            diagnostics: Vec::new(),
        }
    }

    /// Adds located diagnostics (`scenario_invalid`).
    #[must_use]
    pub fn with_diagnostics(mut self, diagnostics: Vec<Diagnostic>) -> Self {
        self.diagnostics = diagnostics;
        self
    }

    /// The code.
    #[must_use]
    pub fn code(&self) -> ProblemCode {
        self.code
    }

    /// The HTTP status: the one status of the code.
    #[must_use]
    pub fn status(&self) -> u16 {
        self.code.status()
    }

    /// The response body.
    #[must_use]
    pub fn into_problem(self) -> Problem {
        let problem = Problem::new(self.code, self.detail);
        if self.diagnostics.is_empty() {
            problem
        } else {
            problem.with_diagnostics(self.diagnostics)
        }
    }

    /// The problem for a failed engine command: one code per `HandleError` and `SessionError` case
    /// (ADR 0026).
    ///
    /// The detail of a load failure that comes from a library error (a plugin, the loader) is the
    /// code's title only, because such a message can quote an absolute path (ASM-194). Use
    /// [`ApiError::from_handle_error_in`] to include the message with the paths scrubbed.
    #[must_use]
    pub fn from_handle_error(error: &HandleError) -> Self {
        Self::map_handle_error(error, None)
    }

    /// As [`ApiError::from_handle_error`], with the message of a library error included after the
    /// paths in it have been written in display form by `confinement` (ASM-194, ASM-197).
    #[must_use]
    pub fn from_handle_error_in(error: &HandleError, confinement: &Confinement) -> Self {
        Self::map_handle_error(error, Some(confinement))
    }

    fn map_handle_error(error: &HandleError, scrub: Option<&Confinement>) -> Self {
        // The text of a library error: scrubbed, or only the title when there is no confinement.
        let library = |code: ProblemCode, text: &dyn std::fmt::Display| {
            let detail = match scrub {
                Some(confinement) => confinement.scrub(&text.to_string()),
                None => code.title().to_owned(),
            };
            Self::new(code, detail)
        };
        match error {
            HandleError::Busy => Self::new(ProblemCode::EngineBusy, error.to_string()),
            HandleError::Closed => Self::new(ProblemCode::EngineUnavailable, error.to_string()),
            HandleError::NoSession => Self::new(ProblemCode::NoScenario, error.to_string()),
            HandleError::InvalidState { state, command } => Self::new(
                ProblemCode::InvalidState,
                format!(
                    "{command} is not valid while the run is {}",
                    map::run_state_name(*state)
                ),
            ),
            HandleError::NotStepped => Self::new(ProblemCode::NotStepped, error.to_string()),
            HandleError::StepInProgress => {
                Self::new(ProblemCode::StepInProgress, error.to_string())
            }
            HandleError::StepCancelled { reached } => Self::new(
                ProblemCode::StepCancelled,
                format!("the step was cancelled at tick {}", reached.tick()),
            ),
            HandleError::Session(session) => Self::map_session_error(session, &library),
            HandleError::Engine(engine) => library(ProblemCode::Internal, engine),
            // `HandleError` is non-exhaustive: a future case is an internal error until mapped.
            _ => Self::new(ProblemCode::Internal, ProblemCode::Internal.title()),
        }
    }

    fn map_session_error(
        error: &SessionError,
        library: &dyn Fn(ProblemCode, &dyn std::fmt::Display) -> Self,
    ) -> Self {
        match error {
            SessionError::Scenario(ScenarioError::Invalid(diagnostics)) => Self::new(
                ProblemCode::ScenarioInvalid,
                format!("the scenario has {} error(s)", diagnostics.len()),
            )
            .with_diagnostics(diagnostics.iter().map(map::diagnostic).collect()),
            SessionError::Scenario(other) => library(ProblemCode::ScenarioLoadFailed, other),
            SessionError::Confine(ConfineError::NotAFile { shown }) => Self::new(
                ProblemCode::ValidationFailed,
                format!("not a scenario file: {shown}"),
            ),
            // `shown` is in display form already (ConfineError doc).
            SessionError::Confine(denied) => {
                Self::new(ProblemCode::PathNotAllowed, denied.to_string())
            }
            SessionError::Engine(EngineError::InvalidSpawn(_)) => {
                library(ProblemCode::InvalidSpawn, error)
            }
            SessionError::Engine(engine) => library(ProblemCode::Internal, engine),
            SessionError::TemplateNotFound(_) => {
                Self::new(ProblemCode::TemplateNotFound, error.to_string())
            }
            SessionError::DuplicateName(_) => {
                Self::new(ProblemCode::DuplicateName, error.to_string())
            }
            SessionError::EntityNotFound(_) => {
                Self::new(ProblemCode::EntityNotFound, error.to_string())
            }
            // `SessionError` is non-exhaustive.
            _ => library(ProblemCode::Internal, error),
        }
    }
}

impl IntoResponse for ApiError {
    /// `application/problem+json` with the status of the code.
    fn into_response(self) -> Response {
        let status =
            StatusCode::from_u16(self.status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let mut response = (status, axum::Json(self.into_problem())).into_response();
        response
            .headers_mut()
            .insert(CONTENT_TYPE, HeaderValue::from_static(PROBLEM_JSON));
        response
    }
}
