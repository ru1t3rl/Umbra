//! Templates under `/api/v1/templates` (REQ-X-5, REQ-T-3).

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use umbra_api_types::{ProblemCode, TemplateDetail, TemplateSummary};
use umbra_engine::handle::Command;

use crate::extract::ApiPath;
use crate::map;
use crate::openapi::CommonProblems;
use crate::problem::ApiError;
use crate::state::AppState;

/// The templates of the loaded scenario, by id.
#[utoipa::path(
    get,
    path = "/api/v1/templates",
    operation_id = "listTemplates",
    tag = "templates",
    responses(
        (status = 200, description = "The templates", body = Vec<TemplateSummary>),
        (status = 409, description = "no_scenario", body = umbra_api_types::Problem, content_type = "application/problem+json"),
        CommonProblems,
    ),
)]
pub(crate) async fn list_templates(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<TemplateSummary>>, ApiError> {
    let templates = state.call(|reply| Command::Templates { reply }).await?;
    Ok(Json(templates.iter().map(map::template_summary).collect()))
}

/// One template with its inheritance applied.
#[utoipa::path(
    get,
    path = "/api/v1/templates/{id}",
    operation_id = "getTemplate",
    tag = "templates",
    params(("id" = String, Path, description = "The template id")),
    responses(
        (status = 200, description = "The template", body = TemplateDetail),
        (status = 404, description = "template_not_found", body = umbra_api_types::Problem, content_type = "application/problem+json"),
        (status = 409, description = "no_scenario", body = umbra_api_types::Problem, content_type = "application/problem+json"),
        CommonProblems,
    ),
)]
pub(crate) async fn get_template(
    State(state): State<Arc<AppState>>,
    ApiPath(id): ApiPath<String>,
) -> Result<Json<TemplateDetail>, ApiError> {
    let templates = state.call(|reply| Command::Templates { reply }).await?;
    templates
        .iter()
        .find(|template| template.id.as_str() == id)
        .map(|template| Json(map::template_detail(template)))
        .ok_or_else(|| {
            ApiError::new(
                ProblemCode::TemplateNotFound,
                format!("the scenario has no template `{id}`"),
            )
        })
}
