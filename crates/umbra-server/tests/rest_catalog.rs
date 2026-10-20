//! AT-8 (WI-14): the behavior catalog, graph validation and the plugin report over REST.
//! Requirements: REQ-X-10, REQ-X-11, REQ-B-13, REQ-B-17, M5 acceptance 4. Plugin rows are in
//! `rest_catalog_plugins.rs` (their own process, m8).
//!
//! Every test is ignored until the router (WI-11) and the catalog handlers (WI-14) exist.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod common;

use std::collections::BTreeSet;

use common::{Harness, examples_dir};
use serde_json::{Value, json};
use umbra_api_types::ProblemCode;
use umbra_engine::builtins::standard_registry;
use umbra_engine::{Catalog, CatalogKind};

fn kind_name(kind: CatalogKind) -> &'static str {
    match kind {
        CatalogKind::NodeType => "node_type",
        CatalogKind::Condition => "condition",
        CatalogKind::Action => "action",
        CatalogKind::Behavior => "behavior",
        _ => "other",
    }
}

fn pairs(catalog: &Value) -> BTreeSet<(String, String)> {
    catalog["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .map(|e| {
            (
                e["kind"].as_str().expect("kind").to_string(),
                e["id"].as_str().expect("id").to_string(),
            )
        })
        .collect()
}

fn base_pairs() -> BTreeSet<(String, String)> {
    let registry = standard_registry().expect("registry");
    Catalog::from_registry(&registry)
        .entries()
        .iter()
        .map(|e| (kind_name(e.kind).to_string(), e.id.clone()))
        .collect()
}

fn follow_route_graph() -> String {
    std::fs::read_to_string(
        examples_dir().join("patrol_and_engage/behaviors/follow_route.behavior.toml"),
    )
    .expect("example graph")
}

#[tokio::test]
#[ignore = "WI-14"]
async fn catalog_lists_node_types_conditions_actions_and_graphs_with_schema_version_provenance() {
    let h = Harness::start("cat-all");
    let catalog = h.get("/api/v1/catalog/behavior").await.assert_json(200);
    let found = pairs(&catalog);
    for kind in ["node_type", "condition", "action", "behavior"] {
        assert!(
            found.iter().any(|(k, _)| k == kind),
            "no entry of kind {kind}"
        );
    }
    assert_eq!(found, base_pairs(), "the catalog is Catalog::from_registry");
    for entry in catalog["entries"].as_array().expect("entries") {
        let version = entry["version"].as_str().expect("version");
        assert_eq!(version.split('.').count(), 3, "semver: {version}");
        assert_eq!(entry["provenance"], "builtin", "{entry}");
        assert_eq!(entry["read_only"], true, "{entry}");
        assert!(entry["params_schema"].is_object(), "{entry}");
        assert!(entry["description"].is_string());
    }
}

#[tokio::test]
#[ignore = "WI-14"]
async fn catalog_when_idle_uses_the_base_registry() {
    let h = Harness::start("cat-idle");
    let idle = pairs(&h.get("/api/v1/catalog/behavior").await.assert_json(200));
    assert_eq!(idle, base_pairs());
    // A session with its own behavior graphs adds entries; the base entries stay.
    h.load_body(&json!({"path": "patrol_and_engage/scenario.toml", "clock": {"mode": "stepped"}}))
        .await
        .assert_json(201);
    let loaded = h.get("/api/v1/catalog/behavior").await.assert_json(200);
    let with_session = pairs(&loaded);
    assert!(with_session.is_superset(&idle));
    assert!(
        with_session.len() > idle.len(),
        "the scenario's graphs are listed"
    );
    assert!(
        loaded["entries"]
            .as_array()
            .expect("entries")
            .iter()
            .any(|e| e["provenance"] == "file"),
        "a graph from a file has file provenance"
    );
}

#[tokio::test]
#[ignore = "WI-14"]
async fn validate_valid_document() {
    let h = Harness::start("cat-valid");
    let result = h
        .post_json(
            "/api/v1/behaviors/validate",
            &json!({"name": "follow_route.behavior.toml", "document": follow_route_graph()}),
        )
        .await
        .assert_json(200);
    assert_eq!(result["valid"], true, "{result}");
    assert_eq!(result["diagnostics"], json!([]));
}

#[tokio::test]
#[ignore = "WI-14"]
async fn validate_invalid_document_returns_every_diagnostic_with_location() {
    let h = Harness::start("cat-invalid");
    let source = follow_route_graph();
    assert!(source.contains("builtin.action.follow_route") && source.contains("root = \"drive\""));
    let broken = follow_route_graph()
        .replace("builtin.action.follow_route", "builtin.action.nope")
        .replace("root = \"drive\"", "root = \"missing\"");
    assert_ne!(broken, follow_route_graph());
    let reply = h
        .post_json(
            "/api/v1/behaviors/validate",
            &json!({"name": "probe.behavior.toml", "document": broken}),
        )
        .await;
    let result = reply.assert_json(200);
    assert_eq!(result["valid"], false);
    let diagnostics = result["diagnostics"].as_array().expect("diagnostics");
    assert!(diagnostics.len() >= 2, "both errors are reported: {result}");
    for diagnostic in diagnostics {
        assert_eq!(diagnostic["file"], "probe.behavior.toml", "{diagnostic}");
        assert!(
            diagnostic["line"].as_u64().is_some_and(|l| l >= 1),
            "{diagnostic}"
        );
        assert!(diagnostic["code"].is_string());
    }
}

#[tokio::test]
#[ignore = "WI-14"]
async fn validate_changes_no_state() {
    // The same stepped session with and without a validation in the middle reaches the same hash.
    let plain = Harness::start("cat-validate-plain");
    plain.load_skeleton("stepped").await;
    plain.run_command("start").await.assert_json(200);
    let expected = plain.step(25).await.assert_json(200);

    let probed = Harness::start("cat-validate-probed");
    probed.load_skeleton("stepped").await;
    probed.run_command("start").await.assert_json(200);
    let before = probed.get("/api/v1/status").await.assert_json(200);
    for document in [follow_route_graph(), "garbage = ".to_string()] {
        probed
            .post_json("/api/v1/behaviors/validate", &json!({"document": document}))
            .await
            .assert_json(200);
    }
    assert_eq!(probed.get("/api/v1/status").await.assert_json(200), before);
    let got = probed.step(25).await.assert_json(200);
    assert_eq!(got["state_hash"], expected["state_hash"]);
    assert_eq!(got["reached"], expected["reached"]);
}

#[tokio::test]
#[ignore = "WI-14"]
async fn validate_body_too_large_is_413() {
    let h = Harness::with("cat-413", |config| config.server.body_limit_bytes = 2048);
    let document = "# padding\n".repeat(1000);
    h.post_json("/api/v1/behaviors/validate", &json!({"document": document}))
        .await
        .assert_problem(ProblemCode::PayloadTooLarge);
}

#[tokio::test]
#[ignore = "WI-14"]
async fn plugins_empty_when_idle() {
    let h = Harness::start("cat-plugins-idle");
    let report = h.get("/api/v1/plugins").await.assert_json(200);
    assert_eq!(report["plugins"], json!([]));
    assert!(report.get("fault").is_none_or(Value::is_null));
}
