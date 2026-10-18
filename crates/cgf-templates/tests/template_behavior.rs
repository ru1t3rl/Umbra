//! AT-6 (template `[behavior]` call sites), AT-20 (`key_form_in_template_behavior_params_is_
//! invalid_binding`) and the WI-14 rows: templates use behavior graphs and sensors.
//! Requirements: REQ-B-6, REQ-B-10, REQ-B-12, REQ-T-1, REQ-T-3, ADR 0010, ADR 0014, ADR 0015,
//! ASM-055, ASM-056, ASM-083.
//!
//! Step 4 of M3: written before WI-14 (now all live). Graph sources
//! and template sources are loaded together through `Loader::templates_from_sources` (graphs
//! resolve against the registry plus the graphs of the same load); the stub registry is
//! `graph_support::registry`. Positions follow the M2 conventions (see `graph_resolve.rs`).

#![forbid(unsafe_code)]

mod common;
mod graph_support;

use cgf_core::ParamValue;
use cgf_templates::{SourceFile, TemplateSet};
use common::{Row, errors, lines, ok, pos_after, pos_of, pos_of_nth, row, rows, src};
use graph_support::{registry, with_interface};
use serde_json::json;

const GRAPH: &str = "behaviors/child.behavior.toml";
const TEMPLATE: &str = "templates/tank.toml";

const CHILD_PARAMS: &str = "params = { count = { type = \"integer\", minimum = 1 }, label = { type = \"string\", default = \"x\" } }";

fn child() -> String {
    with_interface("test.graph.child", CHILD_PARAMS)
}

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

/// A template with the stub motion model and the given head lines (top-level keys), then the given
/// body lines (`[behavior]`, `[[sensors]]`, ...).
fn template(id: &str, head: &[&str], body: &[&str]) -> String {
    let mut all = vec!["schema_version = 1".to_string(), format!("id = \"{id}\"")];
    all.extend(head.iter().map(|h| (*h).to_string()));
    all.extend([
        "[motion]".to_string(),
        "model = \"stub.motion.alpha\"".to_string(),
        "params = { a = 1.0 }".to_string(),
    ]);
    all.extend(body.iter().map(|b| (*b).to_string()));
    let refs: Vec<&str> = all.iter().map(String::as_str).collect();
    lines(&refs)
}

/// A tank template with `[behavior] ref = <graph>` and the given `params` line (may be empty).
fn tank(graph: &str, params: &str) -> String {
    let mut body = vec!["[behavior]".to_string(), format!("ref = \"{graph}\"")];
    if !params.is_empty() {
        body.push(format!("params = {params}"));
    }
    let refs: Vec<&str> = body.iter().map(String::as_str).collect();
    template("tank.blue", &[], &refs)
}

/// Where the `params` value of the `[behavior]` section starts: the second `params = ` (the first
/// is the motion model's).
fn behavior_params_at(text: &str) -> (u32, u32) {
    let (line, column) = pos_of_nth(text, "params = ", 1);
    (line, column + 9)
}

fn load(files: &[SourceFile]) -> Result<TemplateSet, cgf_templates::Diagnostics> {
    let registry = registry();
    common::loader(&registry).templates_from_sources(files)
}

fn template_rows(graph: &str, tank_text: &str) -> Vec<Row> {
    rows(&errors(load(&[
        src(GRAPH, graph),
        src(TEMPLATE, tank_text),
    ])))
}

/// REQ-B-6: a template `[behavior] ref` can name a graph of the same load; the resolved template
/// carries the behavior with its parameters.
#[test]
fn template_ref_to_file_graph_of_the_same_load_resolves() {
    let tank = tank("test.graph.patrol", "{ engage_range_m = 2000.0 }");

    let set = ok(load(&[src(GRAPH, &patrol()), src(TEMPLATE, &tank)]));

    let resolved = set.iter().next().unwrap_or_else(|| panic!("one template"));
    let behavior = resolved
        .behavior
        .as_ref()
        .unwrap_or_else(|| panic!("a behavior"));
    assert_eq!(behavior.id.as_str(), "test.graph.patrol");
    assert_eq!(behavior.params, json!({ "engage_range_m": 2000.0 }));
}

