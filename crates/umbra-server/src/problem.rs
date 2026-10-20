//! Problem details for every non-2xx response (REQ-X-9, ADR 0026). The code table lives in
//! `umbra-api-types`; this module turns failures into [`Problem`] bodies.

#![expect(
    clippy::todo,
    unused_variables,
    reason = "WI-6 signature stubs; the bodies arrive in WI-11"
)]

use axum::response::{IntoResponse, Response};
use umbra_api_types::{Diagnostic, Problem, ProblemCode};
use umbra_engine::handle::HandleError;

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

    /// The problem for a failed engine command (WI-11): the mapping of ADR 0026, one code per
    /// `HandleError` and `SessionError` case.
    #[must_use]
    pub fn from_handle_error(error: &HandleError) -> Self {
        todo!("WI-11")
    }
}

impl IntoResponse for ApiError {
    /// `application/problem+json` with the code's status (WI-11).
    fn into_response(self) -> Response {
        todo!("WI-11")
    }
}
