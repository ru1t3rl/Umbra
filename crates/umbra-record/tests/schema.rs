//! M7 WI-11: `recording.v1.schema.json` is the published schema and does not drift from what the
//! writer emits (D-14, ADR 0053). The reader is authoritative; this guard checks that every key
//! the writer emits is declared, every required key is present, and the enumerations match.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod common;

use common::{MODELS, every_record, header, write_bytes};
use serde_json::Value;

fn schema_json() -> Value {
    serde_json::from_str(umbra_record::RECORDING_FILE_SCHEMA).expect("the schema is JSON")
}

/// Follows `$ref` (local pointers only).
fn resolve<'a>(root: &'a Value, node: &'a Value) -> &'a Value {
    match node["$ref"].as_str() {
        Some(r) => {
            let target = root
                .pointer(r.trim_start_matches('#'))
                .unwrap_or_else(|| panic!("dangling $ref {r}"));
            resolve(root, target)
        }
        None => node,
    }
}

/// Fails closed: a schema keyword the guard does not implement must not be silently ignored.
fn check_keywords(node: &Value) {
    const HANDLED: [&str; 13] = [
        "$ref",
        "oneOf",
        "const",
        "enum",
        "type",
        "pattern",
        "minimum",
        "maximum",
        "minItems",
        "maxItems",
        "items",
        "properties",
        "required",
    ];
    const ANNOTATIONS: [&str; 5] = ["$schema", "$id", "$defs", "title", "description"];
    for key in node.as_object().into_iter().flat_map(|o| o.keys()) {
        assert!(
            HANDLED.contains(&key.as_str()) || ANNOTATIONS.contains(&key.as_str()),
            "the guard does not implement the schema keyword `{key}`"
        );
    }
}

fn type_matches(name: &str, v: &Value) -> bool {
    match name {
        "string" => v.is_string(),
        "integer" => v.is_i64() || v.is_u64(),
        "number" => v.is_number(),
        "boolean" => v.is_boolean(),
        "null" => v.is_null(),
        "object" => v.is_object(),
        "array" => v.is_array(),
        other => panic!("the guard does not know type {other}"),
    }
}

fn is_decimal(s: &str) -> bool {
    s == "0"
        || (!s.starts_with('0')
            && !s.is_empty()
            && s.len() <= 20
            && s.bytes().all(|b| b.is_ascii_digit()))
}

