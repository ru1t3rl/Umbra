//! AT-2 (`Session::validate_graph`, M5 WI-5): REQ-X-11 validation of a posted behavior graph
//! document together with the session's other file graphs, inside the plugin build scope, without
//! changing the session. Requirements: REQ-X-11, REQ-B-13, REQ-B-17, ASM-154.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]
#![expect(clippy::panic, reason = "tests fail by panicking")]

mod plugin_support;

use core::time::Duration;
use std::path::{Path, PathBuf};

use umbra_core::Registry;
use umbra_engine::builtins::standard_registry;
use umbra_engine::scenario::{DiagnosticCode, validate_path};
use umbra_engine::{
    GraphValidation, ManualWallClock, Pumped, Session, SessionOptions, validate_graph_document,
};

fn example(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/scenarios")
        .join(name)
        .join("scenario.toml")
}

fn registry() -> Registry {
    standard_registry().expect("the standard registry builds")
}

fn session(scenario: &Path, plugins: Vec<PathBuf>) -> Session {
    let mut options = SessionOptions::new(Box::new(ManualWallClock::with_auto_advance(
        Duration::from_micros(500),
    )));
    options.load.extra_plugin_paths = plugins;
    Session::load(&registry(), scenario, options).unwrap_or_else(|e| panic!("load: {e}"))
}

fn patrol() -> Session {
    session(&example("patrol_and_engage"), Vec::new())
}

/// A graph whose only node calls `target` as a `subgraph`. The `ref` is on line 8.
fn calling(id: &str, target: &str) -> String {
    format!(
        "schema_version = 1\nid = \"{id}\"\nversion = \"1.0.0\"\nroot = \"call\"\n\n\
         [nodes.call]\ntype = \"subgraph\"\nref = \"{target}\"\n"
    )
}

fn check(session: &Session, name: &str, document: &str) -> GraphValidation {
    session
        .validate_graph(name, document)
        .unwrap_or_else(|e| panic!("validate_graph: {e}"))
}

fn codes(v: &GraphValidation) -> Vec<DiagnosticCode> {
    v.diagnostics.iter().map(|d| d.code).collect()
}

#[test]
fn valid_document_has_no_diagnostics() {
    let session = patrol();
    let doc = calling(
        "test.behavior.calls_follow",
        "example.behavior.follow_route",
    );
    let result = check(&session, "draft.behavior.toml", &doc);
    assert!(result.valid, "{:?}", result.diagnostics);
    assert_eq!(result.diagnostics, Vec::new());
}

#[test]
fn errors_are_located_at_the_given_name() {
    let session = patrol();
    let doc = calling("test.behavior.calls_nothing", "no.such.graph");
    let result = check(&session, "draft.behavior.toml", &doc);
    assert!(!result.valid);
    let located: Vec<_> = result
        .diagnostics
        .iter()
        .filter_map(|d| d.location.as_ref())
        .collect();
    assert_ne!(located.len(), 0);
    assert!(
        located
            .iter()
            .all(|l| l.path == Path::new("draft.behavior.toml")),
        "{located:?}"
    );
    assert!(
        located.iter().any(|l| l.line == 8),
        "the unknown ref is on line 8: {located:?}"
    );
}

/// The posted document stands for the session graph with the same ID: that one is left out, so the
/// ID is not a duplicate (and the document is judged on its own text).
#[test]
fn document_replaces_session_graph_with_same_id_without_duplicate_id() {
    let session = patrol();
    // Comments and the ID after other keys must not hide the ID from the replacement.
    let doc = "# my edit\nschema_version = 1\nversion = \"2.0.0\"\nid = \"example.behavior.follow_route\"\n\
               root = \"drive\"\n\n[nodes.drive]\ntype = \"action\"\nref = \"builtin.action.follow_route\"\n";
    let result = check(&session, "follow_route.behavior.toml", doc);
    assert!(
        !codes(&result).contains(&DiagnosticCode::DuplicateId),
        "{:?}",
        result.diagnostics
    );
    assert!(result.valid, "{:?}", result.diagnostics);
}

