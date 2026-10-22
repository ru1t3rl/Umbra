//! Network adapters under `/api/v1/network/adapters` (REQ-X-5, ADR 0035): list them, enable or
//! disable one by its scenario name. Each command touches the named adapter only.

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use umbra_api_types::AdapterDto;
use umbra_engine::handle::Command;

use crate::extract::ApiPath;
use crate::map;
use crate::openapi::CommonProblems;
use crate::problem::ApiError;
use crate::state::AppState;

/// The network adapters of the loaded scenario in scenario order, with their state and counters;
/// an empty list when the scenario has none.
#[utoipa::path(
    get,
    path = "/api/v1/network/adapters",
    operation_id = "listNetworkAdapters",
    tag = "network",
    responses(
        (status = 200, description = "The adapters in scenario order", body = Vec<AdapterDto>),
        (status = 409, description = "no_scenario", body = umbra_api_types::Problem, content_type = "application/problem+json"),
        CommonProblems,
    ),
)]
pub(crate) async fn list_adapters(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<AdapterDto>>, ApiError> {
    let adapters = state
        .call(|reply| Command::NetworkAdapters { reply })
        .await?;
    Ok(Json(adapters.iter().map(map::adapter).collect()))
}

/// Enables the adapter: it is polled and published to again, and every owned entity is sent afresh.
/// Idempotent; applied between ticks; valid in every state with a scenario.
#[utoipa::path(
    post,
    path = "/api/v1/network/adapters/{name}/enable",
    operation_id = "enableNetworkAdapter",
    tag = "network",
    params(("name" = String, Path, description = "The adapter's name in the scenario")),
    responses(
        (status = 200, description = "The adapter after the change", body = AdapterDto),
        (status = 404, description = "adapter_not_found", body = umbra_api_types::Problem, content_type = "application/problem+json"),
        (status = 409, description = "no_scenario, adapter_faulted", body = umbra_api_types::Problem, content_type = "application/problem+json"),
        CommonProblems,
    ),
)]
pub(crate) async fn enable_adapter(
    State(state): State<Arc<AppState>>,
    ApiPath(name): ApiPath<String>,
) -> Result<Json<AdapterDto>, ApiError> {
    set(&state, name, true).await
}

/// Disables the adapter: it is neither published to nor polled (its sockets stay open) and its
/// ghosts time out normally. Idempotent; applied between ticks; allowed for a faulted adapter.
#[utoipa::path(
    post,
    path = "/api/v1/network/adapters/{name}/disable",
    operation_id = "disableNetworkAdapter",
    tag = "network",
    params(("name" = String, Path, description = "The adapter's name in the scenario")),
    responses(
        (status = 200, description = "The adapter after the change", body = AdapterDto),
        (status = 404, description = "adapter_not_found", body = umbra_api_types::Problem, content_type = "application/problem+json"),
        (status = 409, description = "no_scenario", body = umbra_api_types::Problem, content_type = "application/problem+json"),
        CommonProblems,
    ),
)]
pub(crate) async fn disable_adapter(
    State(state): State<Arc<AppState>>,
    ApiPath(name): ApiPath<String>,
) -> Result<Json<AdapterDto>, ApiError> {
    set(&state, name, false).await
}

async fn set(state: &AppState, name: String, enabled: bool) -> Result<Json<AdapterDto>, ApiError> {
    let report = state
        .call(|reply| Command::SetAdapterEnabled {
            name,
            enabled,
            reply,
        })
        .await?;
    Ok(Json(map::adapter(&report)))
}
