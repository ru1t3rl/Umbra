//! `POST`, `GET` and `DELETE /api/v1/scenario`.

use std::path::PathBuf;
use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use umbra_api_types::{LoadScenarioRequest, ProblemCode, ScenarioInfo};
use umbra_engine::handle::{Command, HandleError};

use crate::extract::ApiJson;
use crate::map;
use crate::openapi::CommonProblems;
use crate::problem::ApiError;
use crate::state::AppState;

/// Loads a scenario, replacing the session. Every failure leaves the server `idle`: the previous
/// session is gone (D-a).
#[utoipa::path(
    post,
    path = "/api/v1/scenario",
    operation_id = "loadScenario",
    tag = "scenario",
    request_body = LoadScenarioRequest,
    responses(
        (status = 201, description = "Loaded; the run is `ready`", body = ScenarioInfo),
        (status = 400, description = "invalid_body", body = umbra_api_types::Problem, content_type = "application/problem+json"),
        (status = 403, description = "path_not_allowed, host_not_allowed", body = umbra_api_types::Problem, content_type = "application/problem+json"),
        (status = 415, description = "unsupported_media_type", body = umbra_api_types::Problem, content_type = "application/problem+json"),
        (status = 422, description = "validation_failed, scenario_invalid, scenario_load_failed", body = umbra_api_types::Problem, content_type = "application/problem+json"),
        CommonProblems,
    ),
)]
pub(crate) async fn load_scenario(
    State(state): State<Arc<AppState>>,
    ApiJson(request): ApiJson<LoadScenarioRequest>,
) -> Result<(StatusCode, Json<ScenarioInfo>), ApiError> {
    let clock = request
        .clock
        .as_ref()
        .map(map::clock_mode)
        .transpose()
        .map_err(|error| ApiError::new(ProblemCode::ValidationFailed, error.to_string()))?;
    let path = PathBuf::from(request.path);
    let seed = request.seed.map(|seed| seed.0);
    let info = state
        .call(|reply| Command::Load {
            path,
            seed,
            clock,
            reply,
        })
        .await?;
    Ok((StatusCode::CREATED, Json(map::scenario_info(&info))))
}

/// The loaded scenario.
#[utoipa::path(
    get,
    path = "/api/v1/scenario",
    operation_id = "getScenario",
    tag = "scenario",
    responses(
        (status = 200, description = "The loaded scenario", body = ScenarioInfo),
        (status = 409, description = "no_scenario", body = umbra_api_types::Problem, content_type = "application/problem+json"),
        CommonProblems,
    ),
)]
pub(crate) async fn get_scenario(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ScenarioInfo>, ApiError> {
    let status = state.call(|reply| Command::Status { reply }).await?;
    status
        .scenario
        .as_ref()
        .map(|info| Json(map::scenario_info(info)))
        .ok_or_else(|| state.problem(&HandleError::NoSession))
}

/// Unloads the scenario; the run returns to `idle`.
#[utoipa::path(
    delete,
    path = "/api/v1/scenario",
    operation_id = "unloadScenario",
    tag = "scenario",
    responses(
        (status = 204, description = "Unloaded"),
        (status = 409, description = "no_scenario", body = umbra_api_types::Problem, content_type = "application/problem+json"),
        CommonProblems,
    ),
)]
pub(crate) async fn unload_scenario(
    State(state): State<Arc<AppState>>,
) -> Result<StatusCode, ApiError> {
    state.call(|reply| Command::Unload { reply }).await?;
    Ok(StatusCode::NO_CONTENT)
}
