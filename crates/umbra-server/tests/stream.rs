//! AT-9 (WI-15): the WebSocket stream over a real loopback socket. Requirements: REQ-X-6,
//! ADR 0027 (M1, M2, M11, m9, M12). Clients are `tokio-tungstenite`; the server serves on
//! `127.0.0.1:0`. Engines run stepped with a `ManualWallClock`, so ticks happen only when a test
//! steps; waits are bounded by a hang guard, never asserted.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod common;

use common::schema_walk;
use common::{Harness, Incoming, WsClient};
use serde_json::{Value, json};
use umbra_api_types::ProblemCode;

const SKELETON: &str = "walking_skeleton/scenario.toml";
const PATROL: &str = "patrol_and_engage/scenario.toml";

/// A started, stepped-clock server for `scenario`, a client that has read `hello`.
async fn connected(tag: &str, scenario: &str) -> (Harness, WsClient) {
    connected_with(tag, scenario, |_| {}).await
}

/// [`connected`] with the configuration adjusted by `configure`.
async fn connected_with(
    tag: &str,
    scenario: &str,
    configure: impl FnOnce(&mut umbra_engine::Config),
) -> (Harness, WsClient) {
    let h = Harness::with(tag, configure);
    h.load_body(&json!({"path": scenario, "clock": {"mode": "stepped"}}))
        .await
        .assert_json(201);
    h.run_command("start").await.assert_json(200);
    let addr = h.serve_on_loopback().await;
    let mut ws = WsClient::connect(addr).await;
    ws.hello().await;
    (h, ws)
}

fn entity_ids(frame: &Value) -> Vec<u64> {
    frame["entities"]
        .as_array()
        .expect("entities")
        .iter()
        .map(|e| e["id"].as_u64().expect("id"))
        .collect()
}

/// Reads frames until one has `tick`, and returns the events of all of them (frames the server
/// skipped carry their events into the next, so nothing is lost on the way).
async fn events_until(ws: &mut WsClient, tick: u64) -> Vec<Value> {
    let mut events = Vec::new();
    loop {
        let frame = ws.frame_until(|_| true).await;
        events.extend(frame["events"].as_array().expect("events").iter().cloned());
        if frame["tick"] == tick {
            return events;
        }
    }
}

fn fire_ticks(events: &[Value]) -> Vec<u64> {
    events
        .iter()
        .filter(|e| e["kind"] == "fire")
        .map(|e| e["tick"].as_u64().expect("tick"))
        .collect()
}

#[tokio::test]
async fn hello_reports_settings() {
    let h = Harness::start("stream-hello");
    let addr = h.serve_on_loopback().await;
    let mut ws = WsClient::connect(addr).await;
    let hello = ws.hello().await;
    assert_eq!(hello["protocol"], "umbra.stream.v1");
    assert_eq!(hello["api_version"], umbra_api_types::API_VERSION);
    assert_eq!(hello["encoding"], "json");
    assert_eq!(
        hello["settings"],
        json!({
            "default_rate_hz": 30.0,
            "max_rate_hz": 60.0,
            "queue_frames": 4,
            "max_events_per_frame": 4096,
            "send_timeout_s": 10.0,
            "max_message_bytes": 65536,
            "max_entity_ids": 10000,
        })
    );
}

#[tokio::test]
async fn subscribe_gets_a_primed_control_frame_at_once() {
    let (h, mut ws) = connected("stream-primed", SKELETON).await;
    h.run_command("pause").await.assert_json(200);
    ws.subscribe(json!({})).await;
    let frame = ws.next_frame().await;
    assert_eq!(frame["kind"], "snapshot");
    assert_eq!(frame["cause"], "control");
    assert_eq!(frame["tick"], 0);
    assert_eq!(frame["run"]["state"], "paused");
    assert_eq!(entity_ids(&frame), [1, 2, 3]);
}

#[tokio::test]
async fn subscribe_while_idle_gets_an_idle_frame() {
    let h = Harness::start("stream-idle");
    let addr = h.serve_on_loopback().await;
    let mut ws = WsClient::connect(addr).await;
    ws.hello().await;
    ws.subscribe(json!({})).await;
    let frame = ws.next_frame().await;
    assert_eq!(frame["cause"], "control");
    assert_eq!(frame["run"]["state"], "idle");
    assert_eq!(entity_ids(&frame), Vec::<u64>::new());
}

