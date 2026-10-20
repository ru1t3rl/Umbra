//! AT-8 (plugin rows, own process, m8): catalog, validation and the plugin report with a real
//! plugin loaded. Requirements: REQ-X-10, REQ-X-11, REQ-B-13, REQ-B-17, REQ-P-10, ADR 0026.
//!
//! The example plugin is built by the nested-cargo helper shared with the engine tests. The plugin
//! image table is process-wide, so the tests of this file take `PLUGIN_TEST_LOCK`. Every test is
//! ignored until the router (WI-11) and the handlers (WI-14) exist.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

#[path = "../../umbra-engine/tests/plugin_support/mod.rs"]
mod plugin_support;

mod common;

use common::{Harness, copy_example};
use serde_json::{Value, json};

/// Serialises the tests of this process (one plugin image table).
static PLUGIN_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// A server whose plugin path holds the example plugin, with `plugin_wander` loaded in its root.
async fn with_wander(tag: &str) -> Harness {
    let plugins = plugin_support::wander();
    let h = Harness::with(tag, |config| config.plugin_paths = Some(vec![plugins]));
    copy_example(&h.root, "plugin_wander");
    h.load_body(&json!({"path": "plugin_wander/scenario.toml", "clock": {"mode": "stepped"}}))
        .await
        .assert_json(201);
    h
}

fn entries<'a>(catalog: &'a Value, provenance: &str) -> Vec<&'a Value> {
    catalog["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .filter(|e| e["provenance"] == provenance)
        .collect()
}

#[tokio::test]
#[ignore = "WI-14"]
async fn builtin_and_plugin_entries_are_read_only() {
    let _guard = PLUGIN_TEST_LOCK.lock().await;
    let h = with_wander("catp-readonly").await;
    let catalog = h.get("/api/v1/catalog/behavior").await.assert_json(200);
    let plugin = entries(&catalog, "plugin");
    let wander = plugin
        .iter()
        .find(|e| e["id"] == "example.behavior.wander")
        .expect("the plugin behavior is in the catalog");
    assert_eq!(wander["kind"], "behavior");
    for entry in plugin.iter().chain(entries(&catalog, "builtin").iter()) {
        assert_eq!(entry["read_only"], true, "{entry}");
    }
}

#[tokio::test]
#[ignore = "WI-14"]
async fn validate_graph_referencing_plugin_behavior_is_valid() {
    let _guard = PLUGIN_TEST_LOCK.lock().await;
    let h = with_wander("catp-validate").await;
    let document = plugin_support::call_graph("probe.call_wander", "example.behavior.wander");
    let result = h
        .post_json("/api/v1/behaviors/validate", &json!({"document": document}))
        .await
        .assert_json(200);
    assert_eq!(result["valid"], true, "{result}");
    // Without the plugin session the same reference is unknown.
    assert_eq!(
        h.send(axum::http::Method::DELETE, "/api/v1/scenario")
            .await
            .status,
        204
    );
    let idle = h
        .post_json("/api/v1/behaviors/validate", &json!({"document": document}))
        .await
        .assert_json(200);
    assert_eq!(idle["valid"], false, "{idle}");
}

#[tokio::test]
#[ignore = "WI-14"]
async fn plugins_report_capabilities_faults_and_root_relative_origins() {
    let _guard = PLUGIN_TEST_LOCK.lock().await;
    let h = with_wander("catp-report").await;
    let report = h.get("/api/v1/plugins").await.assert_json(200);
    let plugins = report["plugins"].as_array().expect("plugins");
    assert_eq!(plugins.len(), 1, "{report}");
    let plugin = &plugins[0];
    assert_eq!(plugin["id"], "example.plugin.wander");
    assert_eq!(plugin["root"], "plugins[0]");
    let origin = plugin["origin"].as_str().expect("origin");
    assert!(!origin.starts_with('/'), "root relative: {origin}");
    let capability = plugin["capabilities"]
        .as_array()
        .expect("capabilities")
        .iter()
        .find(|c| c["id"] == "example.behavior.wander")
        .expect("the behavior capability");
    assert_eq!(capability["kind"], "behavior");
    assert_eq!(capability["faulted"], false);
    assert!(
        report.get("fault").is_none_or(Value::is_null),
        "no fault yet"
    );
}
