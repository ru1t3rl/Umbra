//! The SDK error type (code plus message).

/// A plugin-side error: reported to the host through the error sink.
#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct Error {
    message: String,
}

impl Error {
    /// A general error.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    /// A bad-params error (`UMBRA_ERR_PARAMS` at the boundary).
    pub fn params(message: impl Into<String>) -> Self {
        Self::new(message)
    }
}
