//! AT-11: how plugin behaviors take part in template and graph validation, and how their build errors
//! surface (REQ-T-3, REQ-B-10, REQ-B-12; ASM-099 e, ASM-106 d, ASM-107, ASM-114), with the fixture
//! plugin `umbra-test-plugin` (capabilities `test.behavior.ok`, `.needs_target`, `.cross_param`
//! and, with the feature `bad_schema`, `.bad_schema`; see `plugin_support/mod.rs`).
//!
//! Step 4 of M4: written before WI-12; ignored with WI-12 (the fixture exists since WI-10).

#![forbid(unsafe_code)]

mod behavior_support;
mod common;
mod plugin_support;

use core::time::Duration;
use std::path::PathBuf;
use std::sync::LazyLock;

use behavior_support::standard;
use common::options;
use plugin_support::{Tpl, call_graph, write_scenario};
use umbra_engine::scenario::ScenarioError;
use umbra_engine::{
    EngineError, instantiate_with_plugins, load_scenario_with_plugins, validate_path_with_plugins,
};
use umbra_templates::LoaderError;

static FIXTURE: LazyLock<PathBuf> = LazyLock::new(|| plugin_support::fixture(&[]));

/// A chain of `n` file graphs `test.depth.d00 -> .. -> test.behavior.ok` under one entity.
fn chain(name: &str, n: usize) -> PathBuf {
    let graphs: Vec<(String, String)> = (0..n)
        .map(|i| {
            let target = if i + 1 == n {
                "test.behavior.ok".to_owned()
            } else {
                format!("test.depth.d{:02}", i + 1)
            };
            (
                format!("d{i:02}"),
                call_graph(&format!("test.depth.d{i:02}"), &target),
            )
        })
        .collect();
    let graphs: Vec<(&str, String)> = graphs
        .iter()
        .map(|(s, t)| (s.as_str(), t.clone()))
        .collect();
    write_scenario(
        name,
        1.0,
        "",
        &[Tpl {
            id: "test.entity.chain",
            behavior: "test.depth.d00",
            params: "",
        }],
        &graphs,
        &[("test.entity.chain", "walker")],
    )
}

fn invalid(result: Result<umbra_engine::LoadedScenario, ScenarioError>) -> Vec<String> {
    match result {
        Err(ScenarioError::Invalid(d)) => d.iter().map(|d| d.code.as_str().to_owned()).collect(),
        Err(other) => panic!("expected ScenarioError::Invalid, got {other}"),
        Ok(_) => panic!("expected the load to fail"),
    }
}

/// ASM-099 e, ASM-073: a plugin behavior at the end of a `subgraph` chain counts as one level, at
/// load (validate) and at instantiate: 15 file graphs + the plugin are 16, the limit, and are accepted.
#[test]
#[ignore = "WI-12"]
fn plugin_behavior_counts_one_level_at_the_limit() {
    let scenario = chain("plugin-depth-ok", 15);

    let loaded =
        load_scenario_with_plugins(&standard(), &scenario, std::slice::from_ref(&*FIXTURE))
            .unwrap_or_else(|e| panic!("load: {e}"));

    let run = instantiate_with_plugins(&loaded, options(Duration::from_micros(500), None, None))
        .unwrap_or_else(|e| panic!("instantiate: {e}"));
    assert_eq!(run.entities.len(), 1);
}

/// ASM-099 e: 16 file graphs + the plugin are 17: `subgraph_too_deep` at load, from `validate` and
/// from the scenario load alike.
#[test]
#[ignore = "WI-12"]
fn plugin_behavior_one_level_more_is_subgraph_too_deep() {
    let scenario = chain("plugin-depth-over", 16);

    let from_load = invalid(load_scenario_with_plugins(
        &standard(),
        &scenario,
        std::slice::from_ref(&*FIXTURE),
    ));
    let report = validate_path_with_plugins(
        &standard(),
        scenario.parent().expect("directory"),
        std::slice::from_ref(&*FIXTURE),
    )
    .expect("validate runs");

    assert_eq!(from_load, ["subgraph_too_deep"]);
    let from_validate: Vec<&str> = report.diagnostics.iter().map(|d| d.code.as_str()).collect();
    assert_eq!(from_validate, ["subgraph_too_deep"]);
}

