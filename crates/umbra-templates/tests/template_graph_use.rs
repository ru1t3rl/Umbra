//! WI-14: a template's `[behavior]` use of a graph of the same load goes through the same checks
//! as a registered behavior (`check_component`, one path) and as a `subgraph` call site (values
//! followed down the chain). Requirements: REQ-B-6, REQ-B-10, REQ-T-1, REQ-T-3, ADR 0010,
//! ADR 0014, ASM-056, ASM-083, ASM-106, ASM-107.

#![forbid(unsafe_code)]

mod common;
mod graph_support;

use common::{errors, fixture_path, lines, ok, pos_after, pos_of, row, rows, src};
use graph_support::{caller, minimal, one_graph, register_twin, registry, with_interface};
use umbra_core::{BbType, Registry};
use umbra_templates::{Diagnostic, SourceFile, TemplateSet};

const GRAPH: &str = "behaviors/patrol.behavior.toml";
const TEMPLATE: &str = "templates/tank.toml";
const CHILD_PARAMS: &str = "params = { count = { type = \"integer\", minimum = 1 }, label = { type = \"string\", default = \"x\" } }";

fn patrol() -> String {
    lines(&[
        "schema_version = 1",
        "id = \"test.graph.patrol\"",
        "version = \"1.0.0\"",
        "root = \"main\"",
        "[interface]",
        "params = { engage_range_m = { type = \"number\", exclusiveMinimum = 0, default = 2500.0 } }",
        "[nodes.main]",
        "type = \"condition\"",
        "ref = \"stub.cond.near\"",
        "params = { range_m = { param = \"engage_range_m\" } }",
    ])
}

/// A template with `[behavior] ref = <graph>` and the given `params` value (may be empty).
fn tank(graph: &str, params: &str) -> String {
    let mut all = vec![
        "schema_version = 1".to_string(),
        "id = \"tank.blue\"".to_string(),
        "[motion]".to_string(),
        "model = \"stub.motion.alpha\"".to_string(),
        "params = { a = 1.0 }".to_string(),
        "[behavior]".to_string(),
        format!("ref = \"{graph}\""),
    ];
    if !params.is_empty() {
        all.push(format!("params = {params}"));
    }
    let refs: Vec<&str> = all.iter().map(String::as_str).collect();
    lines(&refs)
}

fn load(files: &[SourceFile]) -> Result<TemplateSet, umbra_templates::Diagnostics> {
    let registry = registry();
    common::loader(&registry).templates_from_sources(files)
}

fn load_with(graph: &str, tank: &str) -> Result<TemplateSet, umbra_templates::Diagnostics> {
    load(&[src(GRAPH, graph), src(TEMPLATE, tank)])
}

/// Row summary and message of each diagnostic, with `from` written as `to` in the messages.
fn shape(found: &[Diagnostic], from: &str, to: &str) -> Vec<(String, u32, u32, String)> {
    found
        .iter()
        .map(|d| {
            let at = d.location.as_ref().map_or((0, 0), |l| (l.line, l.column));
            (
                d.code.as_str().to_string(),
                at.0,
                at.1,
                d.message.replace(from, to),
            )
        })
        .collect()
}

/// Loads `params` as the template's `[behavior] params` once with the graph and once with its
/// registered twin, and checks that both succeed with the same params or fail with the same
/// diagnostics (codes, positions, messages but for the ID).
fn assert_same(graph_text: &str, graph: &str, twin_registry: &Registry, twin: &str, params: &str) {
    let by_graph = load(&[
        src("behaviors/child.behavior.toml", graph_text),
        src(TEMPLATE, &tank(graph, params)),
    ]);
    let by_twin =
        common::loader(twin_registry).templates_from_sources(&[src(TEMPLATE, &tank(twin, params))]);

    match (by_graph, by_twin) {
        (Ok(g), Ok(t)) => {
            let behavior = |set: &TemplateSet| {
                set.iter()
                    .next()
                    .and_then(|r| r.behavior.as_ref().map(|b| b.params.clone()))
            };
            assert_eq!(behavior(&g), behavior(&t), "{params}");
        }
        (Err(g), Err(t)) => {
            let g: Vec<Diagnostic> = g.iter().cloned().collect();
            let t: Vec<Diagnostic> = t.iter().cloned().collect();
            assert_eq!(shape(&g, graph, "ID"), shape(&t, twin, "ID"), "{params}");
        }
        (g, t) => panic!(
            "{params}: graph ok = {}, twin ok = {}",
            g.is_ok(),
            t.is_ok()
        ),
    }
}

