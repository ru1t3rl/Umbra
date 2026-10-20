//! AT-11 (WI-12, WI-14): the `OpenAPI` document and the stream schema match the router and the
//! responses it gives. Requirements: REQ-X-4, spec section 15, M5 acceptance 4, ADR 0025.
//!
//! `responses_conform_to_their_openapi_schemas` walks real responses against the served document
//! with `common::schema_walk`, which fails closed on keywords it does not support. The committed
//! files are `api/openapi.json` and `api/stream.schema.json` (written by `cargo xtask gen-api`).

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use axum::http::Method;
use common::{Harness, schema_walk};
use serde_json::{Value, json};
use umbra_api_types::{API_VERSION, ApiSchemas};
use utoipa::OpenApi;

/// `(method, path, operationId)` of every route (ADR 0026, the plan's endpoint table).
const ROUTES: [(&str, &str, &str); 22] = [
    ("get", "/api/v1/status", "getStatus"),
    ("post", "/api/v1/scenario", "loadScenario"),
    ("get", "/api/v1/scenario", "getScenario"),
    ("delete", "/api/v1/scenario", "unloadScenario"),
    ("post", "/api/v1/run/start", "startRun"),
    ("post", "/api/v1/run/pause", "pauseRun"),
    ("post", "/api/v1/run/resume", "resumeRun"),
    ("post", "/api/v1/run/stop", "stopRun"),
    ("post", "/api/v1/run/step", "stepRun"),
    ("put", "/api/v1/run/clock", "setClock"),
    ("get", "/api/v1/entities", "listEntities"),
    ("post", "/api/v1/entities", "spawnEntity"),
    ("get", "/api/v1/entities/{id}", "getEntity"),
    ("delete", "/api/v1/entities/{id}", "removeEntity"),
    ("get", "/api/v1/templates", "listTemplates"),
    ("get", "/api/v1/templates/{id}", "getTemplate"),
    ("get", "/api/v1/components", "listComponents"),
    ("get", "/api/v1/plugins", "getPlugins"),
    ("get", "/api/v1/catalog/behavior", "getBehaviorCatalog"),
    ("post", "/api/v1/behaviors/validate", "validateBehavior"),
    ("get", "/api/v1/openapi.json", "getOpenApi"),
    ("get", "/api/v1/stream", "openStream"),
];

/// The extra problem statuses each operation documents, besides the four every route has.
const EXTRA_PROBLEMS: [(&str, &[&str]); 16] = [
    ("loadScenario", &["400", "403", "415", "422"]),
    ("getScenario", &["409"]),
    ("unloadScenario", &["409"]),
    ("startRun", &["409"]),
    ("stepRun", &["409", "422"]),
    ("setClock", &["409", "422"]),
    ("listEntities", &["400", "409"]),
    ("spawnEntity", &["404", "409", "422"]),
    ("getEntity", &["400", "404", "409"]),
    ("removeEntity", &["400", "404", "409"]),
    ("listTemplates", &["409"]),
    ("getTemplate", &["404", "409"]),
    ("listComponents", &["400"]),
    ("validateBehavior", &["400", "413", "415", "422"]),
    ("openStream", &["400", "403", "503"]),
    ("stopRun", &["409"]),
];

const TAGS: [&str; 9] = [
    "status",
    "scenario",
    "run",
    "entities",
    "templates",
    "components",
    "plugins",
    "behaviors",
    "stream",
];

fn expected_routes() -> BTreeSet<(String, String)> {
    ROUTES
        .iter()
        .map(|(m, p, _)| ((*m).to_string(), normalise(p)))
        .collect()
}

/// A path with every `{name}` replaced by `{}`.
fn normalise(path: &str) -> String {
    path.split('/')
        .map(|s| if s.starts_with('{') { "{}" } else { s })
        .collect::<Vec<_>>()
        .join("/")
}

async fn served(h: &Harness) -> Value {
    h.get("/api/v1/openapi.json").await.assert_json(200)
}

fn committed(name: &str) -> Value {
    let path: PathBuf = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../api")
        .join(name);
    serde_json::from_str(
        &std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{}: {e} (run `cargo xtask gen-api`)", path.display())),
    )
    .expect("committed JSON")
}

