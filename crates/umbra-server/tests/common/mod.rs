//! Shared helpers of the `umbra-server` acceptance tests (M5 step 4, ASM-180..ASM-182).
//!
//! `Harness` builds an engine thread with a `ManualWallClock`, confined to a fresh scenario root under
//! `CARGO_TARGET_TMPDIR`, and a `Server` over it. REST tests call `Harness::send`, which runs one
//! request through `Server::router()` with `tower::ServiceExt::oneshot`. The flows at the bottom are
//! used by the AT-6, AT-7, AT-8 and AT-19 tests so that the sequence is written once.
#![expect(
    dead_code,
    reason = "each test binary uses a subset of the shared helpers"
)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

pub mod schema_walk;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Method, Request};
use serde_json::{Value, json};
use tower::ServiceExt;
use umbra_api_types::{Problem, ProblemCode};
use umbra_engine::builtins::standard_registry;
use umbra_engine::handle::{EngineHandle, EngineThread, HandleConfig};
use umbra_engine::{Config, Confinement, ManualWallClock, WallClock};
use umbra_server::{Server, ServerSettings, Shutdown};

/// The scenario directory of the repository's examples.
pub fn examples_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/scenarios")
}

/// An empty directory `CARGO_TARGET_TMPDIR/server-tests/<tag>`.
pub fn fresh_dir(tag: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("server-tests")
        .join(tag);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create test dir");
    dir
}

/// Copies the directory `from` to `to` (files and directories only).
pub fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("create target dir");
    for entry in fs::read_dir(from).expect("read source dir") {
        let entry = entry.expect("dir entry");
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).expect("copy file");
        }
    }
}

/// Copies `examples/scenarios/<name>` to `<root>/<name>`.
pub fn copy_example(root: &Path, name: &str) -> PathBuf {
    let target = root.join(name);
    copy_dir(&examples_dir().join(name), &target);
    target
}

/// The response of one request.
#[derive(Debug, Clone)]
pub struct Reply {
    pub status: u16,
    pub content_type: Option<String>,
    pub body: Vec<u8>,
}

impl Reply {
    /// The body as JSON; fails the test if it is not.
    pub fn json(&self) -> Value {
        serde_json::from_slice(&self.body).unwrap_or_else(|e| {
            panic!(
                "status {}: body is not JSON ({e}): {}",
                self.status,
                String::from_utf8_lossy(&self.body)
            )
        })
    }

    /// The body as text.
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    /// The body as a problem document.
    pub fn problem(&self) -> Problem {
        serde_json::from_slice(&self.body).unwrap_or_else(|e| {
            panic!(
                "status {}: body is not a problem ({e}): {}",
                self.status,
                String::from_utf8_lossy(&self.body)
            )
        })
    }

    /// Asserts a problem response with `code`: its status, the media type, and the `code` field.
    pub fn assert_problem(&self, code: ProblemCode) -> Problem {
        assert_eq!(
            self.status,
            code.status(),
            "wrong status for {code:?}: {}",
            self.text()
        );
        assert!(
            self.content_type
                .as_deref()
                .is_some_and(|t| t.starts_with("application/problem+json")),
            "content type {:?}",
            self.content_type
        );
        let problem = self.problem();
        assert_eq!(problem.code, code, "{}", self.text());
        assert_eq!(problem.status, code.status());
        problem
    }

    /// Asserts a status and returns the JSON body.
    pub fn assert_json(&self, status: u16) -> Value {
        assert_eq!(self.status, status, "{}", self.text());
        self.json()
    }
}

/// An engine thread plus a `Server` over it, confined to a fresh root.
pub struct Harness {
    pub server: Server,
    pub handle: EngineHandle,
    pub shutdown: Shutdown,
    pub settings: ServerSettings,
    /// The scenario root; `walking_skeleton` and `patrol_and_engage` are copied into it.
    pub root: PathBuf,
    thread: Option<EngineThread>,
}

impl Harness {
    /// A server with the default settings.
    pub fn start(tag: &str) -> Self {
        Self::with(tag, |_| {})
    }

