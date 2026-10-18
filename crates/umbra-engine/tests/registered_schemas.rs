//! WI-15 (AT-9, AT-13): every schema registered in the standard registry, of every kind, compiles
//! in the ADR 0009 subset, validates its own `examples`, and every example builds.
//! Requirements: REQ-T-3, REQ-B-17, ADR 0009, ADR 0014, ASM-077, ASM-084.
//!
//! The M2 `builtin_schemas.rs` keeps proving this for `builtin_registry` (motion and terrain);
//! this file covers behaviors (including the built-in graphs), sensors, conditions and actions.
//! `umbra-core` and `umbra-behavior` cannot use `umbra-templates`, so the compile-and-validate half lives
//! here.

#![forbid(unsafe_code)]

mod behavior_support;
mod common;

use behavior_support::standard;
use serde_json::Value;
use umbra_core::{ComponentId, ComponentKind, ParamValue, Registry};
use umbra_templates::value::parse;
use umbra_templates::{CompiledSchema, SourceFile};

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

fn build(
    registry: &Registry,
    kind: ComponentKind,
    id: &ComponentId,
    p: &ParamValue,
) -> Option<String> {
    let found = |e: umbra_core::BuildError| Some(e.to_string());
    match kind {
        ComponentKind::MotionModel => registry.motion_model(id)?.build(p).err().and_then(found),
        ComponentKind::Terrain => registry.terrain(id)?.build(p).err().and_then(found),
        ComponentKind::Behavior => registry.behavior(id)?.build(p).err().and_then(found),
        ComponentKind::Sensor => registry.sensor(id)?.build(p).err().and_then(found),
        ComponentKind::Condition => registry.condition(id)?.build(p).err().and_then(found),
        ComponentKind::Action => registry.action(id)?.build(p).err().and_then(found),
        _ => Some(format!("unhandled kind {kind:?}")),
    }
}

#[test]
fn every_schema_of_every_kind_compiles_validates_its_examples_and_builds() {
    let registry = standard();
    let mut seen = [0_usize; 6];

    for (slot, &kind) in ComponentKind::ALL.iter().enumerate() {
        for id in registry.ids(kind) {
            let schema = registry.param_schema(kind, id).expect("registered");
            let compiled = CompiledSchema::compile(schema)
                .unwrap_or_else(|e| panic!("{kind:?} {id}: schema outside the subset: {e}"));
            let examples = schema.as_json()["examples"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            assert!(!examples.is_empty(), "{kind:?} {id} documents an example");
            for example in examples {
                let text = format!("x = {}\n", toml_of(&example));
                let (table, syntax) = parse(&SourceFile::new("example.toml", text.as_str()));
                assert_eq!(syntax.len(), 0, "{id}: {syntax:?}");
                let table = table.expect("table");
                let violations = compiled.validate(table.get("x").expect("x"), "example");
                assert_eq!(
                    violations,
                    [],
                    "{kind:?} {id}: the example violates its schema"
                );
                assert_eq!(
                    build(&registry, kind, id, &example),
                    None,
                    "{kind:?} {id}: the example does not build"
                );
            }
            seen[slot] += 1;
        }
    }

    // motion, terrain, behaviors (2 graphs), sensor, 4 conditions, 4 actions
    assert_eq!(seen, [1, 1, 2, 1, 4, 4]);
}