const METHODS: [&str; 5] = ["get", "post", "put", "delete", "patch"];

/// Every operation of the document as `(method, path, operation)`.
fn operations(doc: &Value) -> Vec<(String, String, &Value)> {
    let mut out = Vec::new();
    for (path, item) in doc["paths"].as_object().expect("paths") {
        for method in METHODS {
            if let Some(operation) = item.get(method) {
                out.push((method.to_string(), path.clone(), operation));
            }
        }
    }
    out
}

#[tokio::test]
async fn openapi_lists_every_route_of_the_router() {
    let h = Harness::start("contract-routes");
    let doc = served(&h).await;
    let listed: BTreeSet<(String, String)> = operations(&doc)
        .into_iter()
        .map(|(m, p, _)| (m, normalise(&p)))
        .collect();
    assert_eq!(
        listed,
        expected_routes(),
        "the document and the route table agree"
    );
    // The router serves each of them: no request is answered with the route-not-found or the
    // method-not-allowed problem.
    for (method, path, _) in operations(&doc) {
        let uri = path.replace("{id}", "1").replace("{template_id}", "x");
        let uri = uri.split('{').next().unwrap_or(&uri).to_string();
        let reply = h
            .send(
                Method::from_bytes(method.to_uppercase().as_bytes()).expect("method"),
                &uri,
            )
            .await;
        assert_ne!(reply.status, 405, "{method} {uri}");
        if reply.status == 404 {
            let code = reply.json()["code"].clone();
            assert_ne!(code, json!("not_found"), "{method} {uri} is not routed");
        }
    }
}

#[tokio::test]
async fn every_operation_has_operation_id_and_one_tag() {
    let h = Harness::start("contract-ids");
    let doc = served(&h).await;
    let mut seen = BTreeMap::new();
    for (method, path, operation) in operations(&doc) {
        let id = operation["operationId"]
            .as_str()
            .unwrap_or_else(|| panic!("{method} {path} has no operationId"));
        assert!(
            seen.insert(id.to_string(), (method.clone(), path.clone()))
                .is_none(),
            "duplicate {id}"
        );
        let expected = ROUTES
            .iter()
            .find(|(m, p, _)| *m == method && normalise(p) == normalise(&path))
            .map(|r| r.2);
        assert_eq!(Some(id), expected, "{method} {path}");
        let tags = operation["tags"].as_array().expect("tags");
        assert_eq!(tags.len(), 1, "{id}: exactly one tag");
        assert!(
            TAGS.contains(&tags[0].as_str().expect("tag")),
            "{id}: {tags:?}"
        );
    }
}

#[tokio::test]
async fn every_operation_documents_problem_responses() {
    let h = Harness::start("contract-problems");
    let doc = served(&h).await;
    for (method, path, operation) in operations(&doc) {
        let id = operation["operationId"].as_str().expect("operationId");
        let mut statuses: Vec<&str> = vec!["403", "405", "500", "503"];
        if let Some((_, extra)) = EXTRA_PROBLEMS.iter().find(|(name, _)| *name == id) {
            statuses.extend(extra.iter());
        } else if id.ends_with("Run") {
            statuses.push("409");
        }
        for status in statuses {
            let response = &operation["responses"][status];
            let schema = &response["content"]["application/problem+json"]["schema"];
            let reference = schema["$ref"].as_str().unwrap_or_else(|| {
                panic!("{method} {path} ({id}): {status} has no application/problem+json schema")
            });
            assert!(
                reference.ends_with("/Problem"),
                "{id} {status}: {reference}"
            );
        }
    }
}

/// One scripted request: method, URI, optional JSON body, and the status it must give.
type Call = (&'static str, &'static str, Option<Value>, u16);

