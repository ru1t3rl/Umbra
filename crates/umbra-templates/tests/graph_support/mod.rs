//! Shared helpers for the M3 behavior-graph tests of `umbra-templates` (new in M3; the M2 helpers in
//! `tests/common/mod.rs` are reused unchanged).
//!
//! The registry has the M2 stub motion and terrain components plus stub conditions, actions and
//! one stub behavior (`ComponentInfo` with a declared binding-only param), so graph files can be
//! loaded without `umbra-behavior` (ASM-005).
//!
//! | ID | Kind | Parameters (schema) |
//! |---|---|---|
//! | `stub.cond.flag` | condition | `name` string, optional |
//! | `stub.cond.near` | condition | `range_m` number in (0, 100000], required; `store_as` string (writes an entity key) |
//!
//! Every factory with a documented "13" rule (`near.range_m`, `wait.duration_s`, `ext.speed`,
//! `eye.fov_deg`) rejects exactly 13.0 in `build` (not in its schema), so tests can see the trial
//! build of each kind.
//!
//! | `stub.cond.chance` | condition | `probability` number in [0, 1], required |
//! | `stub.cond.has` | condition | `target_key` string, required (reads an entity key) |
//! | `stub.action.noop` | action | none |
//! | `stub.action.go` | action | `target_key` string (reads an entity key); `speed_mps` number in (0, 100] |
//! | `stub.action.wait` | action | `duration_s` number > 0, required |
//! | `stub.sensor.eye` | sensor | `range_m` number > 0, required; `fov_deg` number in (0, 360] |
//! | `stub.behavior.plain` | behavior | `speed` number in (0, 50]; no binding-only params |
//! | `stub.behavior.ext` | behavior | `speed` number in (0, 50]; declares `target: entity` (binding-only), output `found: entity` |
#![expect(
    dead_code,
    reason = "each test binary uses a subset of the shared helpers"
)]

use std::sync::Arc;

use umbra_core::{
    Access, Action, ActionInstance, BbType, Behavior, BehaviorId, BehaviorInstance,
    BlackboardParam, BuildError, ComponentId, ComponentInfo, Condition, ContactBuffer, Factory,
    InstantiateError, Observer, ParamSchema, ParamValue, Provenance, Registry, SenseContext,
    Sensor, Target, TickRate, Version,
};
use umbra_templates::{Diagnostics, GraphSet, Loader, LoaderOptions, SourceFile};

use super::common::{Row, errors, lines, loader, ok, registry as base_registry, rows, src};

/// The default path of a single graph file in tests.
pub const PATH: &str = "behaviors/g.behavior.toml";

const FLAG: &str = r#"{"type":"object","properties":{"name":{"type":"string"}},"additionalProperties":false,"examples":[{}]}"#;
const NEAR: &str = r#"{"type":"object","properties":{"range_m":{"type":"number","exclusiveMinimum":0,"maximum":100000},"store_as":{"type":"string"}},"required":["range_m"],"additionalProperties":false,"examples":[{"range_m":1.0}]}"#;
const CHANCE: &str = r#"{"type":"object","properties":{"probability":{"type":"number","minimum":0,"maximum":1}},"required":["probability"],"additionalProperties":false,"examples":[{"probability":0.5}]}"#;
const HAS: &str = r#"{"type":"object","properties":{"target_key":{"type":"string"}},"required":["target_key"],"additionalProperties":false,"examples":[{"target_key":"t"}]}"#;
const NOOP: &str =
    r#"{"type":"object","properties":{},"additionalProperties":false,"examples":[{}]}"#;
const GO: &str = r#"{"type":"object","properties":{"target_key":{"type":"string"},"speed_mps":{"type":"number","exclusiveMinimum":0,"maximum":100}},"additionalProperties":false,"examples":[{}]}"#;
const WAIT: &str = r#"{"type":"object","properties":{"duration_s":{"type":"number","exclusiveMinimum":0}},"required":["duration_s"],"additionalProperties":false,"examples":[{"duration_s":1.0}]}"#;
const EXT: &str = r#"{"type":"object","properties":{"speed":{"type":"number","exclusiveMinimum":0,"maximum":50}},"additionalProperties":false,"examples":[{}]}"#;

