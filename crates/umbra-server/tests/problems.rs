//! AT-6 (`problems.rs`): the error model of ADR 0026 (m7): one status per code, problem details
//! for every non-2xx response, and the mapping of axum's rejections. Requirements: REQ-X-9.
//!
//! The first two tests use `ProblemCode` and `ApiError` directly; the others go through the router.

#![forbid(unsafe_code)]

mod common;

use axum::body::Body;
use axum::http::Method;
use common::Harness;
use serde_json::json;
use umbra_api_types::ProblemCode::{self, *};
use umbra_server::ApiError;

/// The status table of ADR 0026 (amended, m7), one row per code.
const STATUS_TABLE: [(ProblemCode, u16); 27] = [
    (BadRequest, 400),
    (InvalidBody, 400),
    (UnsupportedEncoding, 400),
    (PathNotAllowed, 403),
    (HostNotAllowed, 403),
    (OriginNotAllowed, 403),
    (NotFound, 404),
    (EntityNotFound, 404),
    (TemplateNotFound, 404),
    (MethodNotAllowed, 405),
    (NoScenario, 409),
    (InvalidState, 409),
    (NotStepped, 409),
    (StepInProgress, 409),
    (StepCancelled, 409),
    (DuplicateName, 409),
    (PayloadTooLarge, 413),
    (UnsupportedMediaType, 415),
    (ValidationFailed, 422),
    (InvalidId, 422),
    (InvalidSpawn, 422),
    (ScenarioInvalid, 422),
    (ScenarioLoadFailed, 422),
    (Internal, 500),
    (EngineBusy, 503),
    (EngineUnavailable, 503),
    (TooManyConnections, 503),
];

#[test]
fn every_code_has_exactly_one_status() {
    assert_eq!(ProblemCode::ALL.len(), STATUS_TABLE.len());
    for (code, status) in STATUS_TABLE {
        assert!(ProblemCode::ALL.contains(&code), "{code:?} is in ALL");
        assert_eq!(code.status(), status, "{code:?}");
        assert_eq!(ApiError::new(code, "d").status(), status, "{code:?}");
    }
    let mut names: Vec<&str> = ProblemCode::ALL.iter().map(|c| c.as_str()).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(
        names.len(),
        ProblemCode::ALL.len(),
        "codes are unique strings"
    );
}

#[test]
fn api_error_becomes_a_problem_with_type_status_and_code() {
    // Extra: the body every handler error produces.
    for code in ProblemCode::ALL {
        let problem = ApiError::new(*code, "detail").into_problem();
        assert_eq!(problem.code, *code);
        assert_eq!(problem.status, code.status());
        assert_eq!(
            problem.r#type,
            format!("urn:umbra:problem:{}", code.as_str())
        );
        assert!(problem.diagnostics.is_none());
    }
}

#[tokio::test]
async fn every_error_is_problem_json_with_a_documented_code() {
    let h = Harness::start("problems-all");
    let requests = [
        (Method::GET, "/api/v1/nope"),
        (Method::PATCH, "/api/v1/status"),
        (Method::GET, "/api/v1/scenario"),
        (Method::GET, "/api/v1/entities"),
        (Method::GET, "/api/v1/entities/abc"),
        (Method::DELETE, "/api/v1/scenario"),
        (Method::POST, "/api/v1/run/start"),
        (Method::GET, "/api/v1/components?kind=bogus"),
    ];
    for (method, uri) in requests {
        let reply = h.send(method.clone(), uri).await;
        assert!(
            reply.status >= 400,
            "{method} {uri} answered {}",
            reply.status
        );
        let problem = reply.problem();
        assert!(ProblemCode::ALL.contains(&problem.code), "{method} {uri}");
        assert_eq!(reply.status, problem.code.status(), "{method} {uri}");
        assert_eq!(problem.status, reply.status);
        assert_eq!(
            problem.r#type,
            format!("urn:umbra:problem:{}", problem.code.as_str())
        );
        assert!(
            reply
                .content_type
                .as_deref()
                .is_some_and(|t| t.starts_with("application/problem+json")),
            "{method} {uri}: {:?}",
            reply.content_type
        );
    }
}

#[tokio::test]
async fn unknown_route_is_404_not_found() {
    let h = Harness::start("problems-404");
    for uri in ["/api/v1/nope", "/nope", "/api/v2/status"] {
        h.get(uri).await.assert_problem(NotFound);
    }
}

#[tokio::test]
async fn wrong_method_is_405_with_problem_body() {
    let h = Harness::start("problems-405");
    for (method, uri) in [
        (Method::POST, "/api/v1/status"),
        (Method::PATCH, "/api/v1/entities"),
        (Method::GET, "/api/v1/run/start"),
    ] {
        h.send(method, uri).await.assert_problem(MethodNotAllowed);
    }
}

#[tokio::test]
async fn missing_content_type_is_415() {
    let h = Harness::start("problems-415");
    let body = json!({"path": "walking_skeleton/scenario.toml"}).to_string();
    let none = h
        .send_with(
            Method::POST,
            "/api/v1/scenario",
            &[],
            Body::from(body.clone()),
        )
        .await;
    none.assert_problem(UnsupportedMediaType);
    let wrong = h
        .send_with(
            Method::POST,
            "/api/v1/scenario",
            &[("content-type", "text/plain")],
            Body::from(body),
        )
        .await;
    wrong.assert_problem(UnsupportedMediaType);
}

#[tokio::test]
async fn malformed_json_is_400_invalid_body() {
    let h = Harness::start("problems-400-body");
    for text in ["{\"path\": ", "not json", ""] {
        let reply = h
            .send_with(
                Method::POST,
                "/api/v1/scenario",
                &[("content-type", "application/json")],
                Body::from(text),
            )
            .await;
        reply.assert_problem(InvalidBody);
    }
}

#[tokio::test]
async fn unknown_field_or_wrong_type_is_422_validation_failed() {
    let h = Harness::start("problems-422");
    for body in [
        json!({"path": "walking_skeleton/scenario.toml", "bogus": 1}),
        json!({"path": 5}),
        json!({}),
    ] {
        h.load_body(&body).await.assert_problem(ValidationFailed);
    }
    h.post_json("/api/v1/run/step", &json!({"count": "ten"}))
        .await
        .assert_problem(ValidationFailed);
}

#[tokio::test]
async fn bad_path_or_query_param_is_400_bad_request() {
    let h = Harness::start("problems-400-param");
    h.load_skeleton("stepped").await;
    for uri in [
        "/api/v1/entities/abc",
        "/api/v1/entities?limit=abc",
        "/api/v1/entities?after_id=-1",
        "/api/v1/components?kind=nonsense",
    ] {
        h.get(uri).await.assert_problem(BadRequest);
    }
}

#[tokio::test]
async fn body_over_the_configured_limit_is_413() {
    let h = Harness::with("problems-413", |config| {
        config.server.body_limit_bytes = 1024;
    });
    let big = json!({"path": "x".repeat(4096)});
    h.load_body(&big).await.assert_problem(PayloadTooLarge);
    // A body within the limit is not refused for its size.
    let small = h
        .load_body(&json!({"path": "walking_skeleton/scenario.toml"}))
        .await;
    assert_eq!(small.status, 201, "{}", small.text());
}