fn script() -> Vec<Call> {
    let load = json!({"path": "walking_skeleton/scenario.toml", "clock": {"mode": "stepped"}});
    let valid = std::fs::read_to_string(
        common::examples_dir().join("patrol_and_engage/behaviors/follow_route.behavior.toml"),
    )
    .expect("example graph");
    vec![
        ("GET", "/api/v1/status", None, 200),
        ("GET", "/api/v1/scenario", None, 409),
        ("GET", "/api/v1/entities", None, 409),
        ("GET", "/api/v1/templates", None, 409),
        ("GET", "/api/v1/components", None, 200),
        ("GET", "/api/v1/plugins", None, 200),
        ("GET", "/api/v1/catalog/behavior", None, 200),
        (
            "POST",
            "/api/v1/behaviors/validate",
            Some(json!({"document": valid})),
            200,
        ),
        (
            "POST",
            "/api/v1/behaviors/validate",
            Some(json!({"document": "x = "})),
            200,
        ),
        (
            "POST",
            "/api/v1/scenario",
            Some(json!({"path": "missing.toml"})),
            422,
        ),
        ("POST", "/api/v1/scenario", Some(load), 201),
        ("GET", "/api/v1/scenario", None, 200),
        ("GET", "/api/v1/status", None, 200),
        ("POST", "/api/v1/run/start", None, 200),
        ("POST", "/api/v1/run/step", Some(json!({"count": 5})), 200),
        (
            "PUT",
            "/api/v1/run/clock",
            Some(json!({"mode": "scaled", "scale": 2.0})),
            200,
        ),
        (
            "PUT",
            "/api/v1/run/clock",
            Some(json!({"mode": "stepped"})),
            200,
        ),
        ("POST", "/api/v1/run/pause", None, 200),
        ("POST", "/api/v1/run/resume", None, 200),
        ("GET", "/api/v1/entities?limit=2", None, 200),
        ("GET", "/api/v1/entities/1", None, 200),
        ("GET", "/api/v1/entities/999", None, 404),
        (
            "POST",
            "/api/v1/entities",
            Some(json!({"template": "truck.cargo", "lat_deg": 52.5, "lon_deg": 4.5, "name": "n"})),
            201,
        ),
        ("DELETE", "/api/v1/entities/4", None, 204),
        ("GET", "/api/v1/templates", None, 200),
        ("GET", "/api/v1/templates/truck.cargo", None, 200),
        ("GET", "/api/v1/components?kind=motion_model", None, 200),
        ("POST", "/api/v1/run/stop", None, 200),
        ("DELETE", "/api/v1/scenario", None, 204),
        ("GET", "/api/v1/openapi.json", None, 200),
    ]
}

/// The path template of the document that matches `uri` (placeholders match any segment; the match
/// with the fewest placeholders wins).
fn template_for(doc: &Value, uri: &str) -> Option<String> {
    let path = uri.split('?').next().unwrap_or(uri);
    let segments: Vec<&str> = path.split('/').collect();
    doc["paths"]
        .as_object()?
        .keys()
        .filter(|template| {
            let parts: Vec<&str> = template.split('/').collect();
            parts.len() == segments.len()
                && parts
                    .iter()
                    .zip(&segments)
                    .all(|(t, s)| t.starts_with('{') || t == s)
        })
        .min_by_key(|t| t.matches('{').count())
        .cloned()
}

#[tokio::test]
async fn responses_conform_to_their_openapi_schemas() {
    let h = Harness::start("contract-conform");
    let doc = served(&h).await;
    let mut exercised = BTreeSet::new();
    for (method, uri, body, expected_status) in script() {
        let m = Method::from_bytes(method.as_bytes()).expect("method");
        let reply = match &body {
            Some(b) => h.send_json(m, uri, b).await,
            None => h.send(m, uri).await,
        };
        assert_eq!(
            reply.status,
            expected_status,
            "{method} {uri}: {}",
            reply.text()
        );
        let template = template_for(&doc, uri).unwrap_or_else(|| panic!("{uri} matches no path"));
        let operation = &doc["paths"][&template][method.to_lowercase()];
        let response = &operation["responses"][reply.status.to_string()];
        assert!(
            response.is_object(),
            "{method} {uri}: status {} is not documented ({})",
            reply.status,
            reply.text()
        );
        if reply.status < 300 {
            exercised.insert((method.to_lowercase(), normalise(&template)));
        }
        if reply.status == 204 {
            assert!(
                response.get("content").is_none(),
                "{method} {uri}: 204 has no content"
            );
            continue;
        }
        let media = reply
            .content_type
            .as_deref()
            .and_then(|t| t.split(';').next())
            .expect("content type")
            .to_string();
        let schema = &response["content"][&media]["schema"];
        assert!(
            schema.is_object(),
            "{method} {uri}: no {media} schema for {}",
            reply.status
        );
        if let Err(e) = schema_walk::validate(&doc, schema, &reply.json()) {
            panic!("{method} {uri} ({}): {e}\n{}", reply.status, reply.text());
        }
    }
    let documented: BTreeSet<(String, String)> = operations(&doc)
        .into_iter()
        .filter(|(_, p, _)| !p.ends_with("/stream"))
        .map(|(m, p, _)| (m, normalise(&p)))
        .collect();
    let missing: Vec<_> = documented.difference(&exercised).collect();
    assert!(
        missing.is_empty(),
        "routes the script never got a 2xx from: {missing:?}"
    );
}