#[tokio::test]
async fn control_frame_after_load_lists_all_entities_without_entity_created_events() {
    let h = Harness::start("stream-load");
    let addr = h.serve_on_loopback().await;
    let mut ws = WsClient::connect(addr).await;
    ws.hello().await;
    ws.subscribe(json!({})).await;
    ws.next_frame().await;
    h.load_skeleton("stepped").await;
    let frame = ws.frame_until(|f| f["run"]["state"] == "ready").await;
    assert_eq!(frame["cause"], "control");
    assert_eq!(entity_ids(&frame), [1, 2, 3]);
    assert!(
        frame["events"].as_array().expect("events").is_empty(),
        "the initial load emits no entity_created: {frame}"
    );
}

#[tokio::test]
async fn frame_after_step_has_that_tick() {
    let (h, mut ws) = connected("stream-step", SKELETON).await;
    ws.subscribe(json!({})).await;
    ws.next_frame().await;
    h.step(10).await.assert_json(200);
    let frame = ws.frame_until(|f| f["tick"] == 10).await;
    assert_eq!(frame["cause"], "tick");
    assert_eq!(
        frame["session"],
        h.get("/api/v1/status").await.json()["run"]["session"]
    );
}

#[tokio::test]
async fn filters_bbox_ids_event_kinds() {
    let (h, mut ws) = connected("stream-filters", SKELETON).await;
    // Entity ids.
    ws.subscribe(json!({"filter": {"entity_ids": [3, 1]}}))
        .await;
    assert_eq!(entity_ids(&ws.next_frame().await), [1, 3], "sorted by id");
    // Bounding box (base-1 is at 52.00 N).
    ws.subscribe(json!({"filter": {"bbox": {
        "min_lat_deg": 51.99, "min_lon_deg": 3.9, "max_lat_deg": 52.01, "max_lon_deg": 4.1
    }}}))
    .await;
    assert_eq!(entity_ids(&ws.next_frame().await), [1]);
    // Event kinds: a filter that excludes `fire` and one that keeps it, on the patrol scenario.
    h.load_body(&json!({"path": PATROL, "clock": {"mode": "stepped"}}))
        .await
        .assert_json(201);
    h.run_command("start").await.assert_json(200);
    ws.subscribe(json!({"filter": {"event_kinds": ["custom"]}}))
        .await;
    ws.frame_until(|f| f["run"]["state"] == "running").await;
    h.step(1200).await.assert_json(200);
    let none = events_until(&mut ws, 1200).await;
    assert!(
        fire_ticks(&none).is_empty(),
        "fire is filtered out: {none:?}"
    );
    ws.subscribe(json!({"filter": {"event_kinds": ["fire"]}}))
        .await;
    ws.frame_until(|f| f["cause"] == "control").await;
    h.step(1300).await.assert_json(200);
    let kept = events_until(&mut ws, 2500).await;
    let kinds: Vec<&str> = kept
        .iter()
        .map(|e| e["kind"].as_str().expect("kind"))
        .collect();
    assert!(
        !kinds.is_empty() && kinds.iter().all(|k| *k == "fire"),
        "{kinds:?}"
    );
}

#[tokio::test]
async fn fire_events_reach_a_subscriber() {
    let (h, mut ws) = connected("stream-fire", PATROL).await;
    ws.subscribe(json!({})).await;
    ws.next_frame().await;
    let mut fires = Vec::new();
    for chunk in 1..=12_u64 {
        h.step(100).await.assert_json(200);
        fires.extend(fire_ticks(&events_until(&mut ws, chunk * 100).await));
    }
    assert!(
        fires.contains(&1130),
        "blue-1 fires first at tick 1130 (example README): {fires:?}"
    );
}

#[tokio::test]
async fn events_of_skipped_frames_are_carried() {
    // A queue of one frame and a single step of 1200 ticks without reading force drops.
    let (h, mut ws) = connected_with("stream-carried", PATROL, |c| {
        c.server.stream.queue_frames = 1;
    })
    .await;
    ws.subscribe(json!({})).await;
    ws.next_frame().await;
    h.step(1200).await.assert_json(200);
    let mut events = Vec::new();
    let mut max_dropped = 0;
    loop {
        let frame = ws.frame_until(|_| true).await;
        events.extend(frame["events"].as_array().expect("events").iter().cloned());
        max_dropped = max_dropped.max(frame["dropped"].as_u64().expect("dropped"));
        if frame["tick"] == 1200 {
            break;
        }
    }
    assert!(max_dropped > 0, "frames were dropped on the way");
    let fires = fire_ticks(&events);
    assert!(fires.contains(&1130), "{fires:?}");
}

