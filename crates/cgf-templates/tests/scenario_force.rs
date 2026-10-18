//! WI-14 rows: the placement key `force`, scenarios that load behavior graphs, and agreement of
//! `validate_path` and `load_scenario` on graph errors.
//! Requirements: REQ-B-6, REQ-T-1, REQ-T-2, ADR 0010 (additive keys), ADR 0015, ASM-074, ASM-079.
//!
//! Step 4 of M3: written before WI-14 (and WI-12 for graph discovery). Tests whose behavior exists
//! already are live; the others are ignored with the work item that makes them pass. The scenario
//! fixtures are in `tests/fixtures/graphs/` (see its README); in-memory scenarios use the standard
//! stub registry of `graph_support`.

#![forbid(unsafe_code)]

mod common;
mod graph_support;

use cgf_core::Force;
use common::{errors, fixture_path, lines, ok, pos_after, pos_of, row, rows, src};
use graph_support::registry;

const SCENARIO: &str = "scenario.toml";

/// A scenario with one entity of template `x.veh` and the given extra entity lines.
fn scenario_text(entity_lines: &[&str]) -> String {
    let mut all = vec![
        "schema_version = 1",
        "[scenario]",
        "name = \"force\"",
        "seed = 1",
        "tick_rate_hz = 30",
        "clock = \"afap\"",
        "duration_s = 1.0",
        "[[entities]]",
        "template = \"x.veh\"",
        "position = { lat_deg = 52.0, lon_deg = 4.0 }",
        "heading_deg = 0.0",
    ];
    all.extend_from_slice(entity_lines);
    lines(&all)
}

fn veh() -> String {
    lines(&[
        "schema_version = 1",
        "id = \"x.veh\"",
        "[motion]",
        "model = \"stub.motion.alpha\"",
        "params = { a = 1.0 }",
    ])
}

fn load(text: &str) -> Result<cgf_templates::Scenario, cgf_templates::Diagnostics> {
    let registry = registry();
    common::loader(&registry)
        .scenario_from_sources(&src(SCENARIO, text), &[src("veh.toml", &veh())])
}

/// ADR 0015, ASM-079: an absent `force` is neutral (`None`).
#[test]
fn placement_without_force_is_neutral() {
    let scenario = ok(load(&scenario_text(&[])));

    assert_eq!(scenario.entities[0].force, None);
}

/// ADR 0015: a valid `force` is kept on the placement.
#[test]
fn placement_force_is_kept() {
    let scenario = ok(load(&scenario_text(&["force = \"blue_team_2\""])));

    assert_eq!(scenario.entities[0].force, Force::new("blue_team_2").ok());
}

/// ASM-079: `force` follows `[a-z][a-z0-9_]*`, at most `MAX_ID_LEN` bytes; a violation is
/// `invalid_value` at the value (with a bounded echo of the offender); a non-string is
/// `wrong_type`.
#[test]
fn force_grammar_is_checked_at_the_value() {
    let long = "f".repeat(129);
    for bad in ["Blue", "1st", "blue-team", "", long.as_str()] {
        let text = scenario_text(&[&format!("force = \"{bad}\"")]);

        let found = errors(load(&text));

        assert_eq!(
            rows(&found),
            [row("invalid_value", SCENARIO, pos_after(&text, "force = "))],
            "{bad:?}"
        );
        assert!(
            found[0].message.len() < 400,
            "bounded echo: {}",
            found[0].message.len()
        );
    }
    let text = scenario_text(&["force = 5"]);
    assert_eq!(
        rows(&errors(load(&text))),
        [row("wrong_type", SCENARIO, pos_after(&text, "force = "))]
    );
}

/// ADR 0010: `force` is an additive key: the scenario file schema lists it as an optional property
/// of an entity, and `schema_version` stays 1.
#[test]
fn scenario_file_schema_lists_force_as_optional_and_version_stays_one() {
    let schema: serde_json::Value = serde_json::from_str(cgf_templates::SCENARIO_FILE_SCHEMA)
        .unwrap_or_else(|e| panic!("scenario schema is JSON: {e}"));
    let entity = &schema["$defs"]["entity"];

    assert!(entity["properties"]["force"].is_object(), "{entity}");
    assert!(
        !entity["required"]
            .as_array()
            .is_some_and(|r| r.iter().any(|k| k == "force")),
        "force is optional"
    );
    assert_eq!(cgf_templates::SCHEMA_VERSION, 1);
}

/// ASM-074: a scenario loaded from disk reads the `*.behavior.toml` files under its
/// `template_paths`: the graph is in `Scenario::graphs`, the template resolves to it, and the
/// placement carries its force.
#[test]
fn scenario_loads_graphs_from_its_template_paths() {
    let registry = registry();

    let scenario =
        ok(common::loader(&registry)
            .load_scenario(&fixture_path("graphs/scenario_ok/scenario.toml")));

    assert_eq!(scenario.graphs.len(), 1);
    assert!(
        scenario
            .graphs
            .iter()
            .any(|g| g.id.as_str() == "test.graph.patrol")
    );
    let blue = &scenario.entities[0];
    assert_eq!(blue.force, Force::new("blue").ok());
    assert_eq!(
        blue.behavior.as_ref().map(|b| b.id.as_str().to_string()),
        Some("test.graph.patrol".to_string())
    );
    assert_eq!(scenario.entities[1].force, None);
}