fn cid(s: &str) -> ComponentId {
    ComponentId::new(s).unwrap_or_else(|e| panic!("bad id {s}: {e}"))
}

struct Leaf {
    id: ComponentId,
    schema: ParamSchema,
    /// A parameter that the factory (not the schema) rejects when it is exactly 13.0: lets tests
    /// see the trial build ("dry run") of every component kind.
    unlucky: Option<&'static str>,
}

impl Leaf {
    fn new(id: &str, schema: &str) -> Self {
        Self {
            id: cid(id),
            schema: ParamSchema::from_json_str(schema).unwrap_or_else(|e| panic!("{id}: {e}")),
            unlucky: None,
        }
    }

    fn unlucky(mut self, key: &'static str) -> Self {
        self.unlucky = Some(key);
        self
    }

    fn check(&self, params: &ParamValue) -> Result<(), BuildError> {
        match self.unlucky {
            Some(key) if params.get(key).and_then(ParamValue::as_f64) == Some(13.0) => {
                Err(BuildError::new(format!("`{key}` of 13 is unlucky")).at(format!("/{key}")))
            }
            _ => Ok(()),
        }
    }
}

struct DummySensor(ComponentId);

impl Sensor for DummySensor {
    fn id(&self) -> &ComponentId {
        &self.0
    }
    fn sense(
        &self,
        _: &mut SenseContext<'_>,
        _: &Observer<'_>,
        _: &[Target<'_>],
        _: &mut ContactBuffer<'_>,
    ) {
    }
}

struct Dummy(ComponentId);

impl Condition for Dummy {
    fn id(&self) -> &ComponentId {
        &self.0
    }
    fn check(
        &self,
        _: &mut umbra_core::BehaviorContext<'_>,
        _: &umbra_core::EntityView<'_>,
        _: &mut umbra_core::Blackboard,
    ) -> bool {
        false
    }
}

impl Action for Dummy {
    fn id(&self) -> &ComponentId {
        &self.0
    }
    fn instantiate(&self, _: TickRate) -> Result<Box<dyn ActionInstance>, InstantiateError> {
        Err(InstantiateError::Other {
            message: "stub".into(),
        })
    }
}

struct DummyBehavior(BehaviorId);

impl Behavior for DummyBehavior {
    fn id(&self) -> &BehaviorId {
        &self.0
    }
    fn instantiate(&self, _: TickRate) -> Result<Box<dyn BehaviorInstance>, InstantiateError> {
        Err(InstantiateError::Other {
            message: "stub".into(),
        })
    }
}

macro_rules! leaf_factory {
    ($product:ident) => {
        impl Factory<dyn $product> for Leaf {
            fn id(&self) -> &ComponentId {
                &self.id
            }
            fn param_schema(&self) -> &ParamSchema {
                &self.schema
            }
            fn build(&self, params: &ParamValue) -> Result<Arc<dyn $product>, BuildError> {
                self.check(params)?;
                Ok(Arc::new(Dummy(self.id.clone())))
            }
        }
    };
}
leaf_factory!(Condition);
leaf_factory!(Action);

impl Factory<dyn Behavior> for Leaf {
    fn id(&self) -> &ComponentId {
        &self.id
    }
    fn param_schema(&self) -> &ParamSchema {
        &self.schema
    }
    fn build(&self, params: &ParamValue) -> Result<Arc<dyn Behavior>, BuildError> {
        self.check(params)?;
        let id = BehaviorId::new(self.id.as_str()).map_err(|e| BuildError::new(e.to_string()))?;
        Ok(Arc::new(DummyBehavior(id)))
    }
}

fn info(source: Provenance) -> ComponentInfo {
    ComponentInfo::new("stub", source, Version::new(1, 0, 0))
}

impl Factory<dyn Sensor> for Leaf {
    fn id(&self) -> &ComponentId {
        &self.id
    }
    fn param_schema(&self) -> &ParamSchema {
        &self.schema
    }
    fn build(&self, params: &ParamValue) -> Result<Arc<dyn Sensor>, BuildError> {
        self.check(params)?;
        Ok(Arc::new(DummySensor(self.id.clone())))
    }
}

const EYE: &str = r#"{"type":"object","properties":{"range_m":{"type":"number","exclusiveMinimum":0},"fov_deg":{"type":"number","exclusiveMinimum":0,"maximum":360}},"required":["range_m"],"additionalProperties":false,"examples":[{"range_m":1.0}]}"#;

/// The registry described in the module docs.
pub fn registry() -> Registry {
    let (mut registry, _) = base_registry();
    let mut condition = |id: &str, schema: &str, info: ComponentInfo| {
        registry
            .register_condition(
                Arc::new(Leaf::new(id, schema).unlucky(unlucky_key(id))),
                info,
            )
            .unwrap_or_else(|e| panic!("{e}"));
    };
    condition("stub.cond.flag", FLAG, info(Provenance::Builtin));
    condition(
        "stub.cond.near",
        NEAR,
        info(Provenance::Builtin).with_blackboard_param(BlackboardParam::new(
            "store_as",
            BbType::Entity,
            Access::Write,
        )),
    );
    condition("stub.cond.chance", CHANCE, info(Provenance::Builtin));
    condition(
        "stub.cond.has",
        HAS,
        info(Provenance::Builtin).with_blackboard_param(BlackboardParam::new(
            "target_key",
            BbType::Entity,
            Access::Read,
        )),
    );
    let mut action = |id: &str, schema: &str, info: ComponentInfo| {
        registry
            .register_action(
                Arc::new(Leaf::new(id, schema).unlucky(unlucky_key(id))),
                info,
            )
            .unwrap_or_else(|e| panic!("{e}"));
    };
    action("stub.action.noop", NOOP, info(Provenance::Builtin));
    action(
        "stub.action.go",
        GO,
        info(Provenance::Builtin).with_blackboard_param(BlackboardParam::new(
            "target_key",
            BbType::Entity,
            Access::Read,
        )),
    );
    action("stub.action.wait", WAIT, info(Provenance::Builtin));
    registry
        .register_behavior(
            Arc::new(Leaf::new("stub.behavior.ext", EXT).unlucky("speed")),
            info(Provenance::Plugin)
                .with_param_type("target", BbType::Entity)
                .with_param_type("speed", BbType::Number)
                .with_output("found", BbType::Entity),
        )
        .unwrap_or_else(|e| panic!("{e}"));
    registry
        .register_behavior(
            Arc::new(Leaf::new("stub.behavior.plain", EXT).unlucky("speed")),
            info(Provenance::Plugin),
        )
        .unwrap_or_else(|e| panic!("{e}"));
    registry
        .register_sensor(Arc::new(
            Leaf::new("stub.sensor.eye", EYE).unlucky("fov_deg"),
        ))
        .unwrap_or_else(|e| panic!("{e}"));
    registry
}

/// Registers `id` as a behavior with exactly `schema` and the given bindable params (`types`,
/// binding-only ones are `entity` and `position`): the "twin" of a graph whose interface produced
/// that schema (WI-14 equivalence test).
pub fn register_twin(
    registry: &mut Registry,
    id: &str,
    schema: &ParamSchema,
    types: &[(&str, BbType)],
) {
    let leaf = Leaf {
        id: cid(id),
        schema: schema.clone(),
        unlucky: None,
    };
    let described = types
        .iter()
        .fold(info(Provenance::Plugin), |i, (name, ty)| {
            i.with_param_type(*name, *ty)
        });
    registry
        .register_behavior(Arc::new(leaf), described)
        .unwrap_or_else(|e| panic!("{e}"));
}

/// Registers `ids` as behaviors that call each other in order (`ids[0]` calls `ids[1]`, ...): a
/// registered `subgraph` chain, as `register_graphs` leaves it (ASM-099 e). The last one is a
/// plain behavior with no references.
pub fn register_chain(registry: &mut Registry, ids: &[&str]) {
    for (i, id) in ids.iter().enumerate().rev() {
        let mut described = info(Provenance::File);
        if let Some(next) = ids.get(i + 1) {
            described = described.with_subgraph(cid(next), None);
        }
        registry
            .register_behavior(Arc::new(Leaf::new(id, EXT)), described)
            .unwrap_or_else(|e| panic!("{e}"));
    }
}

/// A stub behavior factory with the `stub.behavior.ext` schema.
pub fn plain_behavior(id: &str) -> impl Factory<dyn Behavior> {
    Leaf::new(id, EXT)
}

/// The parameter that `build` rejects at 13.0 for the leaf `id`, so every kind has a dry run.
fn unlucky_key(id: &str) -> &'static str {
    match id {
        "stub.cond.near" => "range_m",
        "stub.action.wait" => "duration_s",
        _ => "",
    }
}