#[tokio::test]
async fn new_subscribe_replaces_the_old() {
    let (h, mut ws) = connected("stream-replace", SKELETON).await;
    ws.subscribe(json!({"filter": {"entity_ids": [1]}})).await;
    assert_eq!(entity_ids(&ws.next_frame().await), [1]);
    ws.subscribe(json!({"filter": {"entity_ids": [2, 3]}}))
        .await;
    let replaced = ws.frame_until(|f| entity_ids(f) == [2, 3]).await;
    assert_eq!(
        replaced["cause"], "control",
        "the new subscription is primed"
    );
    h.step(5).await.assert_json(200);
    let next = ws.frame_until(|f| f["tick"] == 5).await;
    assert_eq!(entity_ids(&next), [2, 3], "the old filter is gone");
}

#[tokio::test]
async fn rate_above_configured_max_is_an_error() {
    let (h, mut ws) = connected("stream-rate-max", SKELETON).await;
    ws.subscribe(json!({"max_rate_hz": 61.0})).await;
    let error = ws.next_json().await;
    assert_eq!(error["type"], "error");
    assert_eq!(error["code"], "invalid_subscription");
    // The connection stays open and a valid subscription works.
    ws.subscribe(json!({"max_rate_hz": 60.0})).await;
    assert_eq!(ws.next_frame().await["cause"], "control");
    drop(h);
}

/// Builds the engine builds-per-step probe: a started, stepped server whose stream settings are
/// adjusted by `configure`, one subscriber with `subscription`, then `step(300)`. Returns the number
/// of frames the engine built during the step.
async fn builds_during_300_ticks(
    tag: &str,
    configure: fn(&mut umbra_engine::Config),
    subscription: Value,
) -> u64 {
    let h = Harness::with(tag, configure);
    h.load_skeleton("stepped").await;
    h.run_command("start").await.assert_json(200);
    let addr = h.serve_on_loopback().await;
    let mut ws = WsClient::connect(addr).await;
    ws.hello().await;
    ws.subscribe(subscription).await;
    ws.next_frame().await;
    let before = h.handle.snapshot_builds();
    h.step(300).await.assert_json(200);
    h.handle.snapshot_builds() - before
}

#[tokio::test]
async fn missing_rate_uses_configured_default() {
    // 30 Hz ticks and a configured default of 10 Hz: stride 3, so about 100 tick frames in 300 ticks
    // (a few control or step frames may come on top).
    let builds = builds_during_300_ticks(
        "stream-default-rate",
        |config| {
            config.server.stream.default_rate_hz = 10.0;
            config.server.stream.max_rate_hz = 20.0;
        },
        json!({}),
    )
    .await;
    assert!((100..=105).contains(&builds), "{builds} frames built");
}

#[tokio::test]
async fn stride_follows_tick_rate_and_rate() {
    // Stride = max(1, tick_rate_hz / max_rate_hz) with integer division (30 Hz ticks).
    let cases = [(15.0, 150..=155_u64), (30.0, 300..=305), (60.0, 300..=305)];
    for (rate, expected) in cases {
        let builds = builds_during_300_ticks(
            &format!("stream-stride-{rate}"),
            |_| {},
            json!({"max_rate_hz": rate}),
        )
        .await;
        assert!(
            expected.contains(&builds),
            "rate {rate}: {builds} frames built"
        );
    }
}

#[tokio::test]
async fn oversized_message_closes_1009() {
    let (_h, mut ws) = connected("stream-1009", SKELETON).await;
    ws.send_text(format!(
        "{{\"type\":\"subscribe\",\"pad\":\"{}\"}}",
        "x".repeat(70_000)
    ))
    .await;
    loop {
        match ws.recv().await {
            Incoming::Close(code, _) => {
                assert_eq!(code, 1009);
                break;
            }
            Incoming::Json(_) => {}
            Incoming::Ended => panic!("the connection ended without a 1009 close"),
        }
    }
}

#[tokio::test]
async fn too_many_entity_ids_is_an_error() {
    let (_h, mut ws) = connected("stream-ids", SKELETON).await;
    let ids: Vec<u64> = (1..=10_001).collect();
    ws.subscribe(json!({"filter": {"entity_ids": ids}})).await;
    let error = ws.next_json().await;
    assert_eq!(error["type"], "error");
    assert_eq!(error["code"], "invalid_subscription");
    ws.subscribe(json!({})).await;
    assert_eq!(
        ws.next_frame().await["cause"],
        "control",
        "the connection stays open"
    );
}

