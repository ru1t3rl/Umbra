//! WI-11: the pieces around the router: the handle configuration built from the settings, the
//! `Host` header being required, and `serve` ending at the grace deadline (ASM-197).

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod common;

use std::time::Duration;

use axum::body::Body;
use axum::http::Request;
use common::{Harness, fresh_dir};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tower::ServiceExt;
use umbra_api_types::ProblemCode;
use umbra_engine::Config;
use umbra_server::ServerSettings;

#[test]
fn handle_config_carries_the_settings() {
    let root = fresh_dir("foundation-config");
    let plugins = fresh_dir("foundation-config-plugins");
    let mut config = Config::default();
    config.server.scenario_root.clone_from(&root);
    config.server.command_queue = 7;
    config.tick_rate = Some(umbra_engine::TickRate::new(25).expect("rate"));
    config.plugin_paths = Some(vec![plugins.clone()]);
    let settings = ServerSettings::from_config(&config);
    let handle = settings.handle_config().expect("handle configuration");
    assert_eq!(handle.command_capacity, 7);
    assert_eq!(handle.load.default_tick_rate.hz(), 25);
    assert_eq!(handle.load.extra_plugin_paths, vec![plugins.clone()]);
    let confinement = handle.load.confinement.expect("confined");
    assert_eq!(
        confinement.root,
        root.canonicalize().expect("canonical root")
    );
    assert_eq!(
        confinement.plugin_roots,
        vec![plugins.canonicalize().expect("canonical plugins")]
    );
}

#[test]
fn handle_config_refuses_a_missing_scenario_root() {
    let mut config = Config::default();
    config.server.scenario_root = fresh_dir("foundation-missing-root").join("no_such_dir");
    let settings = ServerSettings::from_config(&config);
    assert!(settings.handle_config().is_err());
    assert!(settings.confinement().is_err());
}

#[tokio::test]
async fn a_request_without_a_host_header_is_refused() {
    let h = Harness::start("foundation-no-host");
    let request = Request::builder()
        .uri("/api/v1/status")
        .body(Body::empty())
        .expect("request");
    let response = h.server.router().oneshot(request).await.expect("response");
    assert_eq!(
        response.status().as_u16(),
        ProblemCode::HostNotAllowed.status()
    );
}

/// One HTTP/1.1 request on a fresh connection; the whole response as text.
async fn get_over_tcp(addr: std::net::SocketAddr, path: &str) -> String {
    let mut stream = tokio::net::TcpStream::connect(addr).await.expect("connect");
    let request = format!("GET {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).await.expect("write");
    let mut text = String::new();
    tokio::time::timeout(Duration::from_secs(30), stream.read_to_string(&mut text))
        .await
        .expect("hang guard")
        .expect("read");
    text
}

#[tokio::test]
async fn serve_answers_over_tcp_and_ends_at_the_grace_deadline() {
    let h = Harness::with("foundation-serve", |config| {
        config.server.shutdown_grace = Duration::from_millis(200);
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("address");
    let serving = tokio::spawn(h.server.clone().serve(listener));

    let response = get_over_tcp(addr, "/api/v1/status").await;
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert!(response.contains("\"api_version\""), "{response}");

    // A connection that never finishes its request is an open request at shutdown.
    let mut stuck = tokio::net::TcpStream::connect(addr).await.expect("connect");
    stuck
        .write_all(b"GET /api/v1/status HTTP/1.1\r\nHost: 127.0.0.1\r\n")
        .await
        .expect("write");
    tokio::task::yield_now().await;
    h.shutdown.trigger();
    let ended = tokio::time::timeout(Duration::from_secs(30), serving)
        .await
        .expect("serve ends after the grace period, not when the stuck request does")
        .expect("join");
    assert!(ended.is_ok(), "{ended:?}");
    drop(stuck);
}

#[tokio::test]
async fn the_served_document_names_the_routes_of_wi_11() {
    let h = Harness::start("foundation-openapi");
    let document = h.get("/api/v1/openapi.json").await.assert_json(200);
    assert_eq!(document["info"]["version"], umbra_api_types::API_VERSION);
    let mut ids: Vec<String> = document["paths"]
        .as_object()
        .expect("paths")
        .values()
        .flat_map(|item| item.as_object().expect("path item").values())
        .filter_map(|operation| operation["operationId"].as_str().map(str::to_owned))
        .collect();
    ids.sort();
    for id in [
        "getStatus",
        "loadScenario",
        "getScenario",
        "unloadScenario",
        "startRun",
        "pauseRun",
        "resumeRun",
        "stopRun",
        "stepRun",
        "setClock",
        "getOpenApi",
    ] {
        assert!(ids.iter().any(|known| known == id), "{id} in {ids:?}");
    }
    // Every operation documents the problem responses all routes share, and the schemas of the
    // wire types are components.
    let status = &document["paths"]["/api/v1/status"]["get"]["responses"];
    for code in ["200", "403", "405", "500", "503"] {
        assert!(status.get(code).is_some(), "{code} in {status}");
    }
    assert!(document["components"]["schemas"].get("Problem").is_some());
    // The operations whose handlers arrive later still document their query parameters.
    let names = |path: &str| -> Vec<String> {
        document["paths"][path]["get"]["parameters"]
            .as_array()
            .unwrap_or_else(|| panic!("{path} lists parameters: {document}"))
            .iter()
            .map(|p| {
                assert_eq!(p["in"], "query", "{p}");
                p["name"].as_str().expect("name").to_owned()
            })
            .collect()
    };
    let mut entities = names("/api/v1/entities");
    entities.sort();
    let mut expected = vec![
        "after_id",
        "limit",
        "ids",
        "name",
        "force",
        "group",
        "template",
        "ownership",
        "min_lat_deg",
        "min_lon_deg",
        "max_lat_deg",
        "max_lon_deg",
    ];
    expected.sort_unstable();
    assert_eq!(entities, expected);
    assert_eq!(names("/api/v1/components"), ["kind"]);
    let entity_id = &document["paths"]["/api/v1/entities/{id}"]["get"]["parameters"];
    assert_eq!(entity_id[0]["in"], "path", "{entity_id}");
}
