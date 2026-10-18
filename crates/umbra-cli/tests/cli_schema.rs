//! AT-10 (CLI part): `umbra schema template|scenario|component <ID>`, run as a real process.
//! Requirements: D-14, REQ-T-3, REQ-X-2 (`schema`), ADR 0008.
//!
//! `schema component` serves the schema a registered component supplies itself (the registry is
//! the single source); an unknown ID is exit 1; usage errors are exit 2.

#![forbid(unsafe_code)]

mod common;

use common::{assert_usage_error, cli, json_of, stderr, stdout};
use umbra_engine::builtins::builtin_registry;
use umbra_engine::{ComponentId, ComponentKind, SCENARIO_FILE_SCHEMA, TEMPLATE_FILE_SCHEMA};

fn registered(kind: ComponentKind, id: &str) -> serde_json::Value {
    let registry = builtin_registry().unwrap_or_else(|e| panic!("registry: {e}"));
    let id = ComponentId::new(id).unwrap_or_else(|e| panic!("id: {e}"));
    registry
        .param_schema(kind, &id)
        .unwrap_or_else(|| panic!("{id} is not registered"))
        .as_json()
        .clone()
}

#[test]
fn schema_component_prints_registered_schema() {
    let out = cli(&["schema", "component", "builtin.ground.kinematic"]);

    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    assert_eq!(
        json_of(&out),
        registered(ComponentKind::MotionModel, "builtin.ground.kinematic")
    );
}

#[test]
fn schema_component_searches_every_kind() {
    let out = cli(&["schema", "component", "builtin.terrain.flat"]);

    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    assert_eq!(
        json_of(&out),
        registered(ComponentKind::Terrain, "builtin.terrain.flat")
    );
}

#[test]
fn schema_unknown_component_exit_1() {
    let out = cli(&["schema", "component", "acme.no.such"]);

    assert_eq!(out.status.code(), Some(1));
    assert_eq!(stdout(&out), "");
    assert!(stderr(&out).contains("acme.no.such"), "{}", stderr(&out));
}

#[test]
fn schema_component_with_an_invalid_id_is_exit_1() {
    let out = cli(&["schema", "component", "Not An Id"]);

    assert_eq!(out.status.code(), Some(1));
    assert_ne!(stderr(&out).trim(), "");
}

#[test]
fn schema_template_and_scenario_parse_as_json() {
    for which in ["template", "scenario"] {
        let out = cli(&["schema", which]);

        assert_eq!(out.status.code(), Some(0), "{which}: {}", stderr(&out));
        let doc = json_of(&out);
        assert!(doc["title"].is_string(), "{which}: {doc}");
        assert_eq!(doc["type"], "object", "{which}");
    }
}

#[test]
fn schema_template_and_scenario_print_the_published_file_schemas() {
    for (which, published) in [
        ("template", TEMPLATE_FILE_SCHEMA),
        ("scenario", SCENARIO_FILE_SCHEMA),
    ] {
        let out = cli(&["schema", which]);

        let expected: serde_json::Value = serde_json::from_str(published)
            .unwrap_or_else(|e| panic!("the {which} file schema constant is not JSON: {e}"));
        assert_eq!(json_of(&out), expected, "{which}");
    }
}

#[test]
fn schema_without_a_subcommand_is_a_usage_error() {
    assert_usage_error(&cli(&["schema"]), "schema");
}

#[test]
fn schema_with_an_unknown_subcommand_is_a_usage_error() {
    let out = cli(&["schema", "bogus"]);

    assert_eq!(out.status.code(), Some(2));
    assert!(
        stderr(&out).contains("unrecognized subcommand 'bogus'"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn schema_component_without_an_id_is_a_usage_error() {
    assert_usage_error(&cli(&["schema", "component"]), "required");
}
