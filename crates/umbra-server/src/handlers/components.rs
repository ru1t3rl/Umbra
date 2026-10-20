//! Registered components under `/api/v1/components` (REQ-X-5, REQ-X-10).

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use umbra_api_types::ComponentEntry;
use umbra_engine::handle::Command;

use crate::extract::ApiQuery;
use crate::map;
use crate::openapi::{CommonProblems, ComponentListParams};
use crate::problem::ApiError;
use crate::state::AppState;

/// The registered components, optionally of one kind. With a scenario the list includes the
/// plugins' components and the scenario's behavior graphs; without one, the engine's own.
#[utoipa::path(
    get,
    path = "/api/v1/components",
    operation_id = "listComponents",
    tag = "components",
    params(ComponentListParams),
    responses(
        (status = 200, description = "Registered components", body = Vec<ComponentEntry>),
        (status = 400, description = "bad_request", body = umbra_api_types::Problem, content_type = "application/problem+json"),
        CommonProblems,
    ),
)]
pub(crate) async fn list_components(
    State(state): State<Arc<AppState>>,
    query: ApiQuery,
) -> Result<Json<Vec<ComponentEntry>>, ApiError> {
    let kind = map::component_kind_query(&query)?;
    let entries = state
        .call(|reply| Command::Components { kind, reply })
        .await?;
    Ok(Json(
        entries.iter().filter_map(map::component_entry).collect(),
    ))
}
