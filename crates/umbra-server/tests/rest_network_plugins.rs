//! AT-19 (M6 WI-17), plugin rows: network adapters provided by plugins in the adapter endpoints.
//! Requirements: REQ-X-5, ADR 0035, ASM-410, ASM-415. The plugins are the SDK-built fixture
//! (`umbra-test-plugin`, features `network` and `network_peer`, two distinct images), built by the
//! nested-cargo helper shared with the engine tests. The plugin image table is process-wide, so the
//! tests of this file take `PLUGIN_TEST_LOCK`.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

#[path = "../../umbra-engine/tests/plugin_support/mod.rs"]
mod plugin_support;

mod common;

use std::collections::BTreeSet;
use std::sync::Arc;

use common::{Harness, copy_dir, fixtures, id, peer, space, ticks};
use serde_json::{Value, json};
use umbra_api_types::ProblemCode;
use umbra_engine::LoopbackHub;

static PLUGIN_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// A server whose plugin path holds the fixture (`network`) and the peer image (`network_peer`),
/// with a loopback adapter factory on `hub`; the `net_pair` fixtures are copied into its root.
fn server(tag: &str, hub: &Arc<LoopbackHub>) -> Harness {
    let both = plugin_support::plugins_dir(&[
        (
            "fixture",
            &plugin_support::library("umbra-test-plugin", &["network"]),
        ),
        (
            "peer",
            &plugin_support::library("umbra-test-plugin", &["network_peer"]),
        ),
    ]);
    let hub = Arc::clone(hub);
    let h = Harness::with_handle(
        tag,
        |config| config.plugin_paths = Some(vec![both]),
        move |config| {
            config
                .base_registry
                .register_network_adapter(hub.factory(id("test.net.loopback"), space(1000)))
                .expect("register the loopback");
        },
    );
    copy_dir(&fixtures(), &h.root.join("net_pair"));
    h
}

async fn load(h: &Harness, scenario: &str) {
    h.load_body(&json!({"path": format!("net_pair/{scenario}/scenario.toml"), "clock": {"mode": "stepped"}}))
        .await
        .assert_json(201);
}

async fn adapters(h: &Harness) -> Vec<Value> {
    h.get("/api/v1/network/adapters")
        .await
        .assert_json(200)
        .as_array()
        .expect("an array")
        .clone()
}

fn named<'a>(list: &'a [Value], name: &str) -> &'a Value {
    list.iter()
        .find(|a| a["name"] == name)
        .unwrap_or_else(|| panic!("no adapter {name} in {list:?}"))
}

