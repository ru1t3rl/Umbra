//! Run control under `/api/v1/run`.

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use umbra_api_types::{ClockSetting, ProblemCode, RunStatus, StepRequest, StepResult};
use umbra_engine::handle::{Command, Reply};

use crate::extract::ApiJson;
use crate::map;
use crate::openapi::CommonProblems;
use crate::problem::ApiError;
use crate::state::AppState;

/// Sends a state-changing command and answers with the run status after it.
async fn change(
    state: &AppState,
    make: impl FnOnce(Reply<umbra_engine::bus::RunStatus>) -> Command,
) -> Result<Json<RunStatus>, ApiError> {
    let status = state.call(make).await?;
    Ok(Json(map::run_status(&status)))
}

/// `ready` to `running`.
#[utoipa::path(
    post,
    path = "/api/v1/run/start",
    operation_id = "startRun",
    tag = "run",
    responses(
        (status = 200, description = "Running", body = RunStatus),
        (status = 409, description = "no_scenario, invalid_state", body = umbra_api_types::Problem, content_type = "application/problem+json"),
        CommonProblems,
    ),
)]
pub(crate) async fn start_run(
    State(state): State<Arc<AppState>>,
) -> Result<Json<RunStatus>, ApiError> {
    change(&state, |reply| Command::Start { reply }).await
}

/// `running` to `paused`; cancels a pending step.
#[utoipa::path(
    post,
    path = "/api/v1/run/pause",
    operation_id = "pauseRun",
    tag = "run",
    responses(
        (status = 200, description = "Paused", body = RunStatus),
        (status = 409, description = "no_scenario, invalid_state", body = umbra_api_types::Problem, content_type = "application/problem+json"),
        CommonProblems,
    ),
)]
pub(crate) async fn pause_run(
    State(state): State<Arc<AppState>>,
) -> Result<Json<RunStatus>, ApiError> {
    change(&state, |reply| Command::Pause { reply }).await
}

/// `paused` to `running`.
#[utoipa::path(
    post,
    path = "/api/v1/run/resume",
    operation_id = "resumeRun",
    tag = "run",
    responses(
        (status = 200, description = "Running", body = RunStatus),
        (status = 409, description = "no_scenario, invalid_state", body = umbra_api_types::Problem, content_type = "application/problem+json"),
        CommonProblems,
    ),
)]
pub(crate) async fn resume_run(
    State(state): State<Arc<AppState>>,
) -> Result<Json<RunStatus>, ApiError> {
    change(&state, |reply| Command::Resume { reply }).await
}

/// Any loaded state but `stopped` to `stopped` (reason `command`).
#[utoipa::path(
    post,
    path = "/api/v1/run/stop",
    operation_id = "stopRun",
    tag = "run",
    responses(
        (status = 200, description = "Stopped", body = RunStatus),
        (status = 409, description = "no_scenario, invalid_state", body = umbra_api_types::Problem, content_type = "application/problem+json"),
        CommonProblems,
    ),
)]
pub(crate) async fn stop_run(
    State(state): State<Arc<AppState>>,
) -> Result<Json<RunStatus>, ApiError> {
    change(&state, |reply| Command::Stop { reply }).await
}

/// Runs `count` ticks of a stepped clock and answers when they have run, or when the run stopped
/// earlier (`completed: false`).
#[utoipa::path(
    post,
    path = "/api/v1/run/step",
    operation_id = "stepRun",
    tag = "run",
    request_body = StepRequest,
    responses(
        (status = 200, description = "The step ran (`completed` says whether all ticks did)", body = StepResult),
        (status = 400, description = "invalid_body", body = umbra_api_types::Problem, content_type = "application/problem+json"),
        (status = 409, description = "no_scenario, invalid_state, not_stepped, step_in_progress, step_cancelled", body = umbra_api_types::Problem, content_type = "application/problem+json"),
        (status = 415, description = "unsupported_media_type", body = umbra_api_types::Problem, content_type = "application/problem+json"),
        (status = 422, description = "validation_failed", body = umbra_api_types::Problem, content_type = "application/problem+json"),
        CommonProblems,
    ),
)]
pub(crate) async fn step_run(
    State(state): State<Arc<AppState>>,
    ApiJson(request): ApiJson<StepRequest>,
) -> Result<Json<StepResult>, ApiError> {
    if !request.in_range() {
        return Err(ApiError::new(
            ProblemCode::ValidationFailed,
            format!(
                "`count` must be from 1 to {}",
                umbra_api_types::MAX_STEP_COUNT
            ),
        ));
    }
    let count = request.count;
    let outcome = state.call(|reply| Command::Step { count, reply }).await?;
    Ok(Json(map::step_result(&outcome)))
}

/// Changes the clock in any loaded state but `stopped`.
#[utoipa::path(
    put,
    path = "/api/v1/run/clock",
    operation_id = "setClock",
    tag = "run",
    request_body = ClockSetting,
    responses(
        (status = 200, description = "The run status with the new clock", body = RunStatus),
        (status = 400, description = "invalid_body", body = umbra_api_types::Problem, content_type = "application/problem+json"),
        (status = 409, description = "no_scenario, invalid_state", body = umbra_api_types::Problem, content_type = "application/problem+json"),
        (status = 415, description = "unsupported_media_type", body = umbra_api_types::Problem, content_type = "application/problem+json"),
        (status = 422, description = "validation_failed", body = umbra_api_types::Problem, content_type = "application/problem+json"),
        CommonProblems,
    ),
)]
pub(crate) async fn set_clock(
    State(state): State<Arc<AppState>>,
    ApiJson(setting): ApiJson<ClockSetting>,
) -> Result<Json<RunStatus>, ApiError> {
    let mode = map::clock_mode(&setting)
        .map_err(|error| ApiError::new(ProblemCode::ValidationFailed, error.to_string()))?;
    change(&state, |reply| Command::SetClock { mode, reply }).await
}