/// REQ-T-6: `validate_path` and `load_scenario` agree on graph errors: the same diagnostics, in
/// the same order, at the graph file's positions.
#[test]
fn validate_path_and_load_scenario_agree_on_graph_errors() {
    let registry = registry();
    let reader = common::loader(&registry);
    let scenario = fixture_path("graphs/scenario_bad_graph/scenario.toml");
    let graph = fixture_path("graphs/scenario_bad_graph/templates/patrol.behavior.toml");
    let text = common::read_fixture("graphs/scenario_bad_graph/templates/patrol.behavior.toml");

    let loaded = errors(reader.load_scenario(&scenario));
    let validated = reader.validate_path(&scenario);

    assert_eq!(loaded, validated.diagnostics);
    assert_eq!(
        rows(&loaded),
        [
            row("unknown_node", &graph, pos_of(&text, "\"ghost\"")),
            row("unreachable_node", &graph, pos_of(&text, "idle")),
        ]
    );
}

const GRAPH_PATH: &str = "behaviors/patrol.behavior.toml";

fn patrol() -> String {
    lines(&[
        "schema_version = 1",
        "id = \"test.graph.patrol\"",
        "version = \"1.0.0\"",
        "root = \"main\"",
        "[nodes.main]",
        "type = \"action\"",
        "ref = \"stub.action.noop\"",
    ])
}

fn armed_veh() -> String {
    lines(&[
        "schema_version = 1",
        "id = \"x.veh\"",
        "[motion]",
        "model = \"stub.motion.alpha\"",
        "params = { a = 1.0 }",
        "[behavior]",
        "ref = \"test.graph.patrol\"",
        "[[sensors]]",
        "kind = \"stub.sensor.eye\"",
        "params = { range_m = 900.0 }",
    ])
}

/// REQ-B-6 (ASM-083): `scenario_from_sources` takes graph sources among the template sources; the
/// scenario carries the graph, and every placement carries its template's behavior and sensors.
#[test]
fn scenario_from_sources_with_graph_sources() {
    let registry = registry();
    let text = scenario_text(&["force = \"red\""]);

    let scenario = ok(common::loader(&registry).scenario_from_sources(
        &src(SCENARIO, &text),
        &[src("veh.toml", &armed_veh()), src(GRAPH_PATH, &patrol())],
    ));

    assert_eq!(
        scenario
            .graphs
            .iter()
            .map(|g| g.id.as_str())
            .collect::<Vec<_>>(),
        ["test.graph.patrol"]
    );
    let placement = &scenario.entities[0];
    assert_eq!(
        placement.behavior.as_ref().map(|b| b.id.as_str()),
        Some("test.graph.patrol")
    );
    assert_eq!(
        placement
            .sensors
            .iter()
            .map(|s| (s.id.as_str(), s.params.clone()))
            .collect::<Vec<_>>(),
        [("stub.sensor.eye", serde_json::json!({ "range_m": 900.0 }))]
    );
    assert_eq!(placement.force, Force::new("red").ok());
}

/// A scenario with no graph files has an empty graph set and placements without a behavior (M2
/// behavior unchanged).
#[test]
fn scenario_without_graphs_has_none() {
    let scenario = ok(load(&scenario_text(&[])));

    assert!(scenario.graphs.is_empty());
    assert_eq!(scenario.entities[0].behavior, None);
    assert_eq!(scenario.entities[0].sensors.len(), 0);
}

/// A graph error in the sources of `scenario_from_sources` is reported there too.
#[test]
fn scenario_from_sources_reports_graph_errors() {
    let registry = registry();
    let broken = patrol().replace("root = \"main\"", "root = \"ghost\"");

    let found = errors(common::loader(&registry).scenario_from_sources(
        &src(SCENARIO, &scenario_text(&[])),
        &[src("veh.toml", &veh()), src(GRAPH_PATH, &broken)],
    ));

    assert_eq!(
        rows(&found),
        [row(
            "unknown_node",
            GRAPH_PATH,
            pos_of(&broken, "\"ghost\"")
        )]
    );
}

/// The scenario file schema accepts a placement with `force` (and the reader agrees).
#[test]
fn scenario_file_schema_accepts_a_force() {
    let schema = cgf_core::ParamSchema::from_json_str(cgf_templates::SCENARIO_FILE_SCHEMA)
        .unwrap_or_else(|e| panic!("scenario schema is JSON: {e}"));
    let compiled = cgf_templates::CompiledSchema::compile(&schema)
        .unwrap_or_else(|e| panic!("scenario schema compiles: {e}"));
    let file = src(SCENARIO, &scenario_text(&["force = \"blue_team_2\""]));
    let (table, syntax) = cgf_templates::value::parse(&file);
    assert_eq!(syntax.len(), 0);
    let root = table.unwrap_or_else(|| panic!("a root table")).into_value();

    assert_eq!(compiled.validate(&root, "scenario").len(), 0);
}