fn is_hex64(s: &str) -> bool {
    s.len() == 16
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// A strict structural check of `value` against `node`: JSON type, `const`, `enum`, the two
/// patterns the schema uses, integer bounds, array sizes, `items`, `oneOf` (exactly one branch),
/// `required`, and, for objects with `properties`, that every key is declared and every declared
/// key is required (except one whose description starts "Only with"). The reader ignores unknown
/// keys, so the schema allows them; this guard is stricter on purpose, to catch drift.
fn validate(root: &Value, node: &Value, value: &Value, at: &str) -> Result<(), String> {
    check_keywords(node);
    let node = resolve(root, node);
    check_keywords(node);
    if let Some(branches) = node["oneOf"].as_array() {
        let passing = branches
            .iter()
            .filter(|b| validate(root, b, value, at).is_ok())
            .count();
        if passing != 1 {
            return Err(format!("{at}: {passing} oneOf branches accept {value}"));
        }
    }
    if let Some(expected) = node.get("const")
        && expected != value
    {
        return Err(format!("{at}: expected {expected}, got {value}"));
    }
    if let Some(choices) = node["enum"].as_array()
        && !choices.contains(value)
    {
        return Err(format!("{at}: {value} is not one of {choices:?}"));
    }
    check_type(node, value, at)?;
    check_pattern(node, value, at)?;
    check_bounds(node, value, at)?;
    if let (Some(items), Some(list)) = (node.get("items"), value.as_array()) {
        for (i, item) in list.iter().enumerate() {
            validate(root, items, item, &format!("{at}[{i}]"))?;
        }
    }
    if let (Some(props), Some(object)) = (node["properties"].as_object(), value.as_object()) {
        check_object(root, node, props, object, at)?;
    }
    Ok(())
}

fn check_type(node: &Value, value: &Value, at: &str) -> Result<(), String> {
    let ok = match &node["type"] {
        Value::String(t) => type_matches(t, value),
        Value::Array(ts) => ts
            .iter()
            .any(|t| type_matches(t.as_str().unwrap_or(""), value)),
        _ => true,
    };
    ok.then_some(())
        .ok_or_else(|| format!("{at}: {value} is not of type {}", node["type"]))
}

fn check_pattern(node: &Value, value: &Value, at: &str) -> Result<(), String> {
    let (Some(pattern), Some(text)) = (node["pattern"].as_str(), value.as_str()) else {
        return Ok(());
    };
    let ok = match pattern {
        "^(0|[1-9][0-9]{0,19})$" => is_decimal(text),
        "^[0-9a-f]{16}$" => is_hex64(text),
        other => panic!("the guard does not know pattern {other}"),
    };
    ok.then_some(())
        .ok_or_else(|| format!("{at}: {text:?} does not match {pattern}"))
}

fn check_bounds(node: &Value, value: &Value, at: &str) -> Result<(), String> {
    if let Some(n) = value.as_i64() {
        if node["minimum"].as_i64().is_some_and(|m| n < m)
            || node["maximum"].as_i64().is_some_and(|m| n > m)
        {
            return Err(format!("{at}: {n} is out of bounds"));
        }
    } else if let Some(n) = value.as_u64()
        && node["maximum"].as_u64().is_some_and(|m| n > m)
    {
        return Err(format!("{at}: {n} is out of bounds"));
    }
    if let Some(list) = value.as_array() {
        let len = list.len() as u64;
        if node["minItems"].as_u64().is_some_and(|m| len < m)
            || node["maxItems"].as_u64().is_some_and(|m| len > m)
        {
            return Err(format!("{at}: {len} items is out of bounds"));
        }
    }
    Ok(())
}

fn check_object(
    root: &Value,
    node: &Value,
    props: &serde_json::Map<String, Value>,
    object: &serde_json::Map<String, Value>,
    at: &str,
) -> Result<(), String> {
    let required: Vec<&str> = node["required"]
        .as_array()
        .map(|r| r.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    for key in &required {
        if !object.contains_key(*key) {
            return Err(format!("{at}: required `{key}` is absent"));
        }
    }
    for (key, member) in object {
        let declared = props
            .get(key)
            .ok_or_else(|| format!("{at}: `{key}` is not declared by the schema"))?;
        validate(root, declared, member, &format!("{at}.{key}"))?;
    }
    for (key, declared) in props {
        let optional = declared["description"]
            .as_str()
            .is_some_and(|d| d.starts_with("Only with"));
        if !optional && !required.contains(&key.as_str()) {
            return Err(format!("{at}: `{key}` is declared but not required"));
        }
    }
    Ok(())
}

/// Every line of `bytes` against the top-level schema.
fn check_lines(schema: &Value, bytes: Vec<u8>) {
    let text = String::from_utf8(bytes).expect("utf-8");
    for (i, line) in text.lines().enumerate() {
        let value: Value = serde_json::from_str(line).expect("line");
        validate(schema, schema, &value, &format!("line {}", i + 1))
            .unwrap_or_else(|e| panic!("{e}\n{line}"));
    }
}

#[test]
fn given_the_schema_then_it_is_the_published_2020_12_document() {
    let schema = schema_json();

    assert_eq!(
        schema["$schema"],
        "https://json-schema.org/draft/2020-12/schema"
    );
    assert_eq!(schema["$id"], "urn:umbra:schema:recording:v1");
    assert_eq!(
        schema["$defs"]["header"]["properties"]["version"]["maximum"],
        1
    );
    assert_eq!(
        schema["$defs"]["header"]["properties"]["format"]["const"],
        umbra_record::FORMAT
    );
}

#[test]
fn given_a_recording_of_every_kind_then_every_line_conforms_recursively() {
    let schema = schema_json();
    let mut bare = header();
    bare.scenario.relative_path = None;
    bare.scenario.files.clear();
    bare.plugins.clear();
    bare.network_adapters.clear();

    check_lines(&schema, write_bytes(&header(), &every_record()));
    check_lines(&schema, write_bytes(&bare, &[]));
}

#[test]
fn given_the_sample_then_every_command_and_message_definition_is_exercised() {
    let schema = schema_json();
    let text = String::from_utf8(write_bytes(&header(), &every_record())).expect("utf-8");
    let mut used = std::collections::BTreeSet::new();
    for line in text.lines() {
        let value: Value = serde_json::from_str(line).expect("line");
        if let Some(kind) = value["command"]["kind"].as_str() {
            used.insert(format!("command_{kind}"));
        }
        for m in value["messages"].as_array().into_iter().flatten() {
            used.insert(format!("message_{}", m["kind"].as_str().expect("kind")));
        }
    }

    let defined: std::collections::BTreeSet<String> = schema["$defs"]
        .as_object()
        .expect("defs")
        .keys()
        .filter(|k| k.starts_with("command_") || k.starts_with("message_"))
        .cloned()
        .collect();

    assert_eq!(used, defined);
}

#[test]
fn given_the_guard_then_it_rejects_drifted_lines() {
    let schema = schema_json();
    let good = String::from_utf8(write_bytes(&header(), &every_record())).expect("utf-8");
    let drifted = [
        good.replace("\"count\":", "\"size\":"),
        good.replace("\"hash_every\":1", "\"hash_every\":\"1\""),
        good.replacen("\"seed\":\"18446744073709551615\"", "\"seed\":\"007\"", 1),
        good.replacen("\"hash\":\"0", "\"hash\":\"A", 1),
        good.replace("\"kind\":\"remove\"", "\"kind\":\"delete\""),
    ];
    for bad in drifted {
        assert_ne!(bad, good);
        let failed = bad.lines().any(|line| {
            let value: Value = serde_json::from_str(line).expect("line");
            validate(&schema, &schema, &value, "line").is_err()
        });
        assert!(failed, "the guard accepted a drifted recording");
    }
}

#[test]
fn given_the_dead_reckoning_models_then_the_schema_enumerates_exactly_the_written_names() {
    let schema = schema_json();
    let text = String::from_utf8(write_bytes(&header(), &every_record())).expect("utf-8");
    let mut written: Vec<String> = text
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .flat_map(|v| v["messages"].as_array().cloned().unwrap_or_default())
        .filter_map(|m| m["dead_reckoning"].as_str().map(str::to_owned))
        .collect();
    written.sort();
    let mut declared: Vec<String> =
        schema["$defs"]["message_entity_state"]["properties"]["dead_reckoning"]["enum"]
            .as_array()
            .expect("enum")
            .iter()
            .map(|v| v.as_str().expect("name").to_owned())
            .collect();
    declared.sort();

    assert_eq!(written.len(), MODELS.len());
    assert_eq!(written, declared);
}
