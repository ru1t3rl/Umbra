//! AT-8 (M4 acceptance 1) and the engine row of AT-5: the example plugin `example-wander` through
//! the engine, as a real library. Requirements: REQ-B-9, REQ-B-10, REQ-B-13, REQ-T-1, REQ-X-2,
//! ADR 0013 amendment, ASM-096, ASM-113.
//!
//! Step 4 of M4: written before WI-12/WI-13. Contract with the example scenario
//! `examples/scenarios/plugin_wander` (WI-13), from the plan and the choices recorded here:
//! - entities `wander-direct` (`[behavior] ref = "example.behavior.wander"`, default `speed_mps`
//!   5 and `turn_every_s` 10, so a new heading every `ceil(10 * hz)` ticks), `wander-graph` (a file
//!   graph that calls the plugin as a `subgraph` with `speed_mps = { key = "speed" }`, the
//!   blackboard `speed` being **12.0** m/s, and `turns` bound to a blackboard key inside a
//!   `timeout`) and `patrol-builtin` (on `builtin.fsm.patrol_and_engage`, in a template of its own
//!   that names it in a single `ref`);
//! - no `plugin_paths` in the scenario (ASM-113): the paths are passed through the library API.
//!
//! Ignored with WI-13 (the example) or WI-12 (the fixture plugin and the engine wiring).

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod behavior_support;
mod common;
mod plugin_support;

use core::time::Duration;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};

use behavior_support::{assert_same_behavior_trace, entity, standard};
use common::{options, run_to};
use umbra_core::{
    BbType, BbValue, Behavior, BehaviorContext, BehaviorId, BehaviorInstance, BlackboardError,
    BuildError, Catalog, CatalogKind, ComponentId, ComponentInfo, EntityView, Factory,
    InstantiateError, Intent, IntentBuffer, MetersPerSecond, ParamSchema, ParamValue, Provenance,
    Registry, Status, TickRate, Version,
};
use umbra_engine::scenario::{instantiate, load_scenario, validate_path};
use umbra_engine::{
    instantiate_with_plugins, load_scenario_with_plugins, validate_path_with_plugins,
};

static WANDER: LazyLock<PathBuf> = LazyLock::new(plugin_support::wander);

fn example_dir() -> PathBuf {
    plugin_support::wander_example_dir()
}

fn load(dir: &Path) -> umbra_engine::LoadedScenario {
    load_scenario_with_plugins(
        &standard(),
        &dir.join("scenario.toml"),
        std::slice::from_ref(&*WANDER),
    )
    .unwrap_or_else(|e| panic!("load {}: {e}", dir.display()))
}

/// Instantiates once (a host attaches to one engine only), runs `ticks`, and returns the trace with the
/// entities' ids by name.
fn run_example(dir: &Path, ticks: u64) -> (common::Trace, Vec<umbra_engine::scenario::EntityInfo>) {
    let loaded = load(dir);
    let run = instantiate_with_plugins(&loaded, options(Duration::from_micros(500), None, None))
        .expect("instantiate");
    let entities = run.entities.clone();
    let trace = run_to(run.engine, ticks, false);
    (trace, entities)
}

fn id_named(entities: &[umbra_engine::scenario::EntityInfo], name: &str) -> umbra_core::EntityId {
    entities
        .iter()
        .find(|e| e.name.as_deref() == Some(name))
        .unwrap_or_else(|| panic!("no entity named {name}"))
        .id
}

/// The `SetHeading` intent texts of `id` per tick (empty text when none).
fn headings(trace: &common::Trace, id: umbra_core::EntityId) -> Vec<String> {
    trace
        .snapshots
        .iter()
        .map(|s| {
            format!(
                "{:?}",
                entity(s, id)
                    .intents
                    .iter()
                    .filter(|i| matches!(i, Intent::SetHeading(_)))
                    .collect::<Vec<_>>()
            )
        })
        .collect()
}

/// M4 acceptance 1, REQ-T-6: the example directory validates when the plugin directory is given.
#[test]
fn example_validates_with_plugin_path() {
    let report =
        validate_path_with_plugins(&standard(), &example_dir(), std::slice::from_ref(&*WANDER))
            .expect("validate runs");

    assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
    assert_ne!(report.files_checked.len(), 0);
}

/// REQ-T-3: without the plugin directory the same files fail with `unknown_id` naming the plugin
/// behavior, not with a load error: the registry is the only source of components.
#[test]
fn example_fails_validation_without_plugin_path_with_unknown_id() {
    let report = validate_path(&standard(), &example_dir()).expect("validate runs");

    assert!(
        report.diagnostics.iter().any(|d| {
            d.code.as_str() == "unknown_id" && format!("{d:?}").contains("example.behavior.wander")
        }),
        "{:?}",
        report.diagnostics
    );
}