/// REQ-T-1: an unknown graph is `unknown_id` at the ref, listing the registered behaviors.
#[test]
fn template_ref_to_unknown_graph_is_unknown_id() {
    let tank = tank("test.graph.nowhere", "");

    let d = errors(load(&[src(TEMPLATE, &tank)]));

    assert_eq!(
        rows(&d),
        [row(
            "unknown_id",
            TEMPLATE,
            pos_of(&tank, "\"test.graph.nowhere\"")
        )]
    );
    assert!(
        d[0].notes
            .iter()
            .any(|n| n.message.contains("stub.behavior.ext")),
        "{:?}",
        d[0].notes
    );
}

/// ASM-056: a template that names a graph from a broken file gets no cascade error of its own.
#[test]
fn template_ref_to_broken_graph_gives_no_cascade() {
    let broken = format!("{}this is not toml\n", patrol());
    let tank = tank("test.graph.patrol", "");

    let d = errors(load(&[src(GRAPH, &broken), src(TEMPLATE, &tank)]));

    assert_eq!(
        d.iter().map(|d| d.code.as_str()).collect::<Vec<_>>(),
        ["syntax"]
    );
}

/// ASM-063: a ref to a component of another kind says so.
#[test]
fn template_ref_to_a_condition_says_it_is_a_condition() {
    let tank = tank("stub.cond.flag", "");

    let d = errors(load(&[src(TEMPLATE, &tank)]));

    assert_eq!(
        rows(&d),
        [row(
            "unknown_id",
            TEMPLATE,
            pos_of(&tank, "\"stub.cond.flag\"")
        )]
    );
    assert_eq!(
        d[0].message,
        "`stub.cond.flag` is a condition, not a behavior"
    );
}

/// REQ-B-10 (AT-6): a required param missing in the template's `params` is `missing_param` at the
/// table.
#[test]
fn call_site_missing_param() {
    let tank = tank("test.graph.child", "{ }");

    let d = errors(load(&[src(GRAPH, &child()), src(TEMPLATE, &tank)]));

    assert_eq!(
        rows(&d),
        [row("missing_param", TEMPLATE, behavior_params_at(&tank))]
    );
    assert_eq!(
        d[0].message,
        "behavior `test.graph.child`: missing required parameter `count`"
    );
}

/// REQ-B-10 (AT-6): an undeclared param is `unknown_param` at its key.
#[test]
fn call_site_unknown_param() {
    let tank = tank("test.graph.child", "{ count = 1, extra = 2 }");

    assert_eq!(
        template_rows(&child(), &tank),
        [row("unknown_param", TEMPLATE, pos_of(&tank, "extra"))]
    );
}

/// REQ-B-10 (AT-6): a value of the wrong type is `param_wrong_type` at the value.
#[test]
fn call_site_wrong_type() {
    let tank = tank("test.graph.child", "{ count = \"many\" }");

    assert_eq!(
        template_rows(&child(), &tank),
        [row("param_wrong_type", TEMPLATE, pos_of(&tank, "\"many\""))]
    );
}

/// REQ-B-10 (AT-6): a value out of range is `param_out_of_range` at the value.
#[test]
fn call_site_out_of_range() {
    let tank = tank("test.graph.child", "{ count = 0 }");

    assert_eq!(
        template_rows(&child(), &tank),
        [row(
            "param_out_of_range",
            TEMPLATE,
            pos_after(&tank, "count = ")
        )]
    );
}

/// ADR 0014 (AT-20): a top-level behavior has no parent blackboard, so `{ key = .. }` in template
/// `[behavior] params` is `invalid_binding` at the form.
#[test]
fn key_form_in_template_behavior_params_is_invalid_binding() {
    let graph = with_interface(
        "test.graph.child",
        "params = { n = { type = \"integer\", default = 1 } }",
    );
    let tank = tank("test.graph.child", "{ n = { key = \"k\" } }");

    assert_eq!(
        template_rows(&graph, &tank),
        [row(
            "invalid_binding",
            TEMPLATE,
            pos_of(&tank, "{ key = \"k\" }")
        )]
    );
}

/// ADR 0014: a graph whose interface has a required binding-only param (`entity`) cannot be a
/// top-level behavior: `missing_param` at the template's `params` table.
#[test]
fn graph_with_required_binding_only_param_as_template_behavior_is_missing_param() {
    let graph = with_interface(
        "test.graph.child",
        "params = { target = { type = \"entity\" } }",
    );
    let tank = tank("test.graph.child", "{ }");

    let d = errors(load(&[src(GRAPH, &graph), src(TEMPLATE, &tank)]));

    assert_eq!(
        rows(&d),
        [row("missing_param", TEMPLATE, behavior_params_at(&tank))]
    );
    assert!(d[0].message.contains("target"), "{}", d[0].message);
}