#[tokio::test]
async fn connection_limit_is_503_too_many_connections() {
    let h = Harness::with("stream-limit", |config| {
        config.server.stream.max_connections = 1;
    });
    let addr = h.serve_on_loopback().await;
    let mut first = WsClient::connect(addr).await;
    first.hello().await;
    let refused = WsClient::try_connect(addr, "", &[]).await;
    match refused {
        Err(reply) => {
            reply.assert_problem(ProblemCode::TooManyConnections);
        }
        Ok(_) => panic!("the second connection was accepted"),
    }
}

#[tokio::test]
async fn malformed_client_message_gets_error_and_connection_stays() {
    let (_h, mut ws) = connected("stream-malformed", SKELETON).await;
    ws.send_text("this is not json".to_string()).await;
    let invalid = ws.next_json().await;
    assert_eq!(invalid["type"], "error");
    assert_eq!(invalid["code"], "invalid_message");
    ws.send_json(&json!({"type": "bogus"})).await;
    let unknown = ws.next_json().await;
    assert_eq!(unknown["type"], "error");
    assert_eq!(unknown["code"], "unknown_message_type");
    ws.subscribe(json!({})).await;
    assert_eq!(ws.next_frame().await["cause"], "control");
}

#[tokio::test]
async fn msgpack_is_400_unsupported_encoding() {
    let h = Harness::start("stream-msgpack");
    let addr = h.serve_on_loopback().await;
    for query in ["?encoding=msgpack", "?encoding=xml"] {
        match WsClient::try_connect(addr, query, &[]).await {
            Err(reply) => {
                reply.assert_problem(ProblemCode::UnsupportedEncoding);
            }
            Ok(_) => panic!("{query} was accepted"),
        }
    }
}

#[tokio::test]
async fn frames_validate_against_stream_schema() {
    // The committed `api/stream.schema.json` (WI-12) is the contract (REQ-A-4).
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../api/stream.schema.json");
    let schema: Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("api/stream.schema.json"))
            .expect("schema JSON");
    let (h, mut ws) = connected("stream-schema", PATROL).await;
    let addr_hello = {
        // A second connection to see a `hello` and an `error` too.
        let addr = h.serve_on_loopback().await;
        let mut other = WsClient::connect(addr).await;
        let hello = other.hello().await;
        other.send_text("garbage".to_string()).await;
        (hello, other.next_json().await)
    };
    ws.subscribe(json!({"state_hash": true})).await;
    let primed = ws.next_frame().await;
    h.step(1200).await.assert_json(200);
    let tick = ws.frame_until(|f| f["tick"] == 1200).await;
    for message in [&addr_hello.0, &addr_hello.1, &primed, &tick] {
        if let Err(e) = schema_walk::validate(&schema, &schema, message) {
            panic!("{e}\nmessage: {message}");
        }
    }
    // The client message validates too.
    let subscribe =
        json!({"type": "subscribe", "max_rate_hz": 10.0, "filter": {"entity_ids": [1]}});
    schema_walk::validate(&schema, &schema, &subscribe).expect("subscribe conforms");
    // The schema is as strict as the server: an unknown property of `subscribe` is refused.
    let extra = json!({"type": "subscribe", "extra": 1});
    assert!(
        schema_walk::validate(&schema, &schema, &extra).is_err(),
        "subscribe with an unknown property must not conform"
    );
    // The schema is not vacuous: a frame without its required fields does not conform.
    assert!(schema_walk::validate(&schema, &schema, &json!({"type": "frame"})).is_err());
}

#[tokio::test]
async fn seq_increases_and_dropped_is_reported() {
    // A queue of one frame and 20 steps without reading force drops.
    let (h, mut ws) = connected_with("stream-seq", SKELETON, |c| {
        c.server.stream.queue_frames = 1;
    })
    .await;
    ws.subscribe(json!({})).await;
    let mut last = ws.next_frame().await;
    for _ in 0..20 {
        h.step(100).await.assert_json(200);
    }
    let mut total = 0;
    loop {
        let frame = ws.next_frame().await;
        let seq = frame["seq"].as_u64().expect("seq");
        let before = last["seq"].as_u64().expect("seq");
        assert!(seq > before, "seq increases");
        let dropped = frame["dropped"].as_u64().expect("dropped");
        assert_eq!(
            dropped,
            seq - before - 1,
            "frames never sent between {before} and {seq}"
        );
        total += dropped;
        let done = frame["tick"] == 2000;
        last = frame;
        if done {
            break;
        }
    }
    assert!(total > 0, "something was dropped");
    // The cross-check against `Server::stream_stats()` (bus drops plus skipped frames) belongs to
    // WI-16, which implements that probe; WI-15 pins the per-frame identity above.
}

