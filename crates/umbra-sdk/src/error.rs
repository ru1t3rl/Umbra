//! The SDK error type (code plus message).

use crate::abi::{UMBRA_ERR_INTERNAL, UMBRA_ERR_PARAMS, UmbraCode};

/// A plugin-side error: reported to the host through the error sink, with its code as the return
/// code of the failed export.
#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct Error {
    code: UmbraCode,
    message: String,
}

impl Error {
    /// A general error (`UMBRA_ERR_INTERNAL` at the boundary).
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            code: UMBRA_ERR_INTERNAL,
            message: message.into(),
        }
    }

    /// A bad-params error (`UMBRA_ERR_PARAMS` at the boundary).
    pub fn params(message: impl Into<String>) -> Self {
        Self {
            code: UMBRA_ERR_PARAMS,
            message: message.into(),
        }
    }

    /// The `UMBRA_ERR_*` code the export returns for this error.
    #[must_use]
    pub fn code(&self) -> UmbraCode {
        self.code
    }
}

impl Error {
    /// An error with an explicit boundary code (host refusals, malformed input).
    pub(crate) fn with_code(code: UmbraCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    /// A malformed-input error (`UMBRA_ERR_INVALID_ARGUMENT` at the boundary, ADR 0019).
    pub(crate) fn invalid(message: impl Into<String>) -> Self {
        Self::with_code(crate::abi::UMBRA_ERR_INVALID_ARGUMENT, message)
    }

    /// The message written to the host's error sink.
    pub(crate) fn message(&self) -> &str {
        &self.message
    }
}