/// The plugin's own logic is visible in the run: the heading changes exactly every
/// `ceil(turn_every_s * hz)` ticks (default 10 s), at least three times in 1000 ticks.
#[test]
fn wander_entity_changes_heading_every_turn_period() {
    let loaded = load(&example_dir());
    let period = usize::try_from(10 * loaded.scenario.tick_rate.hz()).expect("fits");
    let (trace, ents) = run_example(&example_dir(), 1000);
    let seen = headings(&trace, id_named(&ents, "wander-direct"));

    let change_ticks: Vec<usize> = (0..seen.len())
        .filter(|&i| i == 0 || seen[i] != seen[i - 1])
        .collect();

    assert!(change_ticks.len() >= 3, "{change_ticks:?}");
    assert!(
        change_ticks.windows(2).all(|w| w[1] - w[0] == period),
        "changes every {period} ticks: {change_ticks:?}"
    );
}

/// REQ-K-4, REQ-B-1: the plugin's intents are the entity's intents in the snapshot (speed and
/// heading), like any built-in behavior's.
#[test]
fn plugin_intents_visible_in_snapshot() {
    let (trace, ents) = run_example(&example_dir(), 3);

    let intents = &entity(&trace.snapshots[2], id_named(&ents, "wander-direct")).intents;

    assert!(
        intents.contains(&Intent::SetSpeed(MetersPerSecond(5.0))),
        "{intents:?}"
    );
    assert!(
        intents.iter().any(|i| matches!(i, Intent::SetHeading(_))),
        "{intents:?}"
    );
}

/// REQ-B-13, REQ-B-17: the catalog lists the plugin behavior as `Plugin` and read-only, with the
/// plugin's version, the schema's description, and the interface from the metadata.
#[test]
fn catalog_lists_wander_as_plugin_read_only_with_version_and_schema() {
    let loaded = load(&example_dir());

    let catalog = Catalog::from_registry(&loaded.registry);

    let e = catalog
        .get(CatalogKind::Behavior, "example.behavior.wander")
        .expect("the plugin behavior is in the catalog");
    assert_eq!(e.source, Provenance::Plugin);
    assert!(e.read_only());
    assert_eq!(e.version, Version::new(0, 1, 0), "the plugin's version");
    assert!(!e.description.is_empty(), "the schema's description");
    assert_eq!(e.param_types.get("speed_mps"), Some(&BbType::Number));
    assert_eq!(e.outputs.get("turns"), Some(&BbType::Integer));
    let schema = format!("{:?}", e.params);
    assert!(
        schema.contains("speed_mps") && schema.contains("turn_every_s"),
        "{schema}"
    );
}

/// REQ-B-9, REQ-B-10, ADR 0014: a file graph calls the plugin as a `subgraph` and binds its
/// `speed_mps` from the blackboard (12 m/s, not the plugin's 5 m/s default), while the plugin keeps
/// emitting its heading.
#[test]
fn graph_calls_plugin_as_subgraph_with_bound_input_and_output() {
    let (trace, ents) = run_example(&example_dir(), 10);

    let intents = &entity(&trace.snapshots[9], id_named(&ents, "wander-graph")).intents;

    assert!(
        intents.contains(&Intent::SetSpeed(MetersPerSecond(12.0))),
        "the bound input reached the plugin: {intents:?}"
    );
    assert!(
        intents.iter().any(|i| matches!(i, Intent::SetHeading(_))),
        "{intents:?}"
    );
}

/// M4 acceptance 1, REQ-T-1: moving an entity from the built-in behavior to the plugin behavior is
/// one `ref` in one template; no scenario change and no code change.
#[test]
fn swapping_builtin_for_plugin_behavior_is_a_template_change() {
    let copy = plugin_support::scratch_copy(&example_dir(), "swap");
    let mut changed = Vec::new();
    for entry in std::fs::read_dir(copy.join("templates")).expect("templates") {
        let path = entry.expect("entry").path();
        let text = std::fs::read_to_string(&path).expect("template");
        if text.contains("ref = \"builtin.fsm.patrol_and_engage\"") {
            std::fs::write(
                &path,
                text.replace(
                    "ref = \"builtin.fsm.patrol_and_engage\"",
                    "ref = \"example.behavior.wander\"",
                ),
            )
            .expect("write");
            changed.push(path);
        }
    }
    assert_eq!(
        changed.len(),
        1,
        "one template names the built-in: {changed:?}"
    );
    assert_eq!(
        std::fs::read(copy.join("scenario.toml")).expect("copy"),
        std::fs::read(example_dir().join("scenario.toml")).expect("original"),
        "the scenario file is untouched"
    );

    let (before, ents) = run_example(&example_dir(), 400);
    let (after, _) = run_example(&copy, 400);

    let id = id_named(&ents, "patrol-builtin");
    assert!(
        headings(&after, id).iter().any(|h| h != "[]"),
        "after the swap the entity runs the plugin and emits headings"
    );
    assert_ne!(
        after.hashes.last(),
        before.hashes.last(),
        "and the run differs"
    );
}