/// Plan risk: the scoped overlay must not diverge from the registry path. A graph and a
/// registered behavior with the same parameter schema give the same diagnostics for every
/// schema-level mistake, no `params` line, and binding forms, and the same params when valid.
#[test]
fn graph_and_registered_twin_with_the_same_schema_agree() {
    let child = with_interface("test.graph.child", CHILD_PARAMS);
    let schema = one_graph(&child).interface.params.clone();
    let mut twin_registry = registry();
    register_twin(&mut twin_registry, "test.twin.child", &schema, &[]);
    let cases = [
        "",
        "{ count = 1 }",
        "{ count = 1, label = \"y\" }",
        "{ }",
        "{ count = 1, extra = 2 }",
        "{ count = \"many\" }",
        "{ count = 0 }",
        "{ count = 1, label = 5 }",
        "{ count = 1.5 }",
        "{ count = { key = \"k\" } }",
        "{ count = 1, label = { param = \"p\" } }",
    ];
    for params in cases {
        assert_same(
            &child,
            "test.graph.child",
            &twin_registry,
            "test.twin.child",
            params,
        );
    }
}

/// The same with a binding-only param (`target: entity`): a graph and a registered behavior that
/// declares it bindable both require it at a template (ADR 0014).
#[test]
fn graph_and_bindable_registered_twin_agree() {
    let bound = with_interface(
        "test.graph.bound",
        "params = { target = { type = \"entity\" }, n = { type = \"integer\", default = 1 } }",
    );
    let schema = one_graph(&bound).interface.params.clone();
    let mut twin_registry = registry();
    register_twin(
        &mut twin_registry,
        "test.twin.bound",
        &schema,
        &[("target", BbType::Entity), ("n", BbType::Integer)],
    );
    for params in [
        "",
        "{ }",
        "{ n = 2 }",
        "{ n = \"x\" }",
        "{ target = { key = \"k\" } }",
        "{ target = { key = \"k\" }, n = 2 }",
    ] {
        assert_same(
            &bound,
            "test.graph.bound",
            &twin_registry,
            "test.twin.bound",
            params,
        );
    }
}

/// ASM-106 d: the value a template passes is followed into the graph's leaves like a call-site
/// value: the leaf's dry run refuses 13.0, shown at the template's value with a note at the
/// `{ param }` reference.
#[test]
fn template_value_is_checked_against_the_leaf_that_uses_it() {
    let tank = tank("test.graph.patrol", "{ engage_range_m = 13.0 }");

    let d = errors(load_with(&patrol(), &tank));

    assert_eq!(
        rows(&d),
        [row(
            "param_invalid",
            TEMPLATE,
            pos_after(&tank, "engage_range_m = ")
        )]
    );
    assert!(
        d[0].notes.iter().any(|n| n.location.is_some()),
        "a note at the `{{ param }}` reference: {:?}",
        d[0].notes
    );
}

/// A leaf range (`range_m <= 100000`) that the interface itself does not state.
#[test]
fn template_value_out_of_the_leaf_range_is_reported_at_the_template() {
    let tank = tank("test.graph.patrol", "{ engage_range_m = 200000.0 }");

    let d = errors(load_with(&patrol(), &tank));

    assert_eq!(
        rows(&d),
        [row(
            "param_out_of_range",
            TEMPLATE,
            pos_after(&tank, "engage_range_m = ")
        )]
    );
}

/// The value is followed through a `subgraph` node as well (a chain of two graphs); the note names
/// the chain.
#[test]
fn template_value_is_followed_through_a_subgraph_node() {
    let outer = caller(
        "test.graph.outer",
        "test.graph.patrol",
        None,
        "{ engage_range_m = { param = \"r\" } }",
        &[
            "[interface]",
            "params = { r = { type = \"number\", exclusiveMinimum = 0, default = 500.0 } }",
        ],
    );
    let tank = tank("test.graph.outer", "{ r = 13.0 }");

    let d = errors(load(&[
        src(GRAPH, &patrol()),
        src("behaviors/outer.behavior.toml", &outer),
        src(TEMPLATE, &tank),
    ]));

    assert_eq!(
        rows(&d),
        [row("param_invalid", TEMPLATE, pos_after(&tank, "r = "))]
    );
    assert!(
        d[0].notes
            .iter()
            .any(|n| n.message.contains("test.graph.patrol")),
        "{:?}",
        d[0].notes
    );
}