pub fn loader_for(registry: &Registry) -> Loader<'_> {
    loader(registry)
}

/// Loads graph files with the stub registry and `Provenance::File`.
pub fn graphs(files: &[SourceFile]) -> Result<GraphSet, Diagnostics> {
    let registry = registry();
    loader(&registry).graphs_from_sources(files, Provenance::File)
}

/// Like [`graphs`] with a depth limit.
pub fn graphs_with_depth(files: &[SourceFile], depth: u32) -> Result<GraphSet, Diagnostics> {
    let registry = registry();
    let mut options = LoaderOptions::default();
    options.max_subgraph_depth = depth;
    Loader::with_options(&registry, options)
        .unwrap_or_else(|e| panic!("loader: {e}"))
        .graphs_from_sources(files, Provenance::File)
}

/// The diagnostics of loading one graph file.
pub fn graph_errors(text: &str) -> Vec<umbra_templates::Diagnostic> {
    errors(graphs(&[src(PATH, text)]))
}

/// `(code, path, line, column)` rows of loading one graph file.
pub fn graph_rows(text: &str) -> Vec<Row> {
    rows(&graph_errors(text))
}

/// Loads one graph file that must be valid and returns its graph.
pub fn one_graph(text: &str) -> Arc<umbra_core::GraphDef> {
    let set = ok(graphs(&[src(PATH, text)]));
    assert_eq!(set.len(), 1);
    Arc::clone(set.iter().next().unwrap_or_else(|| panic!("one graph")))
}

