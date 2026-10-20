//! AT-19 (WI-11 and WI-15): the `Host` allow-list and the WebSocket `Origin` check. Requirements:
//! REQ-X-8, NFR-4, ADR 0026 (amended, D-b), ADR 0029 (`allowed_hosts`, `allowed_origins`).
//!
//! The `Host` rows go through `Server::router()` (WI-11); the `Origin` rows upgrade a real WebSocket
//! (WI-15).

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod common;

use axum::body::Body;
use axum::http::Method;
use common::{Harness, WsClient};
use umbra_api_types::ProblemCode;

async fn status_with_host(h: &Harness, host: &str) -> common::Reply {
    h.send_with(
        Method::GET,
        "/api/v1/status",
        &[("host", host)],
        Body::empty(),
    )
    .await
}

#[tokio::test]
async fn loopback_hosts_on_any_port_allowed() {
    let h = Harness::start("host-loopback");
    for host in [
        "localhost",
        "localhost:3000",
        "127.0.0.1",
        "127.0.0.1:1",
        "127.0.0.2:8080",
        "[::1]",
        "[::1]:7878",
    ] {
        assert_eq!(status_with_host(&h, host).await.status, 200, "Host: {host}");
    }
}

#[tokio::test]
async fn bound_address_allowed() {
    let h = Harness::with("host-bound", |config| {
        config.server.bind = "192.0.2.7:7878".parse().expect("address");
    });
    assert_eq!(status_with_host(&h, "192.0.2.7:7878").await.status, 200);
    status_with_host(&h, "192.0.2.8:7878")
        .await
        .assert_problem(ProblemCode::HostNotAllowed);
}

#[tokio::test]
async fn foreign_host_is_403_host_not_allowed() {
    let h = Harness::start("host-foreign");
    for host in [
        "evil.example",
        "evil.example:7878",
        "localhost.evil.example",
        "10.0.0.5:7878",
    ] {
        status_with_host(&h, host)
            .await
            .assert_problem(ProblemCode::HostNotAllowed);
    }
    // The check is the outermost layer: also an unknown route and a mutating route.
    let unknown = h
        .send_with(
            Method::GET,
            "/nope",
            &[("host", "evil.example")],
            Body::empty(),
        )
        .await;
    unknown.assert_problem(ProblemCode::HostNotAllowed);
    let post = h
        .send_with(
            Method::POST,
            "/api/v1/run/start",
            &[("host", "evil.example")],
            Body::empty(),
        )
        .await;
    post.assert_problem(ProblemCode::HostNotAllowed);
}

/// One raw HTTP/1.1 request with this request line target and `Host`; the whole response as text.
async fn raw_request(addr: std::net::SocketAddr, target: &str, host: &str) -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = tokio::net::TcpStream::connect(addr).await.expect("connect");
    let request = format!("GET {target} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).await.expect("write");
    let mut text = String::new();
    tokio::time::timeout(
        std::time::Duration::from_secs(30),
        stream.read_to_string(&mut text),
    )
    .await
    .expect("hang guard")
    .expect("read");
    text
}

#[tokio::test]
async fn absolute_form_authority_must_be_allowed_too() {
    let h = Harness::start("host-absolute-form");
    let addr = h.serve_on_loopback().await;
    // A loopback `Host` does not excuse a foreign authority in the request target.
    let refused = raw_request(addr, "http://evil.example/api/v1/status", "localhost").await;
    assert!(refused.starts_with("HTTP/1.1 403"), "{refused}");
    assert!(refused.contains("host_not_allowed"), "{refused}");
    // Control: the same shape with an allowed authority is served.
    let target = format!("http://{addr}/api/v1/status");
    let served = raw_request(addr, &target, "localhost").await;
    assert!(served.starts_with("HTTP/1.1 200"), "{served}");
}

#[tokio::test]
async fn configured_allowed_hosts_accepted() {
    let h = Harness::with("host-configured", |config| {
        config.server.allowed_hosts = vec!["umbra.example".to_string()];
    });
    for host in ["umbra.example", "umbra.example:8080"] {
        assert_eq!(status_with_host(&h, host).await.status, 200, "Host: {host}");
    }
    status_with_host(&h, "other.example")
        .await
        .assert_problem(ProblemCode::HostNotAllowed);
}

#[tokio::test]
async fn ws_origin_loopback_any_port_allowed() {
    let h = Harness::start("origin-loopback");
    let addr = h.serve_on_loopback().await;
    for origin in [
        "http://localhost:5173",
        "http://127.0.0.1:3000",
        "http://[::1]:8080",
    ] {
        let mut ws = match WsClient::try_connect(addr, "", &[("origin", origin)]).await {
            Ok(ws) => ws,
            Err(reply) => panic!("Origin {origin} refused: {} {}", reply.status, reply.text()),
        };
        assert_eq!(ws.hello().await["protocol"], "umbra.stream.v1");
    }
}

#[tokio::test]
async fn ws_without_origin_allowed() {
    let h = Harness::start("origin-none");
    let addr = h.serve_on_loopback().await;
    let mut ws = WsClient::connect(addr).await;
    ws.hello().await;
}

#[tokio::test]
async fn ws_foreign_origin_is_403_origin_not_allowed() {
    let h = Harness::start("origin-foreign");
    let addr = h.serve_on_loopback().await;
    for origin in ["https://evil.example", "http://localhost.evil.example:3000"] {
        match WsClient::try_connect(addr, "", &[("origin", origin)]).await {
            Err(reply) => {
                reply.assert_problem(ProblemCode::OriginNotAllowed);
            }
            Ok(_) => panic!("Origin {origin} was accepted"),
        }
    }
}

#[tokio::test]
async fn ws_configured_origin_allowed() {
    let h = Harness::with("origin-configured", |config| {
        config.server.allowed_origins = vec!["https://ops.example".to_string()];
    });
    let addr = h.serve_on_loopback().await;
    let mut ws = match WsClient::try_connect(addr, "", &[("origin", "https://ops.example")]).await {
        Ok(ws) => ws,
        Err(reply) => panic!("the configured origin was refused: {}", reply.status),
    };
    ws.hello().await;
    // The match is exact: another port of the same host is not the configured origin.
    match WsClient::try_connect(addr, "", &[("origin", "https://ops.example:8443")]).await {
        Err(reply) => {
            reply.assert_problem(ProblemCode::OriginNotAllowed);
        }
        Ok(_) => panic!("a different port was accepted"),
    }
}