    /// A server whose `Config` is adjusted by `configure` after the scenario root is set.
    ///
    /// The handle is configured as `umbra serve` must: confinement to the scenario root and the
    /// configured plugin paths, the configured default tick rate and command queue, and a
    /// `ManualWallClock` per session (ASM-180).
    pub fn with(tag: &str, configure: impl FnOnce(&mut Config)) -> Self {
        let root = fresh_dir(tag);
        copy_example(&root, "walking_skeleton");
        copy_example(&root, "patrol_and_engage");
        let mut config = Config::default();
        config.server.scenario_root.clone_from(&root);
        configure(&mut config);
        let settings = ServerSettings::from_config(&config);
        let mut handle_config = HandleConfig::new(standard_registry().expect("standard registry"));
        handle_config.load.default_tick_rate = settings.default_tick_rate;
        handle_config
            .load
            .extra_plugin_paths
            .clone_from(&settings.plugin_paths);
        handle_config.load.confinement = Some(
            Confinement::new(&settings.server.scenario_root, &settings.plugin_paths)
                .expect("confinement"),
        );
        handle_config.command_capacity = settings.server.command_queue;
        handle_config.wall =
            Arc::new(|| -> Box<dyn WallClock> { Box::new(ManualWallClock::new()) });
        let (handle, thread) = EngineHandle::spawn(handle_config).expect("spawn the engine");
        let shutdown = Shutdown::new();
        let server = Server::new(handle.clone(), settings.clone(), shutdown.clone());
        Self {
            server,
            handle,
            shutdown,
            settings,
            root,
            thread: Some(thread),
        }
    }

    /// Sends one request through the router with the given headers (a loopback `Host` is added
    /// unless `headers` has one).
    pub async fn send_with(
        &self,
        method: Method,
        uri: &str,
        headers: &[(&str, &str)],
        body: Body,
    ) -> Reply {
        dispatch(&self.server, method, uri, headers, body).await
    }

    /// A request without a body.
    pub async fn send(&self, method: Method, uri: &str) -> Reply {
        self.send_with(method, uri, &[], Body::empty()).await
    }

    /// A request with a JSON body and `Content-Type: application/json`.
    pub async fn send_json(&self, method: Method, uri: &str, body: &Value) -> Reply {
        self.send_with(
            method,
            uri,
            &[("content-type", "application/json")],
            Body::from(body.to_string()),
        )
        .await
    }

    pub async fn get(&self, uri: &str) -> Reply {
        self.send(Method::GET, uri).await
    }

    pub async fn post(&self, uri: &str) -> Reply {
        self.send(Method::POST, uri).await
    }

    pub async fn post_json(&self, uri: &str, body: &Value) -> Reply {
        self.send_json(Method::POST, uri, body).await
    }

    /// `POST /api/v1/scenario` with this JSON body.
    pub async fn load_body(&self, body: &Value) -> Reply {
        self.post_json("/api/v1/scenario", body).await
    }

    /// Loads `walking_skeleton` (30 Hz, 7200 ticks) with the given clock mode name; expects 201.
    pub async fn load_skeleton(&self, clock: &str) -> Value {
        self.load_body(&json!({
            "path": "walking_skeleton/scenario.toml",
            "clock": {"mode": clock},
        }))
        .await
        .assert_json(201)
    }

    /// `POST /api/v1/run/<action>`.
    pub async fn run_command(&self, action: &str) -> Reply {
        self.post(&format!("/api/v1/run/{action}")).await
    }

    /// `POST /api/v1/run/step`.
    pub async fn step(&self, count: u64) -> Reply {
        self.post_json("/api/v1/run/step", &json!({"count": count}))
            .await
    }

    /// `GET /api/v1/entities` with a raw query string (without the `?`).
    pub async fn entities(&self, query: &str) -> Reply {
        let uri = if query.is_empty() {
            "/api/v1/entities".to_string()
        } else {
            format!("/api/v1/entities?{query}")
        };
        self.get(&uri).await
    }

    /// Starts a JSON request in the background (for a request that stays pending, such as a long
    /// step); join the handle to get the reply.
    pub fn send_json_detached(
        &self,
        method: Method,
        uri: &'static str,
        body: Value,
    ) -> tokio::task::JoinHandle<Reply> {
        let server = self.server.clone();
        tokio::spawn(async move {
            dispatch(
                &server,
                method,
                uri,
                &[("content-type", "application/json")],
                Body::from(body.to_string()),
            )
            .await
        })
    }

    /// Ends the engine thread, as a crashed or shut-down engine would be seen by the server.
    pub fn stop_engine(&mut self) {
        if let Some(thread) = self.thread.take() {
            let _ = thread.shutdown();
        }
    }

