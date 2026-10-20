//! `GET /api/v1/plugins` (REQ-X-5, ADR 0026).

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use umbra_api_types::PluginReport;
use umbra_engine::handle::Command;

use crate::map;
use crate::openapi::CommonProblems;
use crate::problem::ApiError;
use crate::state::AppState;

/// The plugins of the loaded scenario with their capabilities, and the fault that stopped the run,
/// if any. Empty without a scenario.
#[utoipa::path(
    get,
    path = "/api/v1/plugins",
    operation_id = "getPlugins",
    tag = "plugins",
    responses(
        (status = 200, description = "The loaded plugins", body = PluginReport),
        CommonProblems,
    ),
)]
pub(crate) async fn get_plugins(
    State(state): State<Arc<AppState>>,
) -> Result<Json<PluginReport>, ApiError> {
    let report = state.call(|reply| Command::Plugins { reply }).await?;
    Ok(Json(map::plugin_report(&report)))
}
