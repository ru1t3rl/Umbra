//! The access-control seam (ADR 0026, spec 1.3): every mutating route is registered through one
//! layer, so a later authentication or role check wraps them without touching a handler. In v1 the
//! layer lets every request through.

use axum::Router;
use axum::extract::Request;
use axum::middleware::{self, Next};
use axum::response::Response;

/// Wraps `routes`, the mutating routes of the API, in the access-control seam.
pub fn guard<S: Clone + Send + Sync + 'static>(routes: Router<S>) -> Router<S> {
    routes.layer(middleware::from_fn(allow_all))
}

/// v1 has no access control (REQ-X-8, spec 1.3).
async fn allow_all(request: Request, next: Next) -> Response {
    next.run(request).await
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Method, Request, StatusCode};
    use axum::routing::{get, post};
    use tower::ServiceExt;

    use super::*;

    #[tokio::test]
    async fn the_seam_passes_requests_through_in_v1() {
        let routes = Router::new()
            .route("/m", post(|| async { StatusCode::NO_CONTENT }))
            .route("/r", get(|| async { StatusCode::OK }));
        let app = guard(routes);
        for (method, uri, status) in [
            (Method::POST, "/m", StatusCode::NO_CONTENT),
            (Method::GET, "/r", StatusCode::OK),
        ] {
            let request = Request::builder()
                .method(method)
                .uri(uri)
                .body(Body::empty());
            let response = app
                .clone()
                .oneshot(request.expect("request"))
                .await
                .expect("response");
            assert_eq!(response.status(), status, "{uri}");
        }
    }
}