/// ADR 0014: a `{ key }` form anywhere in the params (also nested) and a `{ param }` form are
/// `invalid_binding` at the form; a required param supplied that way is not also `missing_param`.
#[test]
fn binding_forms_in_template_params_are_invalid_binding_without_cascade() {
    let child = with_interface("test.graph.child", CHILD_PARAMS);
    let cases = [
        ("{ count = { key = \"k\" } }", "{ key = \"k\" }"),
        (
            "{ count = 1, label = { param = \"p\" } }",
            "{ param = \"p\" }",
        ),
        ("{ count = [ { key = \"k\" } ] }", "{ key = \"k\" }"),
    ];
    for (params, form) in cases {
        let tank = tank("test.graph.child", params);

        let d = errors(load(&[
            src("behaviors/child.behavior.toml", &child),
            src(TEMPLATE, &tank),
        ]));

        assert_eq!(
            rows(&d),
            [row("invalid_binding", TEMPLATE, pos_of(&tank, form))],
            "{params}"
        );
    }
}

/// ASM-056: a template that names a graph whose ID is a duplicate gets no `unknown_id`.
#[test]
fn template_ref_to_a_duplicate_graph_has_no_cascade() {
    let tank = tank("test.graph.patrol", "{ engage_range_m = 1.0 }");

    let d = errors(load(&[
        src("behaviors/a.behavior.toml", &patrol()),
        src("behaviors/b.behavior.toml", &patrol()),
        src(TEMPLATE, &tank),
    ]));

    assert_eq!(
        d.iter().map(|d| d.code.as_str()).collect::<Vec<_>>(),
        ["duplicate_id"]
    );
}

/// ASM-054 rule 3: when a graph file with a syntax error has no readable ID, an unknown behavior is
/// still `unknown_id`, with a note about the file.
#[test]
fn unknown_behavior_notes_unreadable_graph_files() {
    let tank = tank("test.graph.nowhere", "");

    let d = errors(load(&[
        src("behaviors/broken.behavior.toml", "id = [\n"),
        src(TEMPLATE, &tank),
    ]));

    let unknown: Vec<&Diagnostic> = d
        .iter()
        .filter(|d| d.code.as_str() == "unknown_id")
        .collect();
    assert_eq!(unknown.len(), 1, "{d:?}");
    assert!(
        unknown[0]
            .notes
            .iter()
            .any(|n| n.message.contains("broken.behavior.toml")),
        "{:?}",
        unknown[0].notes
    );
}

/// ASM-056 with graphs: a template whose parent is missing is still checked for what it states
/// itself, including its params against the graph it names.
#[test]
fn template_with_unknown_parent_is_still_checked_against_its_graph() {
    let text = lines(&[
        "schema_version = 1",
        "id = \"tank.child\"",
        "extends = \"tank.nowhere\"",
        "[behavior]",
        "ref = \"test.graph.patrol\"",
        "params = { extra = 1 }",
    ]);

    let d = errors(load(&[src(GRAPH, &patrol()), src(TEMPLATE, &text)]));

    assert_eq!(
        rows(&d),
        [
            row(
                "unknown_template",
                TEMPLATE,
                pos_of(&text, "\"tank.nowhere\"")
            ),
            row("unknown_param", TEMPLATE, pos_of(&text, "extra")),
        ]
    );
}

/// A child that only writes `params` of an inherited graph behavior is checked against that
/// graph even when the chain cannot be resolved.
#[test]
fn own_params_of_an_inherited_graph_are_checked_when_the_chain_is_broken() {
    let parent = tank("test.graph.patrol", "");
    let child = lines(&[
        "schema_version = 1",
        "id = \"tank.child\"",
        "extends = \"tank.blue\"",
        "[behavior]",
        "params = { engage_range_m = \"far\" }",
        "[motion]",
        "model = \"stub.motion.nowhere\"",
    ]);

    let d = errors(load(&[
        src(GRAPH, &patrol()),
        src(TEMPLATE, &parent),
        src("templates/child.toml", &child),
    ]));

    assert_eq!(
        rows(&d),
        [
            // The interface refuses the string once; the leaf that uses the param is not asked
            // about a value that is already refused (ASM-107 d).
            row(
                "param_wrong_type",
                "templates/child.toml",
                pos_of(&child, "\"far\"")
            ),
            row(
                "unknown_id",
                "templates/child.toml",
                pos_of(&child, "\"stub.motion.nowhere\"")
            ),
        ]
    );
}