/// ADR 0010, ASM-083: `[behavior]` merges key-wise along `extends` (a child that only writes
/// `params` keeps the parent's `ref` and overrides keys; naming another `ref` replaces the whole
/// section); `[[sensors]]` is replaced as a whole (an empty list clears it); both are now visible
/// in the resolved template.
#[test]
fn behavior_and_sensors_merge_per_adr_0010_and_are_visible_in_resolved_template() {
    let other = lines(&[
        "schema_version = 1",
        "id = \"test.graph.other\"",
        "version = \"1.0.0\"",
        "root = \"r\"",
        "[nodes.r]",
        "type = \"action\"",
        "ref = \"stub.action.noop\"",
    ]);
    let parent = template(
        "tank.parent",
        &[],
        &[
            "[behavior]",
            "ref = \"test.graph.patrol\"",
            "params = { engage_range_m = 1500.0 }",
            "[[sensors]]",
            "kind = \"stub.sensor.eye\"",
            "params = { range_m = 1000.0 }",
        ],
    );
    let override_params = template(
        "tank.params",
        &["extends = \"tank.parent\""],
        &["[behavior]", "params = { engage_range_m = 1800.0 }"],
    );
    let other_ref = template(
        "tank.other",
        &["extends = \"tank.parent\""],
        &["[behavior]", "ref = \"test.graph.other\""],
    );
    let own_sensors = template(
        "tank.sensors",
        &["extends = \"tank.parent\""],
        &[
            "[[sensors]]",
            "kind = \"stub.sensor.eye\"",
            "params = { range_m = 3000.0, fov_deg = 120.0 }",
        ],
    );
    let no_sensors = template(
        "tank.blind",
        &["extends = \"tank.parent\"", "sensors = []"],
        &[],
    );

    let set = ok(load(&[
        src("behaviors/patrol.behavior.toml", &patrol()),
        src("behaviors/other.behavior.toml", &other),
        src("templates/parent.toml", &parent),
        src("templates/params.toml", &override_params),
        src("templates/other.toml", &other_ref),
        src("templates/sensors.toml", &own_sensors),
        src("templates/blind.toml", &no_sensors),
    ]));

    let get = |id: &str| {
        set.get(&common::tid(id))
            .unwrap_or_else(|| panic!("template {id}"))
            .clone()
    };
    let behavior = |id: &str| {
        get(id)
            .behavior
            .map(|b| (b.id.as_str().to_string(), b.params))
    };
    assert_eq!(
        behavior("tank.parent"),
        Some((
            "test.graph.patrol".into(),
            json!({ "engage_range_m": 1500.0 })
        ))
    );
    assert_eq!(
        behavior("tank.params"),
        Some((
            "test.graph.patrol".into(),
            json!({ "engage_range_m": 1800.0 })
        ))
    );
    assert_eq!(
        behavior("tank.other"),
        Some(("test.graph.other".into(), json!({})))
    );
    let sensor_params =
        |id: &str| -> Vec<ParamValue> { get(id).sensors.into_iter().map(|s| s.params).collect() };
    assert_eq!(sensor_params("tank.parent"), [json!({ "range_m": 1000.0 })]);
    assert_eq!(
        sensor_params("tank.params"),
        [json!({ "range_m": 1000.0 })],
        "inherited"
    );
    assert_eq!(
        sensor_params("tank.sensors"),
        [json!({ "range_m": 3000.0, "fov_deg": 120.0 })]
    );
    assert_eq!(sensor_params("tank.blind"), Vec::<ParamValue>::new());
}

/// REQ-T-1: a sensor kind that is not registered is `unknown_id` at the `kind` value; a registered
/// one is checked against its schema (`param_out_of_range` at the value).
#[test]
fn sensors_are_looked_up_and_validated_by_id() {
    let unknown = template(
        "tank.a",
        &[],
        &["[[sensors]]", "kind = \"stub.sensor.nowhere\""],
    );
    let range = template(
        "tank.b",
        &[],
        &[
            "[[sensors]]",
            "kind = \"stub.sensor.eye\"",
            "params = { range_m = -1.0 }",
        ],
    );

    let u = errors(load(&[src(TEMPLATE, &unknown)]));
    let r = errors(load(&[src(TEMPLATE, &range)]));

    assert_eq!(
        rows(&u),
        [row(
            "unknown_id",
            TEMPLATE,
            pos_of(&unknown, "\"stub.sensor.nowhere\"")
        )]
    );
    assert_eq!(
        rows(&r),
        [row(
            "param_out_of_range",
            TEMPLATE,
            pos_after(&range, "range_m = ")
        )]
    );
}