#[tokio::test]
async fn adapters_of_three_origins_are_listed_and_attributed_and_addressed_one_by_one() {
    let _guard = PLUGIN_TEST_LOCK.lock().await;
    let hub = LoopbackHub::new(4096);
    let h = server("netp-three", &hub);
    let mut b = peer(&hub, "b", 2000);
    load(&h, "plugins_a").await;
    h.run_command("start").await.assert_json(200);

    let before = adapters(&h).await;

    let names: Vec<&str> = before
        .iter()
        .map(|a| a["name"].as_str().expect("name"))
        .collect();
    assert_eq!(names, ["lb", "left", "right"], "scenario order");
    assert_eq!(before[0]["adapter"], "test.net.loopback");
    assert!(
        before[0].get("plugin").is_none(),
        "the loopback is built in"
    );
    assert_eq!(before[1]["adapter"], "test.network.net_ok");
    assert_eq!(before[1]["plugin"], "test.plugin.fixture");
    assert_eq!(before[2]["adapter"], "test.network.peer_ok");
    assert_eq!(before[2]["plugin"], "test.plugin.net_peer");

    // Ticks long enough for each adapter to publish and hear back.
    for _ in 0..4 {
        ticks(&mut b, 20);
        h.step(20).await.assert_json(200);
    }
    let ghosts = h.entities("ownership=remote").await.assert_json(200);
    let sources: BTreeSet<&str> = ghosts["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|e| e["remote_source"].as_str().expect("remote_source"))
        .collect();
    assert_eq!(sources, BTreeSet::from(["lb", "left", "right"]), "{ghosts}");
    let running = adapters(&h).await;
    for adapter in &running {
        assert_eq!(adapter["ghosts"], 1, "{adapter}");
        assert_eq!(adapter["faulted"], false, "{adapter}");
    }

    // Disabling the plugin adapter `left` leaves the other plugin adapter and the loopback alone.
    let off = h
        .post("/api/v1/network/adapters/left/disable")
        .await
        .assert_json(200);
    assert_eq!(off["plugin"], "test.plugin.fixture");
    assert_eq!(off["enabled"], false);
    let after = adapters(&h).await;
    assert_eq!(named(&after, "left")["enabled"], false);
    assert_eq!(named(&after, "right")["enabled"], true);
    assert_eq!(named(&after, "lb")["enabled"], true);
    let received = |list: &[Value], name: &str| named(list, name)["counters"]["received"].as_u64();
    ticks(&mut b, 40);
    h.step(40).await.assert_json(200);
    let later = adapters(&h).await;
    assert_eq!(
        received(&later, "left"),
        received(&after, "left"),
        "not polled"
    );
    assert!(
        received(&later, "right") > received(&after, "right"),
        "{later:?}"
    );
    assert!(received(&later, "lb") > received(&after, "lb"), "{later:?}");
}

#[tokio::test]
async fn a_faulted_plugin_adapter_reports_why_cannot_be_enabled_but_can_be_disabled() {
    let _guard = PLUGIN_TEST_LOCK.lock().await;
    let h = server("netp-fault", &LoopbackHub::new(16));
    load(&h, "plugin_fault_disable").await;
    h.run_command("start").await.assert_json(200);
    assert_eq!(
        adapters(&h).await[0]["faulted"],
        false,
        "healthy until tick 5"
    );

    // The adapter's `poll` panics from tick 5 on; the policy disables the capability.
    h.step(10).await.assert_json(200);

    let list = adapters(&h).await;
    let left = named(&list, "left");
    assert_eq!(left["faulted"], true, "{left}");
    assert_eq!(left["enabled"], false, "{left}");
    assert!(
        left["fault_reason"].as_str().is_some_and(|r| !r.is_empty()),
        "{left}"
    );
    assert_eq!(left["plugin"], "test.plugin.fixture");
    let refused = h.post("/api/v1/network/adapters/left/enable").await;
    let problem = refused.assert_problem(ProblemCode::AdapterFaulted);
    assert_eq!(problem.status, 409);
    assert_eq!(
        named(&adapters(&h).await, "left")["enabled"],
        false,
        "still disabled"
    );
    let off = h
        .post("/api/v1/network/adapters/left/disable")
        .await
        .assert_json(200);
    assert_eq!(off["faulted"], true);
    assert_eq!(off["enabled"], false);
}

/// Loads `net_pair/detonate` (the `plugins_a` adapters, `left` detonating at tick 40), subscribes
/// with `filter`, steps 60 ticks and returns the `detonation` events the stream carried.
async fn detonations(h: &Harness, filter: Value) -> Vec<Value> {
    load(h, "detonate").await;
    h.run_command("start").await.assert_json(200);
    let addr = h.serve_on_loopback().await;
    let mut ws = common::WsClient::connect(addr).await;
    ws.hello().await;
    ws.subscribe(json!({"filter": filter})).await;
    ws.next_frame().await;
    h.step(60).await.assert_json(200);
    let mut events = Vec::new();
    loop {
        let frame = ws.frame_until(|_| true).await;
        events.extend(frame["events"].as_array().expect("events").iter().cloned());
        if frame["tick"].as_u64() >= Some(60) {
            ws.close().await;
            return events;
        }
    }
}

#[tokio::test]
async fn a_peer_detonation_reaches_the_stream_with_its_ghost_and_position_and_follows_the_filters()
{
    let _guard = PLUGIN_TEST_LOCK.lock().await;
    let h = server("netp-detonation", &LoopbackHub::new(16));
    let dir = h.root.join("net_pair/detonate");
    copy_dir(&h.root.join("net_pair/plugins_a"), &dir);
    let file = dir.join("scenario.toml");
    let text = std::fs::read_to_string(&file).expect("read");
    let from = "params = { key_base = 3000 }";
    assert!(text.contains(from));
    std::fs::write(
        &file,
        text.replacen(
            from,
            "params = { key_base = 3000, detonate_at_tick = 40 }",
            1,
        ),
    )
    .expect("write");

    let only = json!({"event_kinds": ["detonation"]});
    let all = detonations(&h, only).await;

    assert_eq!(all.len(), 2, "{all:?}");
    // `poll` of the tick that starts from 40 reports the events in tick 41.
    assert!(
        all.iter()
            .all(|e| e["kind"] == "detonation" && e["tick"] == 41),
        "{all:?}"
    );
    let ghosts = h.entities("ownership=remote").await.assert_json(200);
    let ghost = ghosts["items"]
        .as_array()
        .expect("items")
        .iter()
        .find(|e| e["remote_source"] == "left")
        .expect("the mirrored tank is a ghost of `left`")["id"]
        .as_u64()
        .expect("id");
    let shot: Vec<&Value> = all.iter().filter(|e| e["entity_id"] == ghost).collect();
    assert_eq!(
        shot.len(),
        1,
        "one detonation names the ghost as shooter: {all:?}"
    );
    let unnamed: Vec<&Value> = all
        .iter()
        .filter(|e| e.get("entity_id").is_none())
        .collect();
    assert_eq!(unnamed.len(), 1, "one has no shooter: {all:?}");
    for event in &all {
        let position = &event["detail"]["position"];
        for (key, want, tolerance) in [
            ("lat_deg", 0.0, 1e-9),
            ("lon_deg", 0.0, 1e-9),
            ("alt_m", 100.0, 1e-3),
        ] {
            let got = position[key]
                .as_f64()
                .unwrap_or_else(|| panic!("{key} in {event}"));
            assert!((got - want).abs() < tolerance, "{key}: {got}");
        }
    }

    // The id filter treats the shooter as the entity the event concerns; an event without one stays.
    let named = json!({"event_kinds": ["detonation"], "entity_ids": [ghost]});
    assert_eq!(
        detonations(&h, named).await.len(),
        2,
        "shooter kept, no-shooter kept"
    );
    let other = json!({"event_kinds": ["detonation"], "entity_ids": [ghost + 1000]});
    let kept = detonations(&h, other).await;
    assert_eq!(
        kept.len(),
        1,
        "the event of the other shooter is dropped: {kept:?}"
    );
    assert!(kept[0].get("entity_id").is_none());
    let none = json!({"event_kinds": ["fire"]});
    assert!(
        detonations(&h, none).await.is_empty(),
        "the kind filter drops them"
    );
}
