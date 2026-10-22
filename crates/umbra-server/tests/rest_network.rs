//! AT-19 (M6 WI-17): the network adapter endpoints, ghosts in listings and frames, and the
//! problems around them. Requirements: REQ-X-5, REQ-X-6, REQ-X-9, ADR 0035, ASM-294, ASM-297,
//! ASM-400, ASM-410.
//!
//! The server runs the scenario `net_pair/c` of the engine fixtures: two adapters, `left`
//! (`test.net.left`, hub 1) and `right` (`test.net.right`, hub 2), each on its own in-process
//! loopback hub. Peer engines tick on the test thread: `b` shares hub 1 (its tank becomes a ghost of
//! `left`) and `a` shares hub 2 (its tank becomes a ghost of `right`). The engine's clock is
//! stepped, so every tick is a `step` request. Plugin adapters are in `rest_network_plugins.rs`.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod common;

use std::sync::Arc;

use axum::http::Method;
use common::{Harness, copy_dir, fixtures, id, peer, schema_walk, space, ticks};
use serde_json::{Value, json};
use umbra_api_types::ProblemCode;
use umbra_engine::{Engine, LoopbackHub};

const LEFT_BASE: u64 = 3000;
const RIGHT_BASE: u64 = 5000;

/// The server, two peers, and the hubs that connect them.
struct Net {
    h: Harness,
    /// Peer `b` of adapter `left`.
    b: Engine,
    /// Peer `a` of adapter `right`.
    a: Engine,
    /// The hub of adapter `left` (its peer `b` shares it).
    left_hub: Arc<LoopbackHub>,
}

impl Net {
    /// Builds the server over a registry holding both adapters and copies the fixtures into its
    /// root; nothing is loaded yet.
    fn start(tag: &str) -> Self {
        let hub_left = LoopbackHub::new(4096);
        let hub_right = LoopbackHub::new(4096);
        let (left, right) = (Arc::clone(&hub_left), Arc::clone(&hub_right));
        let h = Harness::with_handle(
            tag,
            |_| {},
            move |config| {
                for (name, hub, base) in [
                    ("test.net.left", &left, LEFT_BASE),
                    ("test.net.right", &right, RIGHT_BASE),
                ] {
                    config
                        .base_registry
                        .register_network_adapter(hub.factory(id(name), space(base)))
                        .expect("register the adapter");
                }
            },
        );
        copy_dir(&fixtures(), &h.root.join("net_pair"));
        Self {
            b: peer(&hub_left, "b", 2000),
            left_hub: hub_left,
            a: peer(&hub_right, "a", 6000),
            h,
        }
    }

    async fn load(&self) {
        self.h
            .load_body(&json!({"path": "net_pair/c/scenario.toml", "clock": {"mode": "stepped"}}))
            .await
            .assert_json(201);
    }

    /// Ticks both peers and the server `n` times, in that order.
    async fn run(&mut self, n: u64) {
        ticks(&mut self.b, n);
        ticks(&mut self.a, n);
        self.h.step(n).await.assert_json(200);
    }

    /// A server in `running` with a ghost from each adapter.
    async fn with_ghosts(tag: &str) -> Self {
        let mut net = Self::start(tag);
        net.load().await;
        net.h.run_command("start").await.assert_json(200);
        net.run(5).await;
        net
    }

    async fn adapters(&self) -> Vec<Value> {
        let list = self
            .h
            .get("/api/v1/network/adapters")
            .await
            .assert_json(200);
        list.as_array().expect("an array").clone()
    }

    async fn adapter(&self, name: &str) -> Value {
        self.adapters()
            .await
            .into_iter()
            .find(|a| a["name"] == name)
            .unwrap_or_else(|| panic!("no adapter {name}"))
    }

    async fn set(&self, name: &str, action: &str) -> common::Reply {
        self.h
            .post(&format!("/api/v1/network/adapters/{name}/{action}"))
            .await
    }
}