#[tokio::test]
async fn served_openapi_equals_committed_file() {
    let h = Harness::start("contract-served");
    let doc = served(&h).await;
    assert_eq!(doc, committed("openapi.json"));
}

/// Every `$ref` string in `value`.
fn refs(value: &Value, out: &mut BTreeSet<String>) {
    match value {
        Value::Object(map) => {
            for (key, inner) in map {
                if key == "$ref" {
                    if let Some(s) = inner.as_str() {
                        out.insert(s.to_string());
                    }
                } else {
                    refs(inner, out);
                }
            }
        }
        Value::Array(list) => list.iter().for_each(|v| refs(v, out)),
        _ => {}
    }
}

#[test]
fn stream_schema_root_references_every_message_type() {
    let schema = committed("stream.schema.json");
    // Follow the `$ref`s from the root.
    let mut reached = BTreeSet::new();
    let mut todo = BTreeSet::new();
    let mut root = schema.clone();
    if let Some(map) = root.as_object_mut() {
        map.remove("$defs");
        map.remove("components");
    }
    refs(&root, &mut todo);
    while let Some(next) = todo.pop_first() {
        if !reached.insert(next.clone()) {
            continue;
        }
        let target = schema
            .pointer(next.strip_prefix('#').unwrap_or(&next))
            .unwrap_or_else(|| panic!("$ref {next} does not resolve"));
        let mut inner = BTreeSet::new();
        refs(target, &mut inner);
        todo.extend(inner);
    }
    for name in [
        "ClientMessage",
        "ServerMessage",
        "Subscription",
        "Hello",
        "Frame",
        "StreamError",
    ] {
        assert!(
            reached.iter().any(|r| r.ends_with(&format!("/{name}"))),
            "the root does not reach {name}: {reached:?}"
        );
    }
}

#[tokio::test]
async fn every_dto_is_in_components() {
    let h = Harness::start("contract-components");
    let doc = served(&h).await;
    let components = doc["components"]["schemas"]
        .as_object()
        .expect("components.schemas");
    let api = ApiSchemas::openapi();
    let declared = api.components.expect("ApiSchemas has components").schemas;
    assert_ne!(declared.len(), 0);
    for name in declared.keys() {
        assert!(
            components.contains_key(name),
            "DTO {name} is missing from components"
        );
    }
    // Every reference in the document resolves.
    let mut all = BTreeSet::new();
    refs(&doc, &mut all);
    for reference in all {
        let pointer = reference.strip_prefix('#').expect("local reference");
        assert!(doc.pointer(pointer).is_some(), "dangling $ref {reference}");
    }
}

#[tokio::test]
async fn api_version_matches_info_version() {
    let h = Harness::start("contract-version");
    let doc = served(&h).await;
    assert!(
        doc["openapi"]
            .as_str()
            .is_some_and(|v| v.starts_with("3.1")),
        "{}",
        doc["openapi"]
    );
    assert_eq!(doc["info"]["version"], API_VERSION);
    assert_eq!(committed("openapi.json")["info"]["version"], API_VERSION);
    let status = h.get("/api/v1/status").await.assert_json(200);
    assert_eq!(status["api_version"], API_VERSION);
}
