//! The server: the router over the engine handle (REQ-X-4, REQ-X-5, ADR 0026).

use std::future::IntoFuture;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::middleware;
use axum::response::IntoResponse;
use axum::routing::{delete, get, post, put};
use tokio::net::TcpListener;
use umbra_api_types::ProblemCode;
use umbra_engine::handle::EngineHandle;

use crate::config::ServerSettings;
use crate::handlers::{components, entities, run, scenario, status, templates};
use crate::layers::{host, mutating};
use crate::openapi;
use crate::problem::ApiError;
use crate::shutdown::Shutdown;
use crate::state::AppState;
use crate::stream::ConnectionStats;

/// The REST API and stream over one engine handle. It talks only to the handle (REQ-X-7).
///
/// `Server` is cheap to clone: clones share one state, so a test or the CLI keeps a clone for
/// [`Server::stream_stats`] while another is consumed by [`Server::serve`].
#[derive(Clone)]
pub struct Server {
    state: Arc<AppState>,
}

impl Server {
    /// A server for `handle`. `shutdown` ends the stream connections and the serve loop.
    #[must_use]
    pub fn new(handle: EngineHandle, settings: ServerSettings, shutdown: Shutdown) -> Self {
        Self {
            state: Arc::new(AppState::new(handle, settings, shutdown)),
        }
    }

    /// The router of every `/api/v1` route with its layers. In-process tests call it with
    /// `tower::ServiceExt::oneshot`.
    ///
    /// Layers, outermost first: the `Host` allow-list, the body limit, the mutating-route seam, the
    /// handlers. The stream route (`/api/v1/stream`) is added by WI-15.
    pub fn router(&self) -> Router {
        let readers = Router::new()
            .route("/api/v1/status", get(status::get_status))
            .route("/api/v1/scenario", get(scenario::get_scenario))
            .route("/api/v1/entities", get(entities::list_entities))
            .route("/api/v1/entities/{id}", get(entities::get_entity))
            .route("/api/v1/templates", get(templates::list_templates))
            .route("/api/v1/templates/{id}", get(templates::get_template))
            .route("/api/v1/components", get(components::list_components))
            .route("/api/v1/openapi.json", get(openapi::get_openapi));
        let writers = mutating::guard(
            Router::new()
                .route("/api/v1/scenario", post(scenario::load_scenario))
                .route("/api/v1/scenario", delete(scenario::unload_scenario))
                .route("/api/v1/entities", post(entities::spawn_entity))
                .route("/api/v1/entities/{id}", delete(entities::remove_entity))
                .route("/api/v1/run/start", post(run::start_run))
                .route("/api/v1/run/pause", post(run::pause_run))
                .route("/api/v1/run/resume", post(run::resume_run))
                .route("/api/v1/run/stop", post(run::stop_run))
                .route("/api/v1/run/step", post(run::step_run))
                .route("/api/v1/run/clock", put(run::set_clock)),
        );
        let limit =
            usize::try_from(self.state.settings.server.body_limit_bytes).unwrap_or(usize::MAX);
        readers
            .merge(writers)
            .fallback(not_found)
            .method_not_allowed_fallback(method_not_allowed)
            .layer(DefaultBodyLimit::max(limit))
            .layer(middleware::from_fn_with_state(
                Arc::clone(&self.state),
                host::check,
            ))
            .with_state(Arc::clone(&self.state))
    }

    /// Serves on `listener` until the shutdown signal fires, then waits for open requests up to the
    /// grace period `[server] shutdown_grace_s` (WI-17 adds signal handling to the CLI).
    ///
    /// # Errors
    /// The listener's I/O error.
    pub async fn serve(self, listener: TcpListener) -> std::io::Result<()> {
        let shutdown = self.state.shutdown.clone();
        let grace: Duration = self.state.settings.server.shutdown_grace;
        let serving = axum::serve(listener, self.router())
            .with_graceful_shutdown({
                let shutdown = shutdown.clone();
                async move { shutdown.triggered().await }
            })
            .into_future();
        tokio::pin!(serving);
        tokio::select! {
            result = &mut serving => result,
            () = async {
                shutdown.triggered().await;
                tokio::time::sleep(grace).await;
            } => Ok(()),
        }
    }

    /// Per-connection queue maxima and drop counts, in connection order (WI-16).
    #[doc(hidden)]
    #[must_use]
    #[expect(
        clippy::todo,
        reason = "WI-6 signature stub; the body arrives in WI-16"
    )]
    pub fn stream_stats(&self) -> Vec<ConnectionStats> {
        todo!("WI-16")
    }
}

/// An unknown route.
async fn not_found() -> impl IntoResponse {
    ApiError::new(ProblemCode::NotFound, "no such route")
}

/// A known route with a method it does not take.
async fn method_not_allowed() -> impl IntoResponse {
    ApiError::new(
        ProblemCode::MethodNotAllowed,
        "the method is not allowed on this route",
    )
}