    /// Binds `127.0.0.1:0`, serves in the background and returns the address.
    pub async fn serve_on_loopback(&self) -> std::net::SocketAddr {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("local address");
        let server = self.server.clone();
        tokio::spawn(async move { server.serve(listener).await });
        addr
    }
}

/// Runs one request through `server.router()` (the one place requests are built).
async fn dispatch(
    server: &Server,
    method: Method,
    uri: &str,
    headers: &[(&str, &str)],
    body: Body,
) -> Reply {
    let mut request = Request::builder().method(method).uri(uri);
    if !headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("host")) {
        request = request.header("host", "127.0.0.1:7878");
    }
    for (key, value) in headers {
        request = request.header(*key, *value);
    }
    let request = request.body(body).expect("request");
    let response = server
        .router()
        .oneshot(request)
        .await
        .expect("the router is infallible");
    let status = response.status().as_u16();
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read the body")
        .to_vec();
    Reply {
        status,
        content_type,
        body,
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        self.shutdown.trigger();
        if let Some(thread) = self.thread.take() {
            let _ = thread.shutdown();
        }
    }
}

/// One pass over the AT-6, AT-7 and AT-8 routes (status, failed and successful load, run control,
/// entities, spawn, remove, templates, components, catalog, validate, plugins, unload), returning
/// every response in order.
///
/// The AT-19 test `no_response_contains_an_absolute_path_outside_the_root` scans the bodies this
/// returns, so the flows are written once.
pub async fn walkthrough(h: &Harness) -> Vec<Reply> {
    let mut replies = Vec::new();
    // Each step names the status it must give (or the problem code behind a 4xx), so a router that
    // answers 404 or 503 to everything fails here and not only in a leak scan.
    let load = json!({"path": "walking_skeleton/scenario.toml", "clock": {"mode": "stepped"}});
    let spawn =
        json!({"template": "truck.cargo", "lat_deg": 52.0, "lon_deg": 4.0, "name": "walk-1"});
    let validate = json!({"name": "probe", "document": "schema_version = 1\n"});
    let steps: Vec<(Reply, u16, Option<ProblemCode>)> = vec![
        (h.get("/api/v1/status").await, 200, None),
        (
            h.get("/api/v1/scenario").await,
            409,
            Some(ProblemCode::NoScenario),
        ),
        (
            h.get("/api/v1/entities").await,
            409,
            Some(ProblemCode::NoScenario),
        ),
        (h.get("/api/v1/catalog/behavior").await, 200, None),
        (h.get("/api/v1/plugins").await, 200, None),
        (
            h.post_json("/api/v1/behaviors/validate", &validate).await,
            200,
            None,
        ),
        (
            h.load_body(&json!({"path": "missing/scenario.toml"})).await,
            422,
            Some(ProblemCode::ScenarioInvalid),
        ),
        (
            h.load_body(&json!({"path": "../outside.toml"})).await,
            403,
            Some(ProblemCode::PathNotAllowed),
        ),
        (
            h.load_body(&json!({"path": "walking_skeleton"})).await,
            422,
            Some(ProblemCode::ValidationFailed),
        ),
        (h.load_body(&load).await, 201, None),
        (h.get("/api/v1/scenario").await, 200, None),
        (h.get("/api/v1/status").await, 200, None),
        (h.get("/api/v1/entities").await, 200, None),
        (h.get("/api/v1/entities/1").await, 200, None),
        (h.get("/api/v1/templates").await, 200, None),
        (h.get("/api/v1/templates/truck.cargo").await, 200, None),
        (h.get("/api/v1/components").await, 200, None),
        (h.get("/api/v1/catalog/behavior").await, 200, None),
        (h.run_command("start").await, 200, None),
        (h.step(10).await, 200, None),
        (h.post_json("/api/v1/entities", &spawn).await, 201, None),
        (h.get("/api/v1/entities").await, 200, None),
        (
            h.send(Method::DELETE, "/api/v1/entities/4").await,
            204,
            None,
        ),
        (h.run_command("stop").await, 200, None),
        (h.send(Method::DELETE, "/api/v1/scenario").await, 204, None),
    ];
    for (i, (reply, status, code)) in steps.into_iter().enumerate() {
        assert_eq!(
            reply.status,
            status,
            "walkthrough step {i}: {}",
            reply.text()
        );
        if let Some(code) = code {
            reply.assert_problem(code);
        }
        replies.push(reply);
    }
    replies
}