#[tokio::test]
async fn server_shutdown_closes_streams_1001() {
    let (h, mut ws) = connected("stream-shutdown", SKELETON).await;
    ws.subscribe(json!({})).await;
    ws.next_frame().await;
    h.shutdown.trigger();
    loop {
        match ws.recv().await {
            Incoming::Close(code, _) => {
                assert_eq!(code, 1001);
                break;
            }
            Incoming::Json(_) => {}
            Incoming::Ended => panic!("the connection ended without a 1001 close"),
        }
    }
}

#[tokio::test]
async fn a_quiet_stream_closes_1011_when_the_engine_is_gone() {
    // ASM-187 h: with no frames coming, the connection finds the dead engine by asking it.
    let (mut h, mut ws) = connected("stream-engine-gone", SKELETON).await;
    ws.subscribe(json!({})).await;
    ws.next_frame().await;
    h.stop_engine();
    loop {
        match ws.recv().await {
            Incoming::Close(code, reason) => {
                assert_eq!((code, reason.as_str()), (1011, "engine_unavailable"));
                break;
            }
            Incoming::Json(_) => {}
            Incoming::Ended => panic!("the connection ended without a 1011 close"),
        }
    }
}

#[tokio::test]
async fn frames_are_paced_at_the_subscribed_rate() {
    // 5 Hz: two frames are at least 200 ms apart on the server; the client sees at least 150 ms.
    let (h, mut ws) = connected("stream-pacing", SKELETON).await;
    ws.subscribe(json!({"max_rate_hz": 5.0})).await;
    ws.next_frame().await;
    let sent_first = tokio::time::Instant::now();
    h.step(1).await.assert_json(200);
    let second = ws.frame_until(|f| f["tick"] == 1).await;
    assert_eq!(second["tick"], 1);
    assert!(
        sent_first.elapsed() >= std::time::Duration::from_millis(150),
        "the second frame came after {:?}",
        sent_first.elapsed()
    );
}

#[tokio::test]
async fn state_hash_is_in_frames_only_when_asked_and_equals_the_step_hash() {
    let (h, mut ws) = connected("stream-hash", SKELETON).await;
    ws.subscribe(json!({})).await;
    ws.next_frame().await;
    h.step(3).await.assert_json(200);
    let plain = ws.frame_until(|f| f["tick"] == 3).await;
    assert!(plain.get("state_hash").is_none(), "not asked: {plain}");
    ws.subscribe(json!({"state_hash": true})).await;
    ws.frame_until(|f| f["cause"] == "control").await;
    let step = h.step(4).await.assert_json(200);
    let hashed = ws.frame_until(|f| f["tick"] == 7).await;
    assert_eq!(
        hashed["state_hash"], step["state_hash"],
        "the frame of tick 7 carries the hash the step reported for tick 7"
    );
    assert!(hashed["state_hash"].as_str().is_some_and(|h| h.len() == 16));
}

#[tokio::test]
async fn a_message_over_128_kib_closes_1009() {
    // Above the library's read cap (twice the limit): the connection still ends with 1009.
    let (_h, mut ws) = connected("stream-1009-huge", SKELETON).await;
    ws.send_text(format!(
        "{{\"type\":\"subscribe\",\"pad\":\"{}\"}}",
        "x".repeat(200_000)
    ))
    .await;
    loop {
        match ws.recv().await {
            Incoming::Close(code, reason) => {
                assert_eq!((code, reason.as_str()), (1009, "message_too_big"));
                break;
            }
            Incoming::Json(_) => {}
            Incoming::Ended => panic!("the connection ended without a 1009 close"),
        }
    }
}

