//! Entities under `/api/v1/entities` (REQ-X-5, ADR 0026).

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::http::header::{HeaderName, LOCATION};
use umbra_api_types::{EntityDetail, EntityPage, ProblemCode, SpawnRequest};
use umbra_engine::EntityId;
use umbra_engine::handle::Command;

use crate::extract::{ApiJson, ApiPath, ApiQuery};
use crate::map;
use crate::openapi::{CommonProblems, EntityListParams};
use crate::problem::ApiError;
use crate::state::AppState;

/// An entity id from the path. Id 0 is never an entity, so it is `404` without asking the engine
/// (and before `409 no_scenario`, ASM-198).
fn entity_id(id: u64) -> Result<EntityId, ApiError> {
    EntityId::new(id).ok_or_else(|| {
        ApiError::new(
            ProblemCode::EntityNotFound,
            "entity ids start at 1; no entity has id 0",
        )
    })
}

/// A page of entities, ascending by id, optionally filtered.
#[utoipa::path(
    get,
    path = "/api/v1/entities",
    operation_id = "listEntities",
    tag = "entities",
    params(EntityListParams),
    responses(
        (status = 200, description = "A page of entities", body = EntityPage),
        (status = 400, description = "bad_request", body = umbra_api_types::Problem, content_type = "application/problem+json"),
        (status = 409, description = "no_scenario", body = umbra_api_types::Problem, content_type = "application/problem+json"),
        (status = 422, description = "validation_failed (`limit` out of range)", body = umbra_api_types::Problem, content_type = "application/problem+json"),
        CommonProblems,
    ),
)]
pub(crate) async fn list_entities(
    State(state): State<Arc<AppState>>,
    query: ApiQuery,
) -> Result<Json<EntityPage>, ApiError> {
    let query = map::entity_query(&query)?;
    let page = state
        .call(|reply| Command::Entities { query, reply })
        .await?;
    Ok(Json(map::entity_page(&page)))
}

/// One entity.
#[utoipa::path(
    get,
    path = "/api/v1/entities/{id}",
    operation_id = "getEntity",
    tag = "entities",
    params(("id" = u64, Path, description = "The entity id")),
    responses(
        (status = 200, description = "The entity", body = EntityDetail),
        (status = 400, description = "bad_request", body = umbra_api_types::Problem, content_type = "application/problem+json"),
        (status = 404, description = "entity_not_found", body = umbra_api_types::Problem, content_type = "application/problem+json"),
        (status = 409, description = "no_scenario", body = umbra_api_types::Problem, content_type = "application/problem+json"),
        CommonProblems,
    ),
)]
pub(crate) async fn get_entity(
    State(state): State<Arc<AppState>>,
    ApiPath(id): ApiPath<u64>,
) -> Result<Json<EntityDetail>, ApiError> {
    let id = entity_id(id)?;
    let entity = state.call(|reply| Command::Entity { id, reply }).await?;
    Ok(Json(map::entity_detail(&entity)))
}

/// Spawns an entity from a template of the loaded scenario. Valid in `ready`, `running` and
/// `paused`.
#[utoipa::path(
    post,
    path = "/api/v1/entities",
    operation_id = "spawnEntity",
    tag = "entities",
    request_body = SpawnRequest,
    responses(
        (status = 201, description = "Spawned", body = EntityDetail,
            headers(("Location" = String, description = "The new entity: `/api/v1/entities/{id}`"))),
        (status = 400, description = "invalid_body", body = umbra_api_types::Problem, content_type = "application/problem+json"),
        (status = 404, description = "template_not_found", body = umbra_api_types::Problem, content_type = "application/problem+json"),
        (status = 409, description = "no_scenario, invalid_state, duplicate_name", body = umbra_api_types::Problem, content_type = "application/problem+json"),
        (status = 415, description = "unsupported_media_type", body = umbra_api_types::Problem, content_type = "application/problem+json"),
        (status = 422, description = "validation_failed, invalid_id, invalid_spawn", body = umbra_api_types::Problem, content_type = "application/problem+json"),
        CommonProblems,
    ),
)]
pub(crate) async fn spawn_entity(
    State(state): State<Arc<AppState>>,
    ApiJson(request): ApiJson<SpawnRequest>,
) -> Result<(StatusCode, [(HeaderName, String); 1], Json<EntityDetail>), ApiError> {
    let spec = map::spawn_spec(&request)
        .map_err(|error| ApiError::new(error.code(), error.to_string()))?;
    let entity = state.call(|reply| Command::Spawn { spec, reply }).await?;
    let detail = map::entity_detail(&entity);
    let location = format!("{}/entities/{}", crate::API_BASE, detail.summary.id);
    Ok((StatusCode::CREATED, [(LOCATION, location)], Json(detail)))
}

/// Removes an entity. Its id is never reused. Valid in `ready`, `running` and `paused`.
#[utoipa::path(
    delete,
    path = "/api/v1/entities/{id}",
    operation_id = "removeEntity",
    tag = "entities",
    params(("id" = u64, Path, description = "The entity id")),
    responses(
        (status = 204, description = "Removed"),
        (status = 400, description = "bad_request", body = umbra_api_types::Problem, content_type = "application/problem+json"),
        (status = 404, description = "entity_not_found", body = umbra_api_types::Problem, content_type = "application/problem+json"),
        (status = 409, description = "no_scenario, invalid_state", body = umbra_api_types::Problem, content_type = "application/problem+json"),
        CommonProblems,
    ),
)]
pub(crate) async fn remove_entity(
    State(state): State<Arc<AppState>>,
    ApiPath(id): ApiPath<u64>,
) -> Result<StatusCode, ApiError> {
    let id = entity_id(id)?;
    state.call(|reply| Command::Remove { id, reply }).await?;
    Ok(StatusCode::NO_CONTENT)
}
