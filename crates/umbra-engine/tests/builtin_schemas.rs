//! AT-3 (contract rows): the built-in components supply their own JSON Schemas through the
//! registry, and those schemas are inside the ADR 0009 subset, validate their own `examples` and
//! build from them. Requirements: REQ-T-3, REQ-T-1, D-8, D-14, ADR 0008, ADR 0009, ASM-055.
//!
//! `umbra-motion` cannot use `umbra-templates`, so the compile-and-validate half of the contract lives
//! here, where both are available.

#![forbid(unsafe_code)]

mod common;

use serde_json::Value;

use common::registry;
use umbra_core::{ComponentId, ComponentKind, ParamSchema, Registry};
use umbra_templates::value::parse;
use umbra_templates::{CompiledSchema, SourceFile};

/// A JSON value as inline TOML (enough for parameter examples: numbers, strings, booleans, arrays,
/// objects).
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

fn every_component(registry: &Registry) -> Vec<(ComponentKind, ComponentId)> {
    ComponentKind::ALL
        .iter()
        .copied()
        .flat_map(|kind| {
            registry
                .ids(kind)
                .into_iter()
                .map(move |id| (kind, id.clone()))
        })
        .collect()
}

fn schema_of(registry: &Registry, kind: ComponentKind, id: &ComponentId) -> ParamSchema {
    registry
        .param_schema(kind, id)
        .unwrap_or_else(|| panic!("{id} has no schema"))
        .clone()
}

#[test]
fn builtin_registry_registers_exactly_the_m2_components() {
    let registry = registry();

    let motion: Vec<String> = registry
        .ids(ComponentKind::MotionModel)
        .iter()
        .map(ToString::to_string)
        .collect();
    let terrain: Vec<String> = registry
        .ids(ComponentKind::Terrain)
        .iter()
        .map(ToString::to_string)
        .collect();

    assert_eq!(motion, ["builtin.ground.kinematic"]);
    assert_eq!(terrain, ["builtin.terrain.flat"]);
}

/// ASM-055: behaviors and sensors have no registry entries before M3.
#[test]
fn builtin_registry_has_no_behaviors_or_sensors() {
    let registry = registry();

    assert_eq!(registry.ids(ComponentKind::Behavior).len(), 0);
    assert_eq!(registry.ids(ComponentKind::Sensor).len(), 0);
}

#[test]
fn every_registered_schema_compiles_and_its_examples_validate_and_build() {
    let registry = registry();
    let components = every_component(&registry);
    assert!(components.len() >= 2, "at least the two built-ins");

    for (kind, id) in components {
        let schema = schema_of(&registry, kind, &id);
        let compiled = CompiledSchema::compile(&schema)
            .unwrap_or_else(|e| panic!("{id}: schema outside the subset: {e}"));
        let examples = schema.as_json()["examples"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        assert!(!examples.is_empty(), "{id} documents at least one example");

        for example in examples {
            let text = format!("x = {}\n", toml_of(&example));
            let (table, syntax) = parse(&SourceFile::new("example.toml", text.as_str()));
            assert_eq!(
                syntax.len(),
                0,
                "{id}: example is not valid TOML: {syntax:?}"
            );
            let table = table.expect("table");
            let violations = compiled.validate(table.get("x").expect("x"), "example");
            assert_eq!(violations, [], "{id}: the example violates its own schema");

            let built = match kind {
                ComponentKind::MotionModel => registry
                    .motion_model(&id)
                    .expect("registered")
                    .build(&example)
                    .map(|_| ()),
                ComponentKind::Terrain => registry
                    .terrain(&id)
                    .expect("registered")
                    .build(&example)
                    .map(|_| ()),
                _ => Ok(()),
            };
            assert_eq!(built, Ok(()), "{id}: the example does not build");
        }
    }
}

/// D-14, ADR 0008: the schema is the component's own, served by the registry.
#[test]
fn ground_kinematic_schema_is_served_by_the_registry_not_by_templates() {
    let registry = registry();
    let id = ComponentId::new("builtin.ground.kinematic").expect("id");

    let schema = schema_of(&registry, ComponentKind::MotionModel, &id);

    assert_eq!(
        schema.as_json()["title"],
        "builtin.ground.kinematic parameters"
    );
    assert_eq!(schema.as_json()["additionalProperties"], false);
    assert_eq!(
        registry
            .motion_model(&id)
            .expect("registered")
            .param_schema(),
        &schema,
        "the registry's schema is the factory's schema"
    );
}

#[test]
fn flat_terrain_schema_requires_height_m_and_nothing_else() {
    let registry = registry();
    let id = ComponentId::new("builtin.terrain.flat").expect("id");

    let schema = schema_of(&registry, ComponentKind::Terrain, &id);

    assert_eq!(
        schema.as_json()["required"],
        serde_json::json!(["height_m"])
    );
    assert_eq!(schema.as_json()["properties"]["height_m"]["type"], "number");
    assert_eq!(schema.as_json()["additionalProperties"], false);
}

/// The ground kinematic schema accepts what the spec 7.1 example writes (units in the key names)
/// and rejects a value the model could not use.
#[test]
fn ground_kinematic_schema_rejects_a_zero_limit_and_an_unknown_key() {
    let registry = registry();
    let id = ComponentId::new("builtin.ground.kinematic").expect("id");
    let compiled = CompiledSchema::compile(&schema_of(&registry, ComponentKind::MotionModel, &id))
        .expect("compiles");
    let text = "x = { max_speed_mps = 0.0, max_turn_rate_dps = 40.0, max_accel_mps2 = 2.5, wings = true }\n";
    let (table, _) = parse(&SourceFile::new("v.toml", text));
    let table = table.expect("table");

    let found = compiled.validate(
        table.get("x").expect("x"),
        "motion model `builtin.ground.kinematic`",
    );

    let codes: Vec<&str> = found.iter().map(|d| d.code.as_str()).collect();
    assert_eq!(codes, ["param_out_of_range", "unknown_param"]);
}

/// ASM-069: the flat terrain schema bounds `height_m` to [-1000, 10000]; the factory itself only
/// checks presence and type.
#[test]
fn flat_terrain_schema_rejects_heights_outside_the_bounds() {
    let registry = registry();
    let id = ComponentId::new("builtin.terrain.flat").expect("id");
    let compiled = CompiledSchema::compile(&schema_of(&registry, ComponentKind::Terrain, &id))
        .expect("compiles");
    let codes = |height: &str| -> Vec<String> {
        let text = format!("x = {{ height_m = {height} }}\n");
        let (table, _) = parse(&SourceFile::new("t.toml", text.as_str()));
        let table = table.expect("table");
        compiled
            .validate(table.get("x").expect("x"), "terrain")
            .iter()
            .map(|d| d.code.as_str().to_string())
            .collect()
    };

    assert_eq!(codes("-1000.0"), Vec::<String>::new());
    assert_eq!(codes("10000.0"), Vec::<String>::new());
    assert_eq!(codes("-1000.5"), ["param_out_of_range"]);
    assert_eq!(codes("10000.5"), ["param_out_of_range"]);
}
