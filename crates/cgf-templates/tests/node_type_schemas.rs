//! M3 WI-4 review follow-up: the `cgf-core` node-type settings schemas (`NODE_TYPES`) stay inside
//! the ADR 0009 subset, accept their own `examples`, and encode the structural rules the graph
//! reader also enforces (WI-12 is authoritative). Requirements: REQ-B-7, REQ-B-17, ADR 0009,
//! ASM-090.

#![forbid(unsafe_code)]

use cgf_core::{NODE_TYPES, ParamSchema};
use cgf_templates::value::parse;
use cgf_templates::{CompiledSchema, SourceFile};
use serde_json::Value;

fn compiled(name: &str) -> (CompiledSchema, Value) {
    let info = NODE_TYPES
        .iter()
        .find(|n| n.name == name)
        .unwrap_or_else(|| panic!("no node type {name}"));
    let parsed =
        ParamSchema::from_json_str(info.settings_schema).unwrap_or_else(|e| panic!("{name}: {e}"));
    let schema = CompiledSchema::compile(&parsed)
        .unwrap_or_else(|e| panic!("{name} must compile in the ADR 0009 subset: {e}"));
    (schema, parsed.as_json().clone())
}

/// A JSON value as a TOML inline value (test data only: no nulls).
fn toml_inline(v: &Value) -> String {
    match v {
        Value::Object(m) => {
            let items: Vec<String> = m
                .iter()
                .map(|(k, x)| format!("{} = {}", Value::String(k.clone()), toml_inline(x)))
                .collect();
            format!("{{ {} }}", items.join(", "))
        }
        Value::Array(a) => format!(
            "[{}]",
            a.iter().map(toml_inline).collect::<Vec<_>>().join(", ")
        ),
        other => other.to_string(),
    }
}

fn violation_count(schema: &CompiledSchema, value: &Value) -> usize {
    let file = SourceFile::new("n.toml", format!("x = {}\n", toml_inline(value)));
    let (table, syntax) = parse(&file);
    assert_eq!(syntax.len(), 0, "{syntax:?}");
    let table = table.unwrap_or_else(|| panic!("no table"));
    let x = table.get("x").unwrap_or_else(|| panic!("no x"));
    schema.validate(x, "node").len()
}

#[test]
fn every_node_type_schema_compiles_and_accepts_its_examples() {
    for n in NODE_TYPES {
        let (schema, doc) = compiled(n.name);
        let examples = doc["examples"].as_array().cloned().unwrap_or_default();
        assert!(!examples.is_empty(), "{} has no examples", n.name);
        for ex in &examples {
            assert_eq!(violation_count(&schema, ex), 0, "{}: {ex}", n.name);
        }
    }
}

#[test]
fn composites_reject_an_empty_children_array() {
    for name in ["sequence", "selector", "parallel"] {
        let (schema, _) = compiled(name);
        let bad = serde_json::json!({ "children": [] });
        assert!(violation_count(&schema, &bad) > 0, "{name}");
    }
}

#[test]
fn fsm_transitions_need_exactly_one_of_when_or_on() {
    let (schema, _) = compiled("fsm");
    let fsm = |t: Value| serde_json::json!({ "initial": "a", "states": { "a": "n" }, "transitions": [t] });
    let ok_when = fsm(serde_json::json!({ "from": "a", "to": "a", "when": "g" }));
    let ok_on = fsm(serde_json::json!({ "from": "a", "to": "a", "on": "success" }));
    let both = fsm(serde_json::json!({ "from": "a", "to": "a", "when": "g", "on": "success" }));
    let neither = fsm(serde_json::json!({ "from": "a", "to": "a" }));

    assert_eq!(violation_count(&schema, &ok_when), 0);
    assert_eq!(violation_count(&schema, &ok_on), 0);
    assert!(violation_count(&schema, &both) > 0);
    assert!(violation_count(&schema, &neither) > 0);
}
