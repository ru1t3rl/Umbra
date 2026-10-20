//! `GET /api/v1/status`.

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use umbra_api_types::EngineStatus;
use umbra_engine::handle::Command;

use crate::map;
use crate::openapi::CommonProblems;
use crate::problem::ApiError;
use crate::state::AppState;

/// The engine status: the contract version, the run status and the loaded scenario, if any.
#[utoipa::path(
    get,
    path = "/api/v1/status",
    operation_id = "getStatus",
    tag = "status",
    responses(
        (status = 200, description = "The engine status", body = EngineStatus),
        CommonProblems,
    ),
)]
pub(crate) async fn get_status(
    State(state): State<Arc<AppState>>,
) -> Result<Json<EngineStatus>, ApiError> {
    let status = state.call(|reply| Command::Status { reply }).await?;
    Ok(Json(map::engine_status(&status)))
}
