//! The served `OpenAPI` document (REQ-X-4, ADR 0025). WI-11 documents the routes it registers and
//! merges the schemas of `umbra-api-types`; WI-12 completes the document, generates the committed
//! files and the drift check.

use axum::Json;
use serde::Deserialize;
use utoipa::openapi::OpenApi as Document;
use utoipa::{IntoParams, IntoResponses, OpenApi};

use umbra_api_types::{API_VERSION, ApiSchemas, ComponentKind, Ownership, Problem};

use crate::handlers;

/// The problem responses every route documents (ADR 0026): `403 host_not_allowed`,
/// `405 method_not_allowed`, `500 internal`, and `503 engine_busy` / `engine_unavailable`.
#[derive(IntoResponses)]
pub enum CommonProblems {
    /// `host_not_allowed`.
    #[response(status = 403, content_type = "application/problem+json")]
    HostNotAllowed(#[to_schema] Problem),
    /// `method_not_allowed`.
    #[response(status = 405, content_type = "application/problem+json")]
    MethodNotAllowed(#[to_schema] Problem),
    /// `internal`.
    #[response(status = 500, content_type = "application/problem+json")]
    Internal(#[to_schema] Problem),
    /// `engine_busy`, `engine_unavailable`.
    #[response(status = 503, content_type = "application/problem+json")]
    Unavailable(#[to_schema] Problem),
}

/// The query of `GET /api/v1/entities`, for the document only: the server parses the raw query
/// itself ([`ApiQuery`](crate::extract::ApiQuery)) so that array parameters may be repeated or
/// comma-separated (ADR 0026, m5). WI-13 reuses this type for the same operation.
#[derive(IntoParams, Deserialize)]
#[into_params(parameter_in = Query)]
#[expect(dead_code, reason = "documents the query; ApiQuery reads it")]
pub struct EntityListParams {
    /// Entities with an id above this one (keyset cursor).
    after_id: Option<u64>,
    /// Page size, 1 to 1000; default 100.
    #[param(minimum = 1, maximum = 1000)]
    limit: Option<u32>,
    /// Only these ids: repeated (`ids=1&ids=2`) or comma-separated (`ids=1,2`).
    #[param(style = Form, explode = true)]
    ids: Option<Vec<u64>>,
    /// Only the entity with this name.
    name: Option<String>,
    /// Only this force.
    force: Option<String>,
    /// Only this group.
    group: Option<String>,
    /// Only this template.
    template: Option<String>,
    /// Only this ownership.
    ownership: Option<Ownership>,
    /// Southern edge of a bounding box; give all four edges or none.
    min_lat_deg: Option<f64>,
    /// Western edge of a bounding box; larger than the eastern edge crosses the antimeridian.
    min_lon_deg: Option<f64>,
    /// Northern edge of a bounding box.
    max_lat_deg: Option<f64>,
    /// Eastern edge of a bounding box.
    max_lon_deg: Option<f64>,
}

/// The query of `GET /api/v1/components`, for the document only.
#[derive(IntoParams, Deserialize)]
#[into_params(parameter_in = Query)]
#[expect(dead_code, reason = "documents the query; the handler reads it")]
pub struct ComponentListParams {
    /// Only components of this kind.
    kind: Option<ComponentKind>,
}

#[derive(OpenApi)]
#[openapi(paths(
    handlers::status::get_status,
    handlers::scenario::load_scenario,
    handlers::scenario::get_scenario,
    handlers::scenario::unload_scenario,
    handlers::run::start_run,
    handlers::run::pause_run,
    handlers::run::resume_run,
    handlers::run::stop_run,
    handlers::run::step_run,
    handlers::run::set_clock,
    handlers::pending::list_entities,
    handlers::pending::get_entity,
    handlers::pending::list_components,
    get_openapi,
))]
struct Routes;

/// The `OpenAPI` document: the routes registered so far plus every schema of the wire types.
#[must_use]
pub fn document() -> Document {
    let mut document = Routes::openapi();
    document.merge(ApiSchemas::openapi());
    "Umbra control API".clone_into(&mut document.info.title);
    API_VERSION.clone_into(&mut document.info.version);
    document
}

/// `GET /api/v1/openapi.json`.
#[utoipa::path(
    get,
    path = "/api/v1/openapi.json",
    operation_id = "getOpenApi",
    tag = "status",
    responses((status = 200, description = "This document", content_type = "application/json"), CommonProblems),
)]
pub(crate) async fn get_openapi() -> Json<Document> {
    Json(document())
}