#[tokio::test]
async fn frames_keep_coming_while_an_afap_run_rings_the_bell_all_the_time() {
    // The engine runs flat out and rings the connection after every tick: the writer must still
    // reach its pacing timer. Bounded only by the hang guard, never by an assertion on time.
    let h = Harness::start("stream-afap");
    let scenario = common::write_long_scenario(&h.root);
    h.load_body(&json!({"path": scenario, "clock": {"mode": "afap"}}))
        .await
        .assert_json(201);
    let addr = h.serve_on_loopback().await;
    let mut ws = WsClient::connect(addr).await;
    ws.hello().await;
    ws.subscribe(json!({"max_rate_hz": 60.0})).await;
    ws.next_frame().await;
    h.run_command("start").await.assert_json(200);
    let running = ws
        .frame_until(|f| f["run"]["state"] == "running" && f["tick"].as_u64() > Some(0))
        .await;
    assert_eq!(running["cause"], "tick");
    let later = ws
        .frame_until(|f| f["tick"].as_u64() > running["tick"].as_u64())
        .await;
    assert_eq!(later["run"]["state"], "running", "still running: {later}");
    h.run_command("stop").await.assert_json(200);
}

/// A stream of the given client messages, and a sink that holds back everything it is given until
/// `gate` is opened, then forwards it to `out`: the writer is blocked on its first send meanwhile.
fn gated_connection(
    gate: &tokio::sync::watch::Sender<bool>,
) -> (
    tokio::sync::mpsc::UnboundedSender<axum::extract::ws::Message>,
    tokio::sync::mpsc::UnboundedReceiver<axum::extract::ws::Message>,
    impl futures_util::Stream<Item = Result<axum::extract::ws::Message, String>> + Unpin + Send + use<>,
    impl futures_util::Sink<axum::extract::ws::Message, Error = String> + Unpin + Send + use<>,
) {
    let (client_tx, client_rx) = tokio::sync::mpsc::unbounded_channel();
    let (out_tx, out_rx) = tokio::sync::mpsc::unbounded_channel();
    let stream = Box::pin(futures_util::stream::unfold(
        client_rx,
        |mut rx| async move { rx.recv().await.map(|message| (Ok(message), rx)) },
    ));
    let gate_rx = gate.subscribe();
    let sink = Box::pin(futures_util::sink::unfold(out_tx, move |out, message| {
        let mut gate_rx = gate_rx.clone();
        async move {
            gate_rx
                .wait_for(|open| *open)
                .await
                .map_err(|e| e.to_string())?;
            out.send(message).map_err(|e| e.to_string())?;
            Ok::<_, String>(out)
        }
    }));
    (client_tx, out_rx, stream, sink)
}

#[tokio::test]
async fn pipelined_subscribes_leave_one_live_subscription_and_the_newest_wins() {
    let h = Harness::start("stream-mailbox");
    h.load_skeleton("stepped").await;
    h.run_command("start").await.assert_json(200);
    let (gate, _keep) = tokio::sync::watch::channel(false);
    let (client, mut out, stream, sink) = gated_connection(&gate);
    let params = umbra_server::ConnectionParams::new(
        h.handle.clone(),
        h.settings.server.stream.clone(),
        h.shutdown.clone(),
        umbra_server::Encoding::Json,
    );
    let builds_before = h.handle.snapshot_builds();
    let connection = tokio::spawn(umbra_server::run_connection(sink, stream, params));
    // The writer is stuck sending `hello`; the reader takes every subscribe meanwhile.
    for id in 1..=3_u64 {
        for _ in 0..4 {
            let message = json!({"type": "subscribe", "filter": {"entity_ids": [id]}});
            client
                .send(axum::extract::ws::Message::Text(message.to_string().into()))
                .expect("send");
        }
    }
    // Everything is read (and answered with a status question to the engine) within a moment.
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    gate.send_replace(true);
    let mut frames = Vec::new();
    while frames.is_empty() {
        let message = tokio::time::timeout(std::time::Duration::from_secs(30), out.recv())
            .await
            .expect("hang guard")
            .expect("the connection is open");
        if let axum::extract::ws::Message::Text(text) = message {
            let value: Value = serde_json::from_str(text.as_str()).expect("json");
            if value["type"] == "frame" {
                frames.push(value);
            }
        }
    }
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert_eq!(entity_ids(&frames[0]), [3], "the newest subscribe won");
    assert_eq!(
        h.handle.snapshot_builds() - builds_before,
        1,
        "twelve subscribes, one live subscription: one priming frame was built"
    );
    h.shutdown.trigger();
    connection.await.expect("the connection ends");
}
