//! `GET /api/v1/catalog/behavior` (REQ-X-10, REQ-B-13, REQ-B-17).

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use umbra_api_types::BehaviorCatalog;
use umbra_engine::handle::Command;

use crate::map;
use crate::openapi::CommonProblems;
use crate::problem::ApiError;
use crate::state::AppState;

/// The behavior catalog: node types, conditions, actions and behavior graphs, each with its schema,
/// version and provenance. With a scenario it includes the session's plugins and graphs; without
/// one, the engine's own entries.
#[utoipa::path(
    get,
    path = "/api/v1/catalog/behavior",
    operation_id = "getBehaviorCatalog",
    tag = "behaviors",
    responses(
        (status = 200, description = "The behavior catalog", body = BehaviorCatalog),
        CommonProblems,
    ),
)]
pub(crate) async fn get_behavior_catalog(
    State(state): State<Arc<AppState>>,
) -> Result<Json<BehaviorCatalog>, ApiError> {
    let catalog = state.call(|reply| Command::Catalog { reply }).await?;
    Ok(Json(map::behavior_catalog(&catalog)))
}