fn single_template(name: &str, behavior: &str, params: &str) -> PathBuf {
    write_scenario(
        name,
        1.0,
        "",
        &[Tpl {
            id: "test.entity.one",
            behavior,
            params,
        }],
        &[],
        &[("test.entity.one", "one")],
    )
}

/// REQ-B-10, ASM-107 a: a param the plugin declares as `entity` in its metadata is binding-only, so a
/// template that does not supply it gets `missing_param`, as for a graph.
#[test]
#[ignore = "WI-12"]
fn binding_only_plugin_param_is_missing_param_in_template() {
    let scenario = single_template("plugin-binding-only", "test.behavior.needs_target", "");

    let codes = invalid(load_scenario_with_plugins(
        &standard(),
        &scenario,
        std::slice::from_ref(&*FIXTURE),
    ));

    assert_eq!(codes, ["missing_param"]);
}

/// ADR 0014, ASM-107 a: `{ key }` and `{ param }` forms are not allowed in the `[behavior] params`
/// of a template, for a plugin behavior as for any registered one.
#[test]
#[ignore = "WI-12"]
fn key_and_param_forms_in_template_params_are_invalid_binding_for_plugin() {
    for (form, params) in [
        ("key", "{ target = { key = \"t\" } }"),
        ("param", "{ target = { param = \"t\" } }"),
    ] {
        let scenario = single_template(
            &format!("plugin-form-{form}"),
            "test.behavior.needs_target",
            params,
        );

        let codes = invalid(load_scenario_with_plugins(
            &standard(),
            &scenario,
            std::slice::from_ref(&*FIXTURE),
        ));

        assert!(
            codes.iter().any(|c| c == "invalid_binding"),
            "{form} form: {codes:?}"
        );
    }
}

/// ASM-106 d, ASM-114: a rule that combines two values passed through a graph (`lo <= hi`) is checked
/// only by the plugin's `configure`, at spawn time, and arrives as `EngineError::BehaviorBuild` for
/// that entity.
#[test]
#[ignore = "WI-12"]
fn plugin_configure_error_at_spawn_is_behavior_build() {
    let graph = "schema_version = 1\nid = \"test.graph.range\"\nversion = \"1.0.0\"\nroot = \"call\"\n\n\
                 [interface]\nparams = { lo = { type = \"number\", default = 0.0 }, hi = { type = \"number\", default = 10.0 } }\n\n\
                 [nodes.call]\ntype = \"subgraph\"\nref = \"test.behavior.cross_param\"\n\
                 params = { lo = { param = \"lo\" }, hi = { param = \"hi\" } }\n"
        .to_owned();
    let scenario = write_scenario(
        "plugin-cross-param",
        1.0,
        "",
        &[Tpl {
            id: "test.entity.range",
            behavior: "test.graph.range",
            params: "{ lo = 5.0, hi = 1.0 }",
        }],
        &[("range", graph)],
        &[("test.entity.range", "ranged")],
    );
    let loaded =
        load_scenario_with_plugins(&standard(), &scenario, std::slice::from_ref(&*FIXTURE))
            .unwrap_or_else(|e| panic!("each value is valid on its own, so the load passes: {e}"));

    let Err(err) =
        instantiate_with_plugins(&loaded, options(Duration::from_micros(500), None, None))
    else {
        panic!("instantiate must fail");
    };

    assert!(
        matches!(err, EngineError::BehaviorBuild { entity: 0, .. }),
        "{err:?}"
    );
}

/// ASM-114: a plugin whose param schema is outside the ADR 0009 subset makes every load with that
/// plugin fail with the existing `LoaderError::Schema`, naming the capability.
#[test]
#[ignore = "WI-12"]
fn plugin_schema_outside_subset_is_loader_schema_error() {
    let dir = plugin_support::fixture(&["bad_schema"]);
    let scenario = single_template("plugin-bad-schema", "test.behavior.ok", "");

    let result = load_scenario_with_plugins(&standard(), &scenario, &[dir]);

    match result {
        Err(ScenarioError::Loader(LoaderError::Schema { id, .. })) => {
            assert_eq!(id.as_str(), "test.behavior.bad_schema");
        }
        Err(other) => panic!("expected LoaderError::Schema, got {other}"),
        Ok(_) => panic!("expected the load to fail"),
    }
}
