//! Graph validation: the engine outcome to the wire result (WI-14, REQ-X-11).

use umbra_api_types as api;
use umbra_engine::GraphValidation;

use super::status::diagnostic;

/// The wire validation result: the engine's diagnostics in the engine's order. The diagnostics are
/// the result, not an error.
#[must_use]
pub fn validation_result(validation: &GraphValidation) -> api::ValidationResult {
    api::ValidationResult {
        valid: validation.valid,
        diagnostics: validation.diagnostics.iter().map(diagnostic).collect(),
    }
}