/// REQ-K-4: the order the files are given in does not change the result.
#[test]
fn file_order_does_not_change_the_resolved_templates() {
    let tank = tank("test.graph.patrol", "{ engage_range_m = 1500.0 }");
    let other = minimal("test.graph.other");
    let files = [
        src(GRAPH, &patrol()),
        src("behaviors/other.behavior.toml", &other),
        src(TEMPLATE, &tank),
    ];
    let reversed: Vec<SourceFile> = files.iter().rev().cloned().collect();

    let a = ok(load(&files));
    let b = ok(load(&reversed));

    assert_eq!(a, b);
}

/// A sensor kind that is a graph's ID is not a sensor: IDs are global, only registered components
/// are looked up for sensors.
#[test]
fn a_graph_id_is_not_a_sensor_kind() {
    let text = lines(&[
        "schema_version = 1",
        "id = \"tank.blue\"",
        "[motion]",
        "model = \"stub.motion.alpha\"",
        "params = { a = 1.0 }",
        "[[sensors]]",
        "kind = \"test.graph.patrol\"",
    ]);

    let d = errors(load(&[src(GRAPH, &patrol()), src(TEMPLATE, &text)]));

    assert_eq!(
        rows(&d),
        [row(
            "unknown_id",
            TEMPLATE,
            pos_of(&text, "\"test.graph.patrol\"")
        )]
    );
}

/// `validate_path` and `load_scenario` agree on a call-site error in a template that uses a graph
/// of the scenario's `template_paths`.
#[test]
fn validate_path_and_load_scenario_agree_on_call_site_errors() {
    let registry = registry();
    let reader = common::loader(&registry);
    let scenario = fixture_path("graphs/scenario_bad_call/scenario.toml");
    let template = fixture_path("graphs/scenario_bad_call/templates/tank.toml");
    let text = common::read_fixture("graphs/scenario_bad_call/templates/tank.toml");

    let loaded = errors(reader.load_scenario(&scenario));
    let validated = reader.validate_path(&scenario);

    assert_eq!(loaded, validated.diagnostics);
    assert_eq!(
        rows(&loaded),
        [
            row(
                "param_invalid",
                &template,
                pos_after(&text, "engage_range_m = ")
            ),
            row("unknown_param", &template, pos_of(&text, "extra")),
        ]
    );
}

/// ASM-107 d: a `subgraph` call site behaves like a template use: a value the callee's interface
/// refuses is reported once, not again by the leaf that uses the param.
#[test]
fn refused_call_site_value_is_reported_once() {
    let outer = caller(
        "test.graph.outer",
        "test.graph.patrol",
        None,
        "{ engage_range_m = \"far\" }",
        &[],
    );

    let d = errors(graph_support::graphs(&[
        src(GRAPH, &patrol()),
        src("behaviors/outer.behavior.toml", &outer),
    ]));

    assert_eq!(
        rows(&d),
        [row(
            "param_wrong_type",
            "behaviors/outer.behavior.toml",
            pos_of(&outer, "\"far\"")
        )]
    );
}

const SPOT: &str = r#"{"type":"object","properties":{"spot":{"type":"object","properties":{"x":{"type":"number"},"y":{"type":"number"}},"required":["x","y"],"additionalProperties":false}},"additionalProperties":false,"examples":[{}]}"#;

fn spot_registry() -> Registry {
    let schema =
        umbra_core::ParamSchema::from_json_str(SPOT).unwrap_or_else(|e| panic!("spot schema: {e}"));
    let mut registry = registry();
    register_twin(&mut registry, "test.twin.spot", &schema, &[]);
    registry
}

/// A `missing_param` below the root of a registered behavior's params stays (only the root ones
/// are recomputed by the loader): at a template, and never a silent drop of the template.
#[test]
fn nested_missing_param_of_a_registered_behavior_is_kept_at_a_template() {
    let registry = spot_registry();
    let tank = tank("test.twin.spot", "{ spot = { x = 1.0 } }");

    let d = errors(common::loader(&registry).templates_from_sources(&[src(TEMPLATE, &tank)]));

    assert_eq!(
        rows(&d),
        [row("missing_param", TEMPLATE, pos_of(&tank, "{ x = 1.0 }"))]
    );
}

/// The same at a `subgraph` call site.
#[test]
fn nested_missing_param_of_a_registered_behavior_is_kept_at_a_call_site() {
    let registry = spot_registry();
    let outer = caller(
        "test.graph.outer",
        "test.twin.spot",
        None,
        "{ spot = { x = 1.0 } }",
        &[],
    );

    let d = errors(
        common::loader(&registry)
            .graphs_from_sources(&[src(GRAPH, &outer)], umbra_core::Provenance::File),
    );

    assert_eq!(
        rows(&d),
        [row("missing_param", GRAPH, pos_of(&outer, "{ x = 1.0 }"))]
    );
}