/// Copies `walking_skeleton` to `<root>/long` with a run length of 100000 s (3 million ticks), so a
/// `step` of the maximum size stays pending for a while. Returns the scenario path relative to the
/// root.
pub fn write_long_scenario(root: &Path) -> String {
    let long = root.join("long");
    copy_dir(&examples_dir().join("walking_skeleton"), &long);
    let file = long.join("scenario.toml");
    let text = fs::read_to_string(&file).expect("read scenario");
    assert!(text.contains("duration_s = 240.0"));
    fs::write(
        &file,
        text.replace("duration_s = 240.0", "duration_s = 100000.0"),
    )
    .expect("write scenario");
    "long/scenario.toml".to_string()
}

/// Copies `walking_skeleton` to `<root>/<name>` and replaces the first occurrence of `from` in its
/// scenario file with `to`. Returns the scenario path relative to the root.
pub fn write_scenario_variant(root: &Path, name: &str, from: &str, to: &str) -> String {
    let variant = root.join(name);
    copy_dir(&examples_dir().join("walking_skeleton"), &variant);
    let file = variant.join("scenario.toml");
    let text = fs::read_to_string(&file).expect("read scenario");
    assert!(text.contains(from), "{from} not in the example scenario");
    fs::write(&file, text.replacen(from, to, 1)).expect("write scenario");
    format!("{name}/scenario.toml")
}

// ---- WebSocket client (tokio-tungstenite, loopback only) ----

type WsStream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// What a stream client received.
#[derive(Debug)]
pub enum Incoming {
    /// A text message parsed as JSON.
    Json(Value),
    /// A close frame: code and reason.
    Close(u16, String),
    /// The connection ended without a close frame.
    Ended,
}

/// A WebSocket client of `/api/v1/stream`. Waits are bounded by a 30 s hang guard (a failing test
/// instead of a hung one; never part of an assertion).
pub struct WsClient {
    ws: WsStream,
}

const HANG_GUARD: std::time::Duration = std::time::Duration::from_secs(30);

impl WsClient {
    /// Connects to `/api/v1/stream`; fails the test if the upgrade is refused.
    pub async fn connect(addr: std::net::SocketAddr) -> Self {
        match Self::try_connect(addr, "", &[]).await {
            Ok(client) => client,
            Err(reply) => panic!("upgrade refused: {} {}", reply.status, reply.text()),
        }
    }

    /// Connects to `/api/v1/stream<query>` (`query` includes the `?`) with extra request headers.
    /// A refused upgrade is returned as the HTTP reply.
    pub async fn try_connect(
        addr: std::net::SocketAddr,
        query: &str,
        headers: &[(&str, &str)],
    ) -> Result<Self, Reply> {
        use tokio_tungstenite::tungstenite::client::IntoClientRequest;
        use tokio_tungstenite::tungstenite::http::{HeaderName, HeaderValue};
        let mut request = format!("ws://{addr}/api/v1/stream{query}")
            .into_client_request()
            .expect("client request");
        for (key, value) in headers {
            request.headers_mut().insert(
                HeaderName::from_bytes(key.as_bytes()).expect("header name"),
                HeaderValue::from_str(value).expect("header value"),
            );
        }
        match tokio_tungstenite::connect_async(request).await {
            Ok((ws, _)) => Ok(Self { ws }),
            // tungstenite returns only the body bytes that arrived with the head, so the refusal is
            // read again over a raw socket by Content-Length.
            Err(tokio_tungstenite::tungstenite::Error::Http(_)) => {
                Err(raw_upgrade(addr, query, headers).await)
            }
            Err(other) => panic!("connect failed: {other}"),
        }
    }

    /// Sends a JSON text message.
    pub async fn send_json(&mut self, message: &Value) {
        self.send_text(message.to_string()).await;
    }

    /// Sends a text message as is.
    pub async fn send_text(&mut self, text: String) {
        use futures_util::SinkExt;
        use tokio_tungstenite::tungstenite::Message;
        self.ws.send(Message::text(text)).await.expect("send");
    }