fn counter(adapter: &Value, name: &str) -> u64 {
    adapter["counters"][name]
        .as_u64()
        .unwrap_or_else(|| panic!("counter {name} in {adapter}"))
}

#[tokio::test]
async fn the_list_has_every_adapter_in_scenario_order_with_its_state_and_counters() {
    let net = Net::start("net-list");
    net.h
        .get("/api/v1/network/adapters")
        .await
        .assert_problem(ProblemCode::NoScenario);
    net.load().await;

    let list = net.adapters().await;

    let names: Vec<&str> = list
        .iter()
        .map(|a| a["name"].as_str().expect("name"))
        .collect();
    assert_eq!(names, ["left", "right"]);
    assert_eq!(list[0]["adapter"], "test.net.left");
    assert_eq!(list[1]["adapter"], "test.net.right");
    assert_eq!(
        list[0]["key_space"],
        json!({"base": LEFT_BASE, "count": 100})
    );
    assert_eq!(
        list[1]["key_space"],
        json!({"base": RIGHT_BASE, "count": 100})
    );
    for adapter in &list {
        assert_eq!(adapter["enabled"], true, "{adapter}");
        assert_eq!(adapter["faulted"], false, "{adapter}");
        assert_eq!(adapter["ghosts"], 0, "{adapter}");
        assert!(
            adapter.get("plugin").is_none(),
            "a built-in adapter: {adapter}"
        );
        assert!(adapter.get("fault_reason").is_none(), "{adapter}");
        for name in [
            "published",
            "received",
            "echo_dropped",
            "unmapped",
            "keys_exhausted",
            "ghosts_capped",
            "control_ignored",
            "world_errors",
            "unsupported",
            "invalid_state",
            "tx_dropped",
            "rx_dropped",
            "decode_errors",
            "io_errors",
        ] {
            assert_eq!(counter(adapter, name), 0, "{name} before the first tick");
        }
    }
}

#[tokio::test]
async fn a_scenario_without_adapters_lists_none() {
    let h = Harness::start("net-none");
    h.load_skeleton("stepped").await;

    let list = h.get("/api/v1/network/adapters").await.assert_json(200);

    assert_eq!(list, json!([]));
    h.post("/api/v1/network/adapters/left/enable")
        .await
        .assert_problem(ProblemCode::AdapterNotFound);
}

#[tokio::test]
async fn list_enable_and_disable_work_in_every_loaded_state_and_need_a_scenario() {
    let net = Net::start("net-states");
    for action in ["enable", "disable"] {
        net.set("left", action)
            .await
            .assert_problem(ProblemCode::NoScenario);
    }
    net.load().await;

    // `ready`, then `running`, `paused` and `stopped`.
    for state in ["ready", "running", "paused", "stopped"] {
        let command = match state {
            "running" => Some("start"),
            "paused" => Some("pause"),
            "stopped" => Some("stop"),
            _ => None,
        };
        if let Some(command) = command {
            net.h.run_command(command).await.assert_json(200);
        }
        let status = net.h.get("/api/v1/status").await.assert_json(200);
        assert_eq!(status["run"]["state"], state);

        let off = net.set("left", "disable").await.assert_json(200);
        assert_eq!(
            (off["name"].clone(), off["enabled"].clone()),
            (json!("left"), json!(false)),
            "{state}"
        );
        assert_eq!(net.adapter("left").await["enabled"], false, "{state}");
        assert_eq!(
            net.adapter("right").await["enabled"],
            true,
            "{state}: right is untouched"
        );

        let on = net.set("left", "enable").await.assert_json(200);
        assert_eq!(on["enabled"], true, "{state}");
        assert_eq!(net.adapter("left").await["enabled"], true, "{state}");
    }
}