// ----------------------------------------------------------------------------- AT-5 (engine row)

/// The Rust twin of the fixture's `test.behavior.draw_speed`: every tick one draw from the entity
/// stream, `SetSpeed(draw % 100)`, Running forever.
struct Draws;

impl BehaviorInstance for Draws {
    fn tick(
        &mut self,
        ctx: &mut BehaviorContext<'_>,
        _: &EntityView<'_>,
        out: &mut IntentBuffer<'_>,
    ) -> Status {
        let draw = ctx.rng.next_u64() % 100;
        #[expect(clippy::cast_precision_loss, reason = "draw is below 100")]
        out.push(Intent::SetSpeed(MetersPerSecond(draw as f64)));
        Status::Running
    }
    fn halt(&mut self) {}
    fn output(&self, _: &str) -> Option<BbValue> {
        None
    }
    fn set_input(&mut self, _: &str, _: Option<BbValue>) -> Result<(), BlackboardError> {
        Ok(())
    }
}

struct StubBehavior(BehaviorId);

impl Behavior for StubBehavior {
    fn id(&self) -> &BehaviorId {
        &self.0
    }
    fn instantiate(&self, _: TickRate) -> Result<Box<dyn BehaviorInstance>, InstantiateError> {
        Ok(Box::new(Draws))
    }
}

struct StubFactory {
    id: ComponentId,
    schema: ParamSchema,
}

impl Factory<dyn Behavior> for StubFactory {
    fn id(&self) -> &ComponentId {
        &self.id
    }
    fn param_schema(&self) -> &ParamSchema {
        &self.schema
    }
    fn build(&self, _: &ParamValue) -> Result<Arc<dyn Behavior>, BuildError> {
        let id = BehaviorId::new(self.id.as_str()).map_err(|e| BuildError::new(e.to_string()))?;
        Ok(Arc::new(StubBehavior(id)))
    }
}

fn caller_scenario(name: &str, target: &str) -> PathBuf {
    plugin_support::write_scenario(
        name,
        2.0,
        "",
        &[plugin_support::Tpl {
            id: "test.entity.caller",
            behavior: "test.graph.caller",
            params: "",
        }],
        &[(
            "caller",
            plugin_support::call_graph("test.graph.caller", target),
        )],
        &[("test.entity.caller", "a"), ("test.entity.caller", "b")],
    )
}

/// REQ-B-9, ADR 0013 amendment, ASM-096: through one `subgraph` node a real plugin behavior and the
/// M3 `Provenance::Plugin` stub with the same logic give the same trace and intents on every tick.
#[test]
fn plugin_and_stub_targets_are_indistinguishable_in_a_subgraph() {
    let mut stub_registry: Registry = standard();
    let stub_id = ComponentId::new("test.behavior.draw_speed_stub").expect("id");
    stub_registry
        .register_behavior(
            Arc::new(StubFactory {
                id: stub_id,
                schema: ParamSchema::from_json_str(r#"{"type":"object","examples":[{}]}"#)
                    .expect("schema"),
            }),
            ComponentInfo::new("stub twin", Provenance::Plugin, Version::new(0, 1, 0)),
        )
        .expect("register the stub");
    let stub_path = caller_scenario("twin-stub", "test.behavior.draw_speed_stub");
    let plugin_path = caller_scenario("twin-plugin", "test.behavior.draw_speed");
    let fixture = plugin_support::fixture(&[]);

    let stub = {
        let scenario = load_scenario(&stub_registry, &stub_path).expect("stub scenario");
        let run = instantiate(
            &stub_registry,
            &scenario,
            options(Duration::from_micros(500), None, None),
        )
        .expect("instantiate the stub run");
        run_to(run.engine, 60, false)
    };
    let plugin = {
        let loaded = load_scenario_with_plugins(&standard(), &plugin_path, &[fixture])
            .expect("plugin scenario");
        let run =
            instantiate_with_plugins(&loaded, options(Duration::from_micros(500), None, None))
                .expect("instantiate the plugin run");
        run_to(run.engine, 60, false)
    };

    assert_same_behavior_trace("plugin vs stub", &plugin, &stub);
    assert!(
        plugin
            .snapshots
            .iter()
            .any(|s| s.entities.iter().any(|e| !e.intents.is_empty())),
        "the behaviors really emitted intents"
    );
}
