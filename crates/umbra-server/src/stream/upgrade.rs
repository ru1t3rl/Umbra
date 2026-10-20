//! `GET /api/v1/stream`: the checks before the upgrade and the hand-over to the connection loop
//! (REQ-X-6, REQ-X-8, ADR 0026 amended, ADR 0027).

use std::sync::Arc;

use axum::extract::State;
use axum::extract::ws::WebSocketUpgrade;
use axum::extract::ws::rejection::WebSocketUpgradeRejection;
use axum::http::HeaderMap;
use axum::http::header::ORIGIN;
use axum::response::{IntoResponse, Response};
use futures_util::StreamExt;
use umbra_api_types::ProblemCode;

use super::{ConnectionParams, Encoding, MAX_MESSAGE_BYTES, ReadFailure, run_connection};
use crate::extract::ApiQuery;
use crate::openapi::{CommonProblems, StreamParams};
use crate::problem::ApiError;
use crate::state::AppState;

/// Upgrades to the stream WebSocket (protocol `umbra.stream.v1`, ADR 0027).
///
/// Refused before the upgrade, in this order: an `Origin` that is not allowed (`403
/// origin_not_allowed`; no `Origin` is allowed), an `encoding` other than `json` (`400
/// unsupported_encoding`), a request that is not a WebSocket upgrade (`400 bad_request`), and more
/// open stream connections than `[server.stream] max_connections` (`503 too_many_connections`).
#[utoipa::path(
    get,
    path = "/api/v1/stream",
    operation_id = "openStream",
    tag = "stream",
    params(StreamParams),
    responses(
        (status = 101, description = "Switching Protocols: a WebSocket carrying the messages of api/stream.schema.json and api/asyncapi.json"),
        (status = 400, description = "unsupported_encoding, or not a WebSocket upgrade request (bad_request)", body = umbra_api_types::Problem, content_type = "application/problem+json"),
        (status = 403, description = "origin_not_allowed, or host_not_allowed", body = umbra_api_types::Problem, content_type = "application/problem+json"),
        (status = 503, description = "too_many_connections", body = umbra_api_types::Problem, content_type = "application/problem+json"),
        CommonProblems,
    ),
)]
pub(crate) async fn open_stream(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    query: ApiQuery,
    upgrade: Result<WebSocketUpgrade, WebSocketUpgradeRejection>,
) -> Result<Response, ApiError> {
    if let Some(origin) = headers.get(ORIGIN)
        && !origin
            .to_str()
            .is_ok_and(|origin| state.origins.allows(origin))
    {
        return Err(ApiError::new(
            ProblemCode::OriginNotAllowed,
            "the Origin header is not allowed (see [server] allowed_origins)",
        ));
    }
    let requested = query.text("encoding")?;
    let encoding = Encoding::parse(requested).map_err(|other| {
        ApiError::new(
            ProblemCode::UnsupportedEncoding,
            format!("`encoding={other}` is not supported; the stream speaks `json`"),
        )
    })?;
    let upgrade = upgrade
        .map_err(|rejection| ApiError::new(ProblemCode::BadRequest, rejection.body_text()))?;
    let settings = state.settings.server.stream.clone();
    let slot = state
        .take_connection(settings.max_connections)
        .ok_or_else(|| {
            ApiError::new(
                ProblemCode::TooManyConnections,
                format!(
                    "{} stream connections are open, the most [server.stream] max_connections allows",
                    settings.max_connections
                ),
            )
        })?;
    let handle = state.handle.clone();
    let shutdown = state.shutdown.clone();
    // The library reads at most twice the message limit, so a huge frame never gets buffered; the
    // connection loop closes anything above the limit itself with 1009.
    let response = upgrade
        .max_message_size(2 * MAX_MESSAGE_BYTES)
        .max_frame_size(2 * MAX_MESSAGE_BYTES)
        .on_upgrade(move |socket| async move {
            // Held until the connection ends: the connection limit counts open streams.
            let _slot = slot;
            let (sink, stream) = socket.split();
            let stream = stream.map(|item| item.map_err(|error| read_failure(&error)));
            run_connection(
                sink,
                stream,
                ConnectionParams::new(handle, settings, shutdown, encoding),
            )
            .await;
        });
    Ok(response.into_response())
}

/// Tells a message the library refused for its size (the 2x read cap above) from other read errors.
fn read_failure(error: &axum::Error) -> ReadFailure {
    let too_big = std::error::Error::source(error)
        .and_then(|source| source.downcast_ref::<tungstenite::Error>())
        .is_some_and(|inner| matches!(inner, tungstenite::Error::Capacity(_)));
    if too_big {
        ReadFailure::TooBig
    } else {
        ReadFailure::Other(error.to_string())
    }
}