#[tokio::test]
async fn enable_and_disable_are_idempotent() {
    let mut net = Net::start("net-idempotent");
    net.load().await;
    net.h.run_command("start").await.assert_json(200);
    net.run(3).await;

    let first = net.set("left", "disable").await.assert_json(200);
    let second = net.set("left", "disable").await.assert_json(200);
    assert_eq!(first, second);
    assert_eq!(first["enabled"], false);

    net.set("left", "enable").await.assert_json(200);
    let again = net.set("left", "enable").await.assert_json(200);
    assert_eq!(again["enabled"], true);
}

#[tokio::test]
async fn a_command_touches_only_the_named_adapter() {
    let mut net = Net::with_ghosts("net-one-only").await;
    let before_left = net.adapter("left").await;
    let before_right = net.adapter("right").await;
    assert!(
        counter(&before_left, "received") > 0,
        "left heard its peer: {before_left}"
    );
    assert!(
        counter(&before_right, "received") > 0,
        "right heard its peer: {before_right}"
    );

    net.set("left", "disable").await.assert_json(200);
    // Longer than the 1 s heartbeat (30 ticks), so the enabled adapter publishes again.
    net.run(40).await;

    let left = net.adapter("left").await;
    let right = net.adapter("right").await;
    assert_eq!(left["enabled"], false);
    assert_eq!(right["enabled"], true);
    assert_eq!(
        counter(&left, "published"),
        counter(&before_left, "published"),
        "a disabled adapter is not published to"
    );
    assert_eq!(
        counter(&left, "received"),
        counter(&before_left, "received"),
        "a disabled adapter is not polled"
    );
    assert!(
        counter(&right, "published") > counter(&before_right, "published"),
        "the other adapter still publishes: {right}"
    );
    assert!(
        counter(&right, "received") > counter(&before_right, "received"),
        "the other adapter is still polled: {right}"
    );

    // Disabling the other one leaves the first as it is, and enabling resumes the traffic.
    net.set("right", "disable").await.assert_json(200);
    net.set("left", "enable").await.assert_json(200);
    net.run(40).await;
    let left_after = net.adapter("left").await;
    assert_eq!(left_after["enabled"], true);
    assert!(
        counter(&left_after, "published") > counter(&left, "published"),
        "{left_after}"
    );
    let right_after = net.adapter("right").await;
    assert_eq!(right_after["enabled"], false);
    assert_eq!(
        counter(&right_after, "published"),
        counter(&right, "published")
    );
}

