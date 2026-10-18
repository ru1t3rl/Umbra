//! AT-9 (provenance and `read_only`), AT-13 (the catalog data model) and AT-10
//! (`graph_version_reaches_catalog`): `Catalog::from_registry` over the standard registry plus the
//! graphs of a scenario.
//! Requirements: REQ-B-13, REQ-B-14, REQ-B-17, REQ-T-3, ADR 0009, ADR 0011, ADR 0014, ASM-077, ASM-084, ASM-091.
//!
//! Step 4 of M3: written before WI-15; run since WI-15, which implemented `standard_registry()` and
//! `scenario_registry()`.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod behavior_support;
mod common;

use std::sync::Arc;

use behavior_support::{load_example, standard};
use cgf_core::{
    BbType, Behavior, BehaviorId, BehaviorInstance, BuildError, Catalog, CatalogKind, ComponentId,
    ComponentInfo, Factory, InstantiateError, ParamSchema, ParamValue, Provenance, Registry,
    TickRate, Version,
};
use cgf_engine::scenario::scenario_registry;
use cgf_templates::value::parse;
use cgf_templates::{CompiledSchema, SourceFile};
use serde_json::Value;

const BUILTIN_CONDITIONS: [&str; 4] = [
    "builtin.cond.chance",
    "builtin.cond.enemy_in_range",
    "builtin.cond.target_in_range",
    "builtin.cond.timer_elapsed",
];
const BUILTIN_ACTIONS: [&str; 4] = [
    "builtin.action.fire",
    "builtin.action.follow_route",
    "builtin.action.move_to",
    "builtin.action.wait",
];
const BUILTIN_GRAPHS: [&str; 2] = [
    "builtin.fsm.patrol_and_engage",
    "builtin.subgraph.engage_target",
];
const FILE_GRAPHS: [&str; 3] = [
    "example.behavior.follow_route",
    "example.behavior.patrol_and_engage",
    "example.subgraph.attack",
];

/// The catalog of the standard registry and of the registry for the example scenario.
fn standard_catalog() -> Catalog {
    Catalog::from_registry(&standard())
}

fn scenario_catalog() -> Catalog {
    let registry = scenario_registry(&standard(), &load_example()).expect("scenario registry");
    Catalog::from_registry(&registry)
}

fn entry<'a>(catalog: &'a Catalog, kind: CatalogKind, id: &str) -> &'a cgf_core::CatalogEntry {
    catalog
        .get(kind, id)
        .unwrap_or_else(|| panic!("no {kind:?} entry `{id}`"))
}

struct PluginStub {
    id: ComponentId,
    schema: ParamSchema,
}

struct PluginBehavior(BehaviorId);

impl Behavior for PluginBehavior {
    fn id(&self) -> &BehaviorId {
        &self.0
    }
    fn instantiate(&self, _: TickRate) -> Result<Box<dyn BehaviorInstance>, InstantiateError> {
        Err(InstantiateError::Other {
            message: "stub".into(),
        })
    }
}

impl Factory<dyn Behavior> for PluginStub {
    fn id(&self) -> &ComponentId {
        &self.id
    }
    fn param_schema(&self) -> &ParamSchema {
        &self.schema
    }
    fn build(&self, _: &ParamValue) -> Result<Arc<dyn Behavior>, BuildError> {
        let id = BehaviorId::new(self.id.as_str()).map_err(|e| BuildError::new(e.to_string()))?;
        Ok(Arc::new(PluginBehavior(id)))
    }
}

/// REQ-B-13 (AT-9): the built-in conditions, actions and graphs are `Builtin` and read-only.
#[test]
fn builtin_graphs_and_leaves_are_builtin_and_read_only() {
    let catalog = standard_catalog();
    let all = BUILTIN_CONDITIONS
        .iter()
        .map(|id| (CatalogKind::Condition, *id))
        .chain(BUILTIN_ACTIONS.iter().map(|id| (CatalogKind::Action, *id)))
        .chain(BUILTIN_GRAPHS.iter().map(|id| (CatalogKind::Behavior, *id)));

    for (kind, id) in all {
        let e = entry(&catalog, kind, id);

        assert_eq!(e.source, Provenance::Builtin, "{id}");
        assert!(e.read_only(), "{id}");
    }
}

