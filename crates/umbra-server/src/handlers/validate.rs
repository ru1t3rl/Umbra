//! `POST /api/v1/behaviors/validate` (REQ-X-11).

use std::path::{Component, Path};
use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use umbra_api_types::{ProblemCode, ValidateGraphRequest, ValidationResult};
use umbra_engine::handle::Command;

use crate::extract::ApiJson;
use crate::map;
use crate::openapi::CommonProblems;
use crate::problem::ApiError;
use crate::state::AppState;

/// The name diagnostics use for a document that has none.
const DEFAULT_NAME: &str = "document";

/// Whether `name` is a plain relative name: no root, drive prefix or `..` component. Any other name
/// could be made to equal the on-disk path of a session graph (D-b).
fn is_plain_name(name: &str) -> bool {
    Path::new(name)
        .components()
        .all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
}

/// Validates a behavior graph document without changing any state. A `name` that is absolute or
/// contains `..` is refused with `422 validation_failed`. Inside a loaded scenario the
/// document sees the session's plugins and graphs; otherwise only the engine's own. The
/// diagnostics are the result: an invalid document answers 200 with `valid: false`.
#[utoipa::path(
    post,
    path = "/api/v1/behaviors/validate",
    operation_id = "validateBehavior",
    tag = "behaviors",
    request_body = ValidateGraphRequest,
    responses(
        (status = 200, description = "The diagnostics of the document", body = ValidationResult),
        (status = 400, description = "invalid_body", body = umbra_api_types::Problem, content_type = "application/problem+json"),
        (status = 413, description = "payload_too_large", body = umbra_api_types::Problem, content_type = "application/problem+json"),
        (status = 415, description = "unsupported_media_type", body = umbra_api_types::Problem, content_type = "application/problem+json"),
        (status = 422, description = "validation_failed (also: `name` is absolute or contains `..`)", body = umbra_api_types::Problem, content_type = "application/problem+json"),
        CommonProblems,
    ),
)]
pub(crate) async fn validate_behavior(
    State(state): State<Arc<AppState>>,
    ApiJson(request): ApiJson<ValidateGraphRequest>,
) -> Result<Json<ValidationResult>, ApiError> {
    let name = request.name.unwrap_or_else(|| DEFAULT_NAME.to_owned());
    if !is_plain_name(&name) {
        // The name is not echoed: it may be a guess at the scenario root.
        return Err(ApiError::new(
            ProblemCode::ValidationFailed,
            "`name` must be a relative file name without `..` components",
        ));
    }
    let document = request.document;
    let validation = state
        .call(|reply| Command::ValidateGraph {
            name,
            document,
            reply,
        })
        .await?;
    Ok(Json(map::validation_result(&validation)))
}

#[cfg(test)]
mod tests {
    use super::is_plain_name;

    #[test]
    fn only_a_plain_relative_name_passes() {
        for ok in ["document", "a.behavior.toml", "graphs/a.toml", "./a.toml"] {
            assert!(is_plain_name(ok), "{ok}");
        }
        for bad in ["/srv/a.toml", "../a.toml", "a/../b.toml", "/"] {
            assert!(!is_plain_name(bad), "{bad}");
        }
    }
}