    /// The next JSON, close or end (pings and pongs are skipped).
    pub async fn recv(&mut self) -> Incoming {
        use futures_util::StreamExt;
        use tokio_tungstenite::tungstenite::Message;
        loop {
            let next = tokio::time::timeout(HANG_GUARD, self.ws.next())
                .await
                .expect("hang guard: nothing arrived within 30 s");
            match next {
                Some(Ok(Message::Text(text))) => {
                    return Incoming::Json(
                        serde_json::from_str(text.as_str()).expect("server text is JSON"),
                    );
                }
                Some(Ok(Message::Close(frame))) => {
                    return match frame {
                        Some(f) => Incoming::Close(u16::from(f.code), f.reason.to_string()),
                        None => Incoming::Close(1005, String::new()),
                    };
                }
                Some(Ok(Message::Ping(_) | Message::Pong(_))) => {}
                Some(Ok(other)) => panic!("unexpected message {other:?}"),
                Some(Err(_)) | None => return Incoming::Ended,
            }
        }
    }

    /// The next message, which must be JSON.
    pub async fn next_json(&mut self) -> Value {
        match self.recv().await {
            Incoming::Json(value) => value,
            other => panic!("expected a JSON message, got {other:?}"),
        }
    }

    /// The `hello`; fails the test if the first message is something else.
    pub async fn hello(&mut self) -> Value {
        let hello = self.next_json().await;
        assert_eq!(hello["type"], "hello", "{hello}");
        hello
    }

    /// Sends `{"type":"subscribe", ..fields}`.
    pub async fn subscribe(&mut self, fields: Value) {
        let mut message = fields;
        message["type"] = Value::from("subscribe");
        self.send_json(&message).await;
    }

    /// Reads frames until `accept` is true for one, and returns it. Other frames are skipped.
    pub async fn frame_until(&mut self, accept: impl Fn(&Value) -> bool) -> Value {
        loop {
            let message = self.next_json().await;
            if message["type"] == "frame" && accept(&message) {
                return message;
            }
        }
    }

    /// The next `frame` message (an `error` or `hello` fails the test).
    pub async fn next_frame(&mut self) -> Value {
        let message = self.next_json().await;
        assert_eq!(message["type"], "frame", "{message}");
        message
    }

    /// Closes the client side.
    pub async fn close(mut self) {
        let _ = self.ws.close(None).await;
    }
}

/// Sends a WebSocket upgrade request on a raw socket and reads the whole HTTP response by
/// `Content-Length` (for the refusals of the upgrade).
async fn raw_upgrade(addr: std::net::SocketAddr, query: &str, headers: &[(&str, &str)]) -> Reply {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut request = format!(
        "GET /api/v1/stream{query} HTTP/1.1\r\nHost: {addr}\r\nUpgrade: websocket\r\n\
         Connection: Upgrade\r\nSec-WebSocket-Version: 13\r\n\
         Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n"
    );
    for (key, value) in headers {
        request.push_str(key);
        request.push_str(": ");
        request.push_str(value);
        request.push_str("\r\n");
    }
    request.push_str("\r\n");
    let mut stream = tokio::net::TcpStream::connect(addr).await.expect("connect");
    stream.write_all(request.as_bytes()).await.expect("write");
    let mut raw = Vec::new();
    let (head_end, length) = loop {
        let mut chunk = [0_u8; 4096];
        let n = tokio::time::timeout(HANG_GUARD, stream.read(&mut chunk))
            .await
            .expect("hang guard")
            .expect("read");
        assert!(n > 0, "closed before the response head ended");
        raw.extend_from_slice(&chunk[..n]);
        if let Some(i) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&raw[..i]).to_lowercase();
            let length = head
                .lines()
                .find_map(|l| l.strip_prefix("content-length:"))
                .and_then(|v| v.trim().parse::<usize>().ok())
                .unwrap_or(0);
            break (i + 4, length);
        }
    };
    while raw.len() < head_end + length {
        let mut chunk = [0_u8; 4096];
        let n = tokio::time::timeout(HANG_GUARD, stream.read(&mut chunk))
            .await
            .expect("hang guard")
            .expect("read");
        assert!(n > 0, "closed before the body ended");
        raw.extend_from_slice(&chunk[..n]);
    }
    let head = String::from_utf8_lossy(&raw[..head_end]).into_owned();
    let status = head
        .split(' ')
        .nth(1)
        .and_then(|s| s.parse().ok())
        .expect("status line");
    let content_type = head.lines().find_map(|l| {
        let (k, v) = l.split_once(':')?;
        k.eq_ignore_ascii_case("content-type")
            .then(|| v.trim().to_string())
    });
    Reply {
        status,
        content_type,
        body: raw[head_end..head_end + length].to_vec(),
    }
}