/// ASM-055 / WI-14: the trial build ("dry run") is real for every behavior-family kind: a factory
/// that rejects a combination the schema allows is `param_invalid`, located by the factory's JSON
/// pointer: behavior (template `[behavior]`), sensor (template `[[sensors]]`), condition and
/// action (graph leaves). In M2 the behavior and sensor arms were placeholders and condition and
/// action were refused as unsupported.
#[test]
fn trial_build_runs_real_dry_runs_for_behavior_sensor_condition_and_action() {
    let behavior = tank("stub.behavior.plain", "{ speed = 13.0 }");
    let sensor = template(
        "tank.s",
        &[],
        &[
            "[[sensors]]",
            "kind = \"stub.sensor.eye\"",
            "params = { range_m = 5.0, fov_deg = 13.0 }",
        ],
    );
    let leaves = lines(&[
        "schema_version = 1",
        "id = \"test.graph.leaves\"",
        "version = \"1.0.0\"",
        "root = \"s\"",
        "[nodes.s]",
        "type = \"sequence\"",
        "children = [\"c\", \"a\"]",
        "[nodes.c]",
        "type = \"condition\"",
        "ref = \"stub.cond.near\"",
        "params = { range_m = 13.0 }",
        "[nodes.a]",
        "type = \"action\"",
        "ref = \"stub.action.wait\"",
        "params = { duration_s = 13.0 }",
    ]);

    let b = errors(load(&[src(TEMPLATE, &behavior)]));
    let s = errors(load(&[src(TEMPLATE, &sensor)]));
    let l = errors(load(&[src(GRAPH, &leaves)]));

    assert_eq!(
        rows(&b),
        [row(
            "param_invalid",
            TEMPLATE,
            pos_after(&behavior, "speed = ")
        )]
    );
    assert_eq!(
        rows(&s),
        [row(
            "param_invalid",
            TEMPLATE,
            pos_after(&sensor, "fov_deg = ")
        )]
    );
    assert_eq!(
        rows(&l),
        [
            row("param_invalid", GRAPH, pos_after(&leaves, "range_m = ")),
            row("param_invalid", GRAPH, pos_after(&leaves, "duration_s = ")),
        ]
    );
}

/// ADR 0014 applies to registered behaviors too: one with a required binding-only param
/// (`stub.behavior.ext` declares `target: entity`) is `missing_param` at the template's `params`
/// table, besides the dry run of the values it was given.
#[test]
fn registered_behavior_with_required_binding_only_param_is_missing_param() {
    let tank = tank("stub.behavior.ext", "{ speed = 13.0 }");

    let d = errors(load(&[src(TEMPLATE, &tank)]));

    assert_eq!(
        rows(&d),
        [
            row("missing_param", TEMPLATE, behavior_params_at(&tank)),
            row("param_invalid", TEMPLATE, pos_after(&tank, "speed = ")),
        ]
    );
    assert!(d[0].message.contains("target"), "{}", d[0].message);
}

/// ADR 0014: `{ key }` and `{ param }` forms in the params of a registered behavior are
/// `invalid_binding` at the form, not `param_wrong_type`; the required binding-only param given as
/// such a form is not also missing.
#[test]
fn binding_forms_for_registered_behaviors_are_invalid_binding() {
    let key = tank("stub.behavior.ext", "{ target = { key = \"k\" } }");
    let param = tank("stub.behavior.plain", "{ speed = { param = \"p\" } }");
    let nested = tank("stub.behavior.plain", "{ speed = [ { key = \"k\" } ] }");

    for (text, form) in [
        (&key, "{ key = \"k\" }"),
        (&param, "{ param = \"p\" }"),
        (&nested, "{ key = \"k\" }"),
    ] {
        let d = errors(load(&[src(TEMPLATE, text)]));

        assert_eq!(
            rows(&d),
            [row("invalid_binding", TEMPLATE, pos_of(text, form))],
            "{text}"
        );
    }
}