/// REQ-B-13 (AT-9): graphs loaded from the scenario's files are `File` and editable.
#[test]
fn file_graphs_are_file_and_editable() {
    let catalog = scenario_catalog();

    for id in FILE_GRAPHS {
        let e = entry(&catalog, CatalogKind::Behavior, id);

        assert_eq!(e.source, Provenance::File, "{id}");
        assert!(!e.read_only(), "{id}");
    }
}

/// REQ-B-13 (AT-9): a behavior registered by a plugin is `Plugin` and read-only, and the catalog
/// does not tell it from the others otherwise.
#[test]
fn plugin_stub_is_plugin_and_read_only() {
    let mut registry: Registry = standard();
    let id = ComponentId::new("acme.behavior.smart_tank").expect("id");
    registry
        .register_behavior(
            Arc::new(PluginStub {
                id: id.clone(),
                schema: ParamSchema::from_json_str(r#"{"type":"object","examples":[{}]}"#)
                    .expect("schema"),
            }),
            ComponentInfo::new(
                "A plugin behavior",
                Provenance::Plugin,
                Version::new(2, 1, 0),
            ),
        )
        .expect("register");

    let catalog = Catalog::from_registry(&registry);

    let e = entry(&catalog, CatalogKind::Behavior, id.as_str());
    assert_eq!(e.source, Provenance::Plugin);
    assert!(e.read_only());
    assert_eq!(e.version, Version::new(2, 1, 0));
}

/// REQ-B-13 (AT-9): the twelve node types are built in and read-only.
#[test]
fn node_types_are_builtin() {
    let catalog = standard_catalog();

    let nodes: Vec<_> = catalog
        .entries()
        .iter()
        .filter(|e| e.kind == CatalogKind::NodeType)
        .collect();

    assert_eq!(nodes.len(), 12);
    assert!(
        nodes
            .iter()
            .all(|e| e.source == Provenance::Builtin && e.read_only())
    );
}

/// REQ-B-17 (AT-13): the catalog lists node types, conditions, actions and graphs, and nothing of
/// the other kinds (motion models, terrain, sensors are not in it).
#[test]
fn catalog_lists_node_types_conditions_actions_and_graphs() {
    let catalog = scenario_catalog();
    let ids = |kind: CatalogKind| -> Vec<&str> {
        catalog
            .entries()
            .iter()
            .filter(|e| e.kind == kind)
            .map(|e| e.id.as_str())
            .collect()
    };

    assert_eq!(ids(CatalogKind::NodeType).len(), 12);
    assert_eq!(ids(CatalogKind::Condition), BUILTIN_CONDITIONS);
    assert_eq!(ids(CatalogKind::Action), BUILTIN_ACTIONS);
    let mut graphs: Vec<&str> = BUILTIN_GRAPHS
        .iter()
        .chain(FILE_GRAPHS.iter())
        .copied()
        .collect();
    graphs.sort_unstable();
    assert_eq!(ids(CatalogKind::Behavior), graphs);
    assert!(
        catalog
            .get(CatalogKind::Behavior, "builtin.ground.kinematic")
            .is_none()
    );
    assert!(
        catalog
            .get(CatalogKind::Behavior, "builtin.visual")
            .is_none()
    );
}

/// REQ-B-17 (AT-13): every entry has an ID, a description, a source, a version and a schema.
#[test]
fn every_entry_has_id_description_source_version_schema() {
    let catalog = scenario_catalog();

    for e in catalog.entries() {
        assert_ne!(e.id, "");
        assert!(
            !e.description.is_empty(),
            "{}: a catalog description is documentation",
            e.id
        );
        assert!(e.params.as_json().is_object(), "{}", e.id);
        assert!(matches!(
            e.source,
            Provenance::Builtin | Provenance::File | Provenance::Plugin
        ));
        assert!(e.version.pre.is_empty(), "{}: {}", e.id, e.version);
    }
}

/// REQ-B-10, REQ-B-17, ADR 0014 (AT-13): binding-only param types reach the catalog: the attack
/// subgraph's `target` is an `entity` in `param_types` although it is not in the JSON Schema.
#[test]
fn binding_only_param_types_reach_the_catalog() {
    let catalog = scenario_catalog();

    let attack = entry(&catalog, CatalogKind::Behavior, "example.subgraph.attack");

    assert_eq!(attack.param_types.get("target"), Some(&BbType::Entity));
    assert_eq!(
        attack.param_types.get("fire_range_m"),
        Some(&BbType::Number)
    );
    assert!(
        attack.params.as_json()["properties"]
            .get("target")
            .is_none()
    );
    assert!(
        attack.params.as_json()["properties"]
            .get("fire_range_m")
            .is_some()
    );
    assert_eq!(
        attack.result.as_deref(),
        Some(
            "Failure when the target is unset; otherwise Running, also when a set target is lost (the approach restarts each tick)"
        )
    );
}

/// REQ-B-13 (AT-13): `read_only()` is `source != File` for every entry.
#[test]
fn read_only_follows_source() {
    for e in scenario_catalog().entries() {
        assert_eq!(e.read_only(), e.source != Provenance::File, "{}", e.id);
    }
}

/// REQ-B-17 (AT-13): entries are sorted by (kind, id) and unique.
#[test]
fn catalog_is_sorted_by_kind_and_id() {
    let catalog = scenario_catalog();

    let keys: Vec<_> = catalog
        .entries()
        .iter()
        .map(|e| (e.kind, e.id.clone()))
        .collect();

    let mut sorted = keys.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(keys, sorted);
}

/// A JSON value as inline TOML.
fn toml_of(v: &Value) -> String {
    match v {
        Value::Null => panic!("TOML has no null"),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => format!("{s:?}"),
        Value::Array(a) => format!("[{}]", a.iter().map(toml_of).collect::<Vec<_>>().join(", ")),
        Value::Object(o) => format!(
            "{{ {} }}",
            o.iter()
                .map(|(k, v)| format!("{k} = {}", toml_of(v)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

/// ADR 0009, REQ-T-3 (AT-13): every catalog schema compiles in the supported subset and validates
/// its own examples.
#[test]
fn every_catalog_schema_compiles_in_subset_and_examples_validate() {
    for e in scenario_catalog().entries() {
        let compiled = CompiledSchema::compile(&e.params)
            .unwrap_or_else(|err| panic!("{}: schema outside the subset: {err}", e.id));
        let examples = e.params.as_json()["examples"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        assert!(
            !examples.is_empty(),
            "{} documents at least one example",
            e.id
        );
        for example in examples {
            let text = format!("x = {}\n", toml_of(&example));
            let (table, syntax) = parse(&SourceFile::new("example.toml", text.as_str()));
            assert_eq!(syntax.len(), 0, "{}: {syntax:?}", e.id);
            let table = table.expect("table");
            let violations = compiled.validate(table.get("x").expect("x"), "example");
            assert_eq!(
                violations,
                [],
                "{}: the example violates its own schema",
                e.id
            );
        }
    }
}

/// REQ-B-17 (AT-13): the catalog is a pure function of the registry: the same registry gives equal
/// catalogs, and two independently built standard registries do too.
#[test]
fn catalog_is_a_pure_function_of_the_registry() {
    let registry = standard();

    assert_eq!(
        Catalog::from_registry(&registry),
        Catalog::from_registry(&registry)
    );
    assert_eq!(standard_catalog(), standard_catalog());
}

/// REQ-B-14 (AT-10): a graph's `version` reaches the catalog; built-in leaves carry the package
/// version.
#[test]
fn graph_version_reaches_catalog() {
    let catalog = scenario_catalog();

    let attack = entry(&catalog, CatalogKind::Behavior, "example.subgraph.attack");
    let patrol = entry(
        &catalog,
        CatalogKind::Behavior,
        "example.behavior.patrol_and_engage",
    );
    let leaf = entry(&catalog, CatalogKind::Condition, "builtin.cond.chance");

    assert_eq!(attack.version, Version::new(1, 0, 0));
    assert_eq!(patrol.version, Version::new(1, 0, 0));
    assert_eq!(
        leaf.version,
        Version::parse(env!("CARGO_PKG_VERSION")).expect("package version")
    );
}
