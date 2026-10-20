//! Read routes whose answers arrive in later work items (ASM-197). They parse their parameters with
//! the shared extractors, so the error rows of AT-6 hold from WI-11; a request that is valid answers
//! `500 internal` until WI-13 (entities) and WI-14 (components) replace them.

use std::sync::Arc;

use axum::extract::State;
use umbra_api_types::{ComponentKind, EntityPage, ProblemCode};
use umbra_engine::EntityId;
use umbra_engine::handle::{Command, EntityQuery};

use crate::extract::{ApiPath, ApiQuery};
use crate::openapi::{CommonProblems, ComponentListParams, EntityListParams};
use crate::problem::ApiError;
use crate::state::AppState;

fn not_served(what: &str, work_item: &str) -> ApiError {
    ApiError::new(
        ProblemCode::Internal,
        format!("{what} is not served yet ({work_item})"),
    )
}

/// `GET /api/v1/entities`: parameters and engine errors only (WI-13 serves the page).
#[utoipa::path(
    get,
    path = "/api/v1/entities",
    operation_id = "listEntities",
    tag = "entities",
    params(EntityListParams),
    responses(
        (status = 200, description = "A page of entities (served from WI-13)", body = EntityPage),
        (status = 400, description = "bad_request", body = umbra_api_types::Problem, content_type = "application/problem+json"),
        (status = 409, description = "no_scenario", body = umbra_api_types::Problem, content_type = "application/problem+json"),
        (status = 422, description = "validation_failed (`limit` out of range)", body = umbra_api_types::Problem, content_type = "application/problem+json"),
        CommonProblems,
    ),
)]
pub(crate) async fn list_entities(
    State(state): State<Arc<AppState>>,
    query: ApiQuery,
) -> Result<axum::Json<EntityPage>, ApiError> {
    let mut entities = EntityQuery::default();
    entities.limit = query.limit()?;
    entities.after_id = query.number::<u64>("after_id")?.and_then(EntityId::new);
    state
        .call(|reply| Command::Entities {
            query: entities,
            reply,
        })
        .await?;
    Err(not_served("the entity listing", "WI-13"))
}

/// `GET /api/v1/entities/{id}`: the id and engine errors only (WI-13 serves the entity).
#[utoipa::path(
    get,
    path = "/api/v1/entities/{id}",
    operation_id = "getEntity",
    tag = "entities",
    params(("id" = u64, Path, description = "The entity id")),
    responses(
        (status = 200, description = "The entity (served from WI-13)", body = umbra_api_types::EntityDetail),
        (status = 400, description = "bad_request", body = umbra_api_types::Problem, content_type = "application/problem+json"),
        (status = 404, description = "entity_not_found", body = umbra_api_types::Problem, content_type = "application/problem+json"),
        (status = 409, description = "no_scenario", body = umbra_api_types::Problem, content_type = "application/problem+json"),
        CommonProblems,
    ),
)]
pub(crate) async fn get_entity(
    State(state): State<Arc<AppState>>,
    ApiPath(id): ApiPath<u64>,
) -> Result<axum::Json<umbra_api_types::EntityDetail>, ApiError> {
    let Some(id) = EntityId::new(id) else {
        return Err(ApiError::new(
            ProblemCode::EntityNotFound,
            "entity ids start at 1",
        ));
    };
    state.call(|reply| Command::Entity { id, reply }).await?;
    Err(not_served("the entity", "WI-13"))
}

/// `GET /api/v1/components`: the `kind` parameter only (WI-14 serves the list).
#[utoipa::path(
    get,
    path = "/api/v1/components",
    operation_id = "listComponents",
    tag = "components",
    params(ComponentListParams),
    responses(
        (status = 200, description = "Registered components (served from WI-14)", body = Vec<umbra_api_types::ComponentEntry>),
        (status = 400, description = "bad_request", body = umbra_api_types::Problem, content_type = "application/problem+json"),
        CommonProblems,
    ),
)]
pub(crate) async fn list_components(
    query: ApiQuery,
) -> Result<axum::Json<Vec<umbra_api_types::ComponentEntry>>, ApiError> {
    if let Some(kind) = query.text("kind")? {
        serde_json::from_value::<ComponentKind>(serde_json::Value::from(kind)).map_err(|_| {
            ApiError::new(
                ProblemCode::BadRequest,
                format!("`kind={kind}` is not a component kind"),
            )
        })?;
    }
    Err(not_served("the component list", "WI-14"))
}