/// ASM-154: other file graphs are validated with the document, so a document that breaks a call
/// site in another graph is reported there.
#[test]
fn call_site_errors_in_other_session_graphs_are_reported() {
    let session = patrol();
    // `attack` without the interface its caller (patrol_and_engage) binds `target` to.
    let doc = "schema_version = 1\nid = \"example.subgraph.attack\"\nversion = \"1.0.0\"\n\
               root = \"go\"\n\n[nodes.go]\ntype = \"action\"\nref = \"builtin.action.follow_route\"\n";
    let result = check(&session, "attack.behavior.toml", doc);
    assert!(!result.valid);
    let in_caller = result.diagnostics.iter().any(|d| {
        d.location.as_ref().is_some_and(|l| {
            l.path
                .file_name()
                .is_some_and(|n| n == "patrol_and_engage.behavior.toml")
        })
    });
    assert!(in_caller, "{:?}", result.diagnostics);
}

/// The diagnostics of a posted document are those of `umbra validate` on the same text in a file.
#[test]
fn diagnostics_equal_umbra_validate_for_the_same_file() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("graph-validation-equal");
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let file = dir.join("broken.behavior.toml");
    let documents = [
        "schema_version = 1\nid = \"test.behavior.broken\"\nversion = \"1.0.0\"\nroot = \"a\"\n\n\
         [nodes.a]\ntype = \"no_such_node\"\n",
        &calling("test.behavior.broken", "no.such.graph"),
    ];
    let session = patrol();
    for document in documents {
        std::fs::write(&file, document).expect("write");
        let want = validate_path(&registry(), &file)
            .expect("validate")
            .diagnostics;
        assert_ne!(want.len(), 0);
        let got = check(&session, file.to_str().expect("utf-8 path"), document);
        assert_eq!(got.diagnostics, want);
        assert!(!got.valid);
    }
}

/// The free function judges a document alone against a registry.
#[test]
fn validate_graph_document_alone() {
    let registry = registry();
    let ok = calling("test.behavior.alone", "builtin.fsm.patrol_and_engage");
    let result = validate_graph_document(&registry, "alone.behavior.toml", &ok).expect("validate");
    assert!(result.valid, "{:?}", result.diagnostics);
    let bad = calling("test.behavior.alone", "no.such.graph");
    let result = validate_graph_document(&registry, "alone.behavior.toml", &bad).expect("validate");
    assert!(!result.valid);
}

/// REQ-X-11: validation reads and compares; the session's state, entities and future ticks are the
/// same as in a session that was never asked.
#[test]
fn validation_changes_no_session_state() {
    let mut asked = patrol();
    let mut control = patrol();
    let entities = asked.entities().clone();
    let hash = asked.engine().state_hash();
    let graphs = asked.scenario().graphs.len();

    let valid = calling(
        "test.behavior.calls_follow",
        "example.behavior.follow_route",
    );
    let replacing = calling(
        "example.behavior.follow_route",
        "example.behavior.patrol_and_engage",
    );
    let broken = calling("test.behavior.broken", "no.such.graph");
    for (name, doc) in [("a", &valid), ("b", &replacing), ("c", &broken)] {
        let _ = check(&asked, &format!("{name}.behavior.toml"), doc);
    }

    assert_eq!(asked.entities(), &entities);
    assert_eq!(asked.engine().state_hash(), hash);
    assert_eq!(asked.engine().now().tick(), 0);
    assert_eq!(asked.scenario().graphs.len(), graphs);
    for session in [&mut asked, &mut control] {
        session.engine_mut().resume();
        for _ in 0..60 {
            assert!(matches!(
                session.engine_mut().pump(Duration::from_millis(10)),
                Ok(Pumped::Ticked(_))
            ));
        }
    }
    assert_eq!(asked.engine().state_hash(), control.engine().state_hash());
}

/// M6, REQ-X-11: a plugin behavior referenced by a posted graph dry-runs inside the session's build
/// scope, so the document is valid (the example plugin is the real `example-wander` library).
#[test]
fn validate_graph_referencing_plugin_behavior_is_valid() {
    let wander = plugin_support::wander();
    let session = session(&example("plugin_wander"), vec![wander]);
    let doc = calling("test.behavior.calls_wander", "example.behavior.wander");
    let result = check(&session, "draft.behavior.toml", &doc);
    assert!(result.valid, "{:?}", result.diagnostics);
    // The graph file the scenario itself uses (which calls the plugin) stays valid alongside it.
    let again = check(
        &session,
        "other.behavior.toml",
        &calling("test.behavior.second", "example.behavior.wander_graph"),
    );
    assert!(again.valid, "{:?}", again.diagnostics);
}