#[tokio::test]
async fn an_unknown_name_is_adapter_not_found_and_names_are_exact() {
    let net = Net::start("net-unknown");
    net.load().await;
    for name in ["nope", "Left", "left%20", "test.net.left"] {
        for action in ["enable", "disable"] {
            let problem = net
                .set(name, action)
                .await
                .assert_problem(ProblemCode::AdapterNotFound);
            assert_eq!(problem.status, 404);
            assert_eq!(problem.r#type, "urn:umbra:problem:adapter_not_found");
        }
    }
    // Nothing changed.
    assert!(net.adapters().await.iter().all(|a| a["enabled"] == true));
}

#[tokio::test]
async fn ghosts_name_the_adapter_they_came_from() {
    let net = Net::with_ghosts("net-sources").await;

    let remote = net.h.entities("ownership=remote").await.assert_json(200);

    assert_eq!(remote["total"], 2, "{remote}");
    let items = remote["items"].as_array().expect("items");
    let source_of = |name: &str| -> Value {
        items
            .iter()
            .find(|e| e["name"] == name)
            .unwrap_or_else(|| panic!("no ghost {name} in {remote}"))["remote_source"]
            .clone()
    };
    assert_eq!(source_of("b-tank"), "left");
    assert_eq!(source_of("a-tank"), "right");
    for ghost in items {
        assert_eq!(ghost["ownership"], "remote");
        assert_eq!(
            ghost["template"], "tank.mbt_generic",
            "mapped template: {ghost}"
        );
    }
    assert_eq!(net.adapter("left").await["ghosts"], 1);
    assert_eq!(net.adapter("right").await["ghosts"], 1);

    // Owned entities carry no source, and one entity read agrees with the listing.
    let owned = net.h.entities("ownership=owned").await.assert_json(200);
    assert_eq!(owned["total"], 1, "{owned}");
    assert!(owned["items"][0].get("remote_source").is_none(), "{owned}");
    let ghost_id = items[0]["id"].as_u64().expect("id");
    let one = net
        .h
        .get(&format!("/api/v1/entities/{ghost_id}"))
        .await
        .assert_json(200);
    assert_eq!(one["summary"]["remote_source"], items[0]["remote_source"]);
}

#[tokio::test]
async fn a_stream_frame_carries_the_ghost_with_its_source() {
    let mut net = Net::with_ghosts("net-stream").await;
    let addr = net.h.serve_on_loopback().await;
    let mut ws = common::WsClient::connect(addr).await;
    ws.hello().await;
    ws.subscribe(json!({})).await;
    let primed = ws.next_frame().await;
    // A tick after the subscription, so the frame is a tick frame, not the priming one.
    net.run(1).await;

    let frame = ws
        .frame_until(|frame| frame["tick"].as_u64() > primed["tick"].as_u64())
        .await;
    assert_eq!(frame["cause"], "tick");

    let mut sources: Vec<(String, String)> = frame["entities"]
        .as_array()
        .expect("entities")
        .iter()
        .filter(|e| e["ownership"] == "remote")
        .map(|e| {
            (
                e["name"].as_str().expect("name").to_owned(),
                e["remote_source"].as_str().expect("source").to_owned(),
            )
        })
        .collect();
    sources.sort();
    assert_eq!(
        sources,
        [
            ("a-tank".to_owned(), "right".to_owned()),
            ("b-tank".to_owned(), "left".to_owned())
        ]
    );
    let owned: Vec<&Value> = frame["entities"]
        .as_array()
        .expect("entities")
        .iter()
        .filter(|e| e["ownership"] == "owned")
        .collect();
    assert!(
        owned.iter().all(|e| e.get("remote_source").is_none()),
        "{frame}"
    );
}

#[tokio::test]
async fn removing_a_ghost_is_entity_remote_and_changes_nothing() {
    let net = Net::with_ghosts("net-ghost-delete").await;
    let remote = net.h.entities("ownership=remote").await.assert_json(200);
    let ghost = remote["items"][0]["id"].as_u64().expect("ghost id");

    let refused = net
        .h
        .send(Method::DELETE, &format!("/api/v1/entities/{ghost}"))
        .await;

    let problem = refused.assert_problem(ProblemCode::EntityRemote);
    assert_eq!(problem.status, 409);
    assert!(
        problem.detail.contains(&ghost.to_string()),
        "{}",
        problem.detail
    );
    net.h
        .get(&format!("/api/v1/entities/{ghost}"))
        .await
        .assert_json(200);
    assert_eq!(
        net.h.entities("ownership=remote").await.assert_json(200)["total"],
        2
    );
    // An owned entity is removed as before.
    let owned = net.h.entities("ownership=owned").await.assert_json(200);
    let own_id = owned["items"][0]["id"].as_u64().expect("owned id");
    let gone = net
        .h
        .send(Method::DELETE, &format!("/api/v1/entities/{own_id}"))
        .await;
    assert_eq!(gone.status, 204);
}

#[tokio::test]
async fn scenario_warnings_are_carried_with_severity_warning() {
    let net = Net::start("net-warnings");
    // A ghost timeout that is not above the heartbeat is a warning, never an error (ASM-290).
    let dir = net.h.root.join("net_pair/warn");
    copy_dir(&net.h.root.join("net_pair/c"), &dir);
    let file = dir.join("scenario.toml");
    let text = std::fs::read_to_string(&file).expect("read");
    std::fs::write(
        &file,
        text.replacen("timeout_s = 3.0", "timeout_s = 0.5", 1),
    )
    .expect("write");

    let loaded = net
        .h
        .load_body(&json!({"path": "net_pair/warn/scenario.toml", "clock": {"mode": "stepped"}}))
        .await
        .assert_json(201);

    let warnings = loaded["warnings"].as_array().expect("warnings");
    assert_eq!(warnings.len(), 1, "{loaded}");
    assert_eq!(warnings[0]["severity"], "warning");
    assert_eq!(warnings[0]["code"], "suspicious_value");
    assert_eq!(
        warnings[0]["file"], "net_pair/warn/scenario.toml",
        "the file is in display form"
    );
    let again = net.h.get("/api/v1/scenario").await.assert_json(200);
    assert_eq!(again["warnings"], loaded["warnings"]);

    // A clean scenario has no `warnings` key, and an error keeps `severity: error`.
    let clean = net
        .h
        .load_body(&json!({"path": "net_pair/c/scenario.toml"}))
        .await
        .assert_json(201);
    assert!(clean.get("warnings").is_none(), "{clean}");
    let bad = net
        .h
        .load_body(&json!({"path": "net_pair/missing/scenario.toml"}))
        .await;
    assert_eq!(bad.status, 422, "{}", bad.text());
    assert_eq!(
        bad.json()["diagnostics"][0]["severity"],
        "error",
        "{}",
        bad.text()
    );
}

/// Checks `reply` against the schema the served document gives `method template` for its status.
fn conforms(doc: &Value, method: &str, template: &str, reply: &common::Reply) {
    let response = &doc["paths"][template][method]["responses"][reply.status.to_string()];
    assert!(
        response.is_object(),
        "{method} {template}: status {} undocumented",
        reply.status
    );
    let media = reply
        .content_type
        .as_deref()
        .and_then(|t| t.split(';').next())
        .expect("content type");
    let schema = &response["content"][media]["schema"];
    assert!(
        schema.is_object(),
        "{method} {template}: no {media} schema for {}",
        reply.status
    );
    if let Err(e) = schema_walk::validate_closed(doc, schema, &reply.json()) {
        panic!(
            "{method} {template} ({}): {e}\n{}",
            reply.status,
            reply.text()
        );
    }
}

#[tokio::test]
async fn the_adapter_responses_conform_to_the_openapi_document() {
    let net = Net::with_ghosts("net-conform").await;
    let doc = net.h.get("/api/v1/openapi.json").await.assert_json(200);
    let list = "/api/v1/network/adapters";
    let enable = "/api/v1/network/adapters/{name}/enable";
    let disable = "/api/v1/network/adapters/{name}/disable";

    let reply = net.h.get(list).await;
    assert_eq!(reply.status, 200);
    assert!(
        reply.json()[0]["counters"]["received"]
            .as_u64()
            .expect("count")
            > 0
    );
    conforms(&doc, "get", list, &reply);
    for (template, action) in [(enable, "enable"), (disable, "disable")] {
        let reply = net.set("left", action).await;
        assert_eq!(reply.status, 200, "{}", reply.text());
        conforms(&doc, "post", template, &reply);
        let missing = net.set("nope", action).await;
        assert_eq!(missing.status, 404, "{}", missing.text());
        conforms(&doc, "post", template, &missing);
    }
    // A ghost listing and a stopped-run status conform too (`remote_source`).
    let remote = net.h.entities("ownership=remote").await;
    assert_eq!(remote.status, 200);
    conforms(&doc, "get", "/api/v1/entities", &remote);
    let idle = Harness::start("net-conform-idle");
    let conflict = idle.get(list).await;
    assert_eq!(conflict.status, 409);
    conforms(&doc, "get", list, &conflict);
}

#[tokio::test]
async fn a_built_in_adapter_fault_under_abort_stops_the_run_as_network_fault() {
    let mut net = Net::start("net-fault");
    net.load().await;
    net.h.run_command("start").await.assert_json(200);
    let addr = net.h.serve_on_loopback().await;
    let mut ws = common::WsClient::connect(addr).await;
    ws.hello().await;
    ws.subscribe(json!({})).await;
    net.run(3).await;
    let healthy = net.h.get("/api/v1/status").await.assert_json(200);
    assert_eq!(healthy["run"]["state"], "running");
    assert!(healthy["run"].get("network_fault").is_none());

    // The next publish and poll of `left` fail (the peer on that hub fails too and is not stepped);
    // the default policy is `abort`.
    net.left_hub.close();
    let _ = net.h.step(3).await;

    let status = net.h.get("/api/v1/status").await.assert_json(200);
    assert_eq!(status["run"]["state"], "stopped", "{status}");
    assert_eq!(status["run"]["stop_reason"], "network_fault", "{status}");
    assert_eq!(
        status["run"]["network_fault"]["adapter"], "left",
        "{status}"
    );
    assert!(
        status["run"]["network_fault"]["reason"]
            .as_str()
            .is_some_and(|r| !r.is_empty()),
        "{status}"
    );
    assert!(
        status["run"].get("fault").is_none(),
        "not a plugin fault: {status}"
    );
    // The stream's status says the same.
    let frame = ws
        .frame_until(|f| f["run"]["stop_reason"] == "network_fault")
        .await;
    assert_eq!(frame["run"]["network_fault"]["adapter"], "left");
    let adapters = net.adapters().await;
    assert_eq!(adapters[0]["faulted"], true);
    assert_eq!(
        adapters[1]["faulted"], false,
        "only the closed hub's adapter"
    );
}

#[tokio::test]
async fn two_adapters_of_one_component_are_addressed_by_their_names() {
    let mut net = Net::start("net-same-component");
    // `right` becomes a second `test.net.left` (one hub, its own key space).
    let dir = net.h.root.join("net_pair/twin");
    copy_dir(&net.h.root.join("net_pair/c"), &dir);
    let file = dir.join("scenario.toml");
    let text = std::fs::read_to_string(&file).expect("read");
    let from = "adapter = \"test.net.right\"\nname = \"right\"\n";
    assert!(text.contains(from));
    std::fs::write(
        &file,
        text.replacen(
            from,
            "adapter = \"test.net.left\"\nname = \"right\"\nparams = { key_space = [5000, 100] }\n",
            1,
        ),
    )
    .expect("write");
    net.h
        .load_body(&json!({"path": "net_pair/twin/scenario.toml", "clock": {"mode": "stepped"}}))
        .await
        .assert_json(201);
    net.h.run_command("start").await.assert_json(200);
    net.run(3).await;

    let list = net.adapters().await;
    assert_eq!(list[0]["name"], "left");
    assert_eq!(list[1]["name"], "right");
    assert_eq!(list[0]["adapter"], "test.net.left");
    assert_eq!(
        list[1]["adapter"], "test.net.left",
        "the same component twice"
    );
    assert_eq!(list[1]["key_space"], json!({"base": 5000, "count": 100}));

    let off = net.set("right", "disable").await.assert_json(200);
    assert_eq!(
        (off["name"].clone(), off["enabled"].clone()),
        (json!("right"), json!(false))
    );
    let before = net.adapters().await;
    net.run(40).await;
    let after = net.adapters().await;
    assert_eq!(before[0]["enabled"], true);
    assert_eq!(
        after[0]["enabled"], true,
        "the twin with the other name is untouched"
    );
    assert!(
        counter(&after[0], "published") > counter(&before[0], "published"),
        "left still publishes: {}",
        after[0]
    );
    assert_eq!(
        counter(&after[1], "published"),
        counter(&before[1], "published")
    );
    assert_eq!(
        counter(&after[1], "received"),
        counter(&before[1], "received")
    );
}