/// A minimal valid graph file: header, one `stub.action.noop` root.
pub fn minimal(id: &str) -> String {
    lines(&[
        "schema_version = 1",
        &format!("id = \"{id}\""),
        "version = \"1.0.0\"",
        "root = \"r\"",
        "[nodes.r]",
        "type = \"action\"",
        "ref = \"stub.action.noop\"",
    ])
}

/// A graph file whose root node `r` calls the subgraph `target` with `params` (inline TOML
/// table text, for example `{ n = 1 }`), plus the given extra lines before the nodes.
pub fn caller(
    id: &str,
    target: &str,
    version: Option<&str>,
    params: &str,
    head: &[&str],
) -> String {
    let mut all = vec![
        "schema_version = 1".to_string(),
        format!("id = \"{id}\""),
        "version = \"1.0.0\"".to_string(),
        "root = \"r\"".to_string(),
    ];
    all.extend(head.iter().map(|l| (*l).to_string()));
    all.push("[nodes.r]".into());
    all.push("type = \"subgraph\"".into());
    all.push(format!("ref = \"{target}\""));
    if let Some(v) = version {
        all.push(format!("version = \"{v}\""));
    }
    if !params.is_empty() {
        all.push(format!("params = {params}"));
    }
    let refs: Vec<&str> = all.iter().map(String::as_str).collect();
    lines(&refs)
}

/// A graph file `id` with the given interface params line (`params = { .. }`) and a root `r`
/// that is a `stub.action.noop`.
pub fn with_interface(id: &str, interface_params: &str) -> String {
    lines(&[
        "schema_version = 1",
        &format!("id = \"{id}\""),
        "version = \"1.0.0\"",
        "root = \"r\"",
        "[interface]",
        interface_params,
        "[nodes.r]",
        "type = \"action\"",
        "ref = \"stub.action.noop\"",
    ])
}
