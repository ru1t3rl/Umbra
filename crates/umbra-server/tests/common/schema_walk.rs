//! A small JSON Schema walker for the response-conformance tests (AT-11, AT-9). It supports exactly
//! the keywords the generated documents use and fails closed on any other keyword (WI-18, m6):
//!
//! - checked: `type` (a name or an array such as `["integer", "null"]`), `properties`, `required`,
//!   `items`, `$ref` (a JSON pointer into the document, `#/components/schemas/X` or `#/$defs/X`),
//!   `enum`, `const`, `oneOf` (exactly one branch matches), `allOf`, `additionalProperties`
//!   (`false` or a schema), `propertyNames` (a schema every key must satisfy), `minimum`, `maximum`
//!   and `unevaluatedProperties: false` (a key must be named in `properties` of the schema or of
//!   an `allOf` branch, followed through `$ref`);
//! - ignored annotations: `description`, `title`, `example`, `examples`, `default`, `deprecated`,
//!   `format`, `pattern`, `readOnly`, `writeOnly`, `$schema`, `$id`, `$defs`, `definitions`,
//!   `externalDocs` and `discriminator` (a hint; `oneOf` decides).
//!
//! `pattern` and `format` are annotations here: the contract tests check shapes, not string syntax
//! (ASM-183).

use serde_json::Value;

const ANNOTATIONS: [&str; 16] = [
    "description",
    "title",
    "example",
    "examples",
    "default",
    "deprecated",
    "format",
    "pattern",
    "readOnly",
    "writeOnly",
    "$schema",
    "$id",
    "$defs",
    "definitions",
    "externalDocs",
    "discriminator",
];

const CHECKED: [&str; 14] = [
    "type",
    "properties",
    "required",
    "items",
    "$ref",
    "enum",
    "const",
    "oneOf",
    "allOf",
    "additionalProperties",
    "propertyNames",
    "unevaluatedProperties",
    "minimum",
    "maximum",
];

/// Checks `instance` against `schema`; `root` is the document `$ref`s point into.
///
/// # Errors
/// The first violation, with the instance path.
pub fn validate(root: &Value, schema: &Value, instance: &Value) -> Result<(), String> {
    walk(root, schema, instance, "$", 0)
}

fn walk(
    root: &Value,
    schema: &Value,
    instance: &Value,
    path: &str,
    depth: usize,
) -> Result<(), String> {
    if depth > 64 {
        return Err(format!("{path}: schema nesting is too deep"));
    }
    let Some(object) = schema.as_object() else {
        // `true` accepts everything; anything else is not a schema.
        return if schema == &Value::Bool(true) {
            Ok(())
        } else {
            Err(format!(
                "{path}: the schema is neither an object nor true: {schema}"
            ))
        };
    };
    for key in object.keys() {
        if !CHECKED.contains(&key.as_str()) && !ANNOTATIONS.contains(&key.as_str()) {
            return Err(format!(
                "{path}: unsupported schema keyword `{key}` (fail closed)"
            ));
        }
    }
    if let Some(target) = object.get("$ref") {
        let reference = target
            .as_str()
            .ok_or_else(|| format!("{path}: $ref is not a string"))?;
        let pointer = reference
            .strip_prefix('#')
            .ok_or_else(|| format!("{path}: only local $ref is supported: {reference}"))?;
        let resolved = root
            .pointer(pointer)
            .ok_or_else(|| format!("{path}: $ref {reference} does not resolve"))?;
        walk(root, resolved, instance, path, depth + 1)?;
    }
    if let Some(types) = object.get("type") {
        check_type(types, instance, path)?;
    }
    if let Some(allowed) = object.get("enum") {
        let list = allowed
            .as_array()
            .ok_or_else(|| format!("{path}: enum is not an array"))?;
        if !list.contains(instance) {
            return Err(format!("{path}: {instance} is not one of {allowed}"));
        }
    }
    if let Some(expected) = object.get("const")
        && expected != instance
    {
        return Err(format!("{path}: {instance} is not the constant {expected}"));
    }
    check_bounds(object, instance, path)?;
    if let Some(items) = object.get("items")
        && let Some(array) = instance.as_array()
    {
        for (i, element) in array.iter().enumerate() {
            walk(root, items, element, &format!("{path}[{i}]"), depth + 1)?;
        }
    }
    if let Some(map) = instance.as_object() {
        check_object(root, object, map, path, depth)?;
    }
    if let Some(branches) = object.get("allOf") {
        for branch in branches
            .as_array()
            .ok_or_else(|| format!("{path}: allOf is not an array"))?
        {
            walk(root, branch, instance, path, depth + 1)?;
        }
    }
    check_unevaluated(root, object, instance, path)?;
    if let Some(branches) = object.get("oneOf") {
        let list = branches
            .as_array()
            .ok_or_else(|| format!("{path}: oneOf is not an array"))?;
        let mut failures = Vec::new();
        let mut matched = 0_usize;
        for branch in list {
            match walk(root, branch, instance, path, depth + 1) {
                Ok(()) => matched += 1,
                Err(e) => failures.push(e),
            }
        }
        if matched != 1 {
            return Err(format!(
                "{path}: {matched} of {} oneOf branches match (needs exactly one): {}",
                list.len(),
                failures.join(" | ")
            ));
        }
    }
    Ok(())
}

/// `unevaluatedProperties: false`: every key of an object instance is named by the schema or by one
/// of its `allOf` branches.
fn check_unevaluated(
    root: &Value,
    schema: &serde_json::Map<String, Value>,
    instance: &Value,
    path: &str,
) -> Result<(), String> {
    let Some(rule) = schema.get("unevaluatedProperties") else {
        return Ok(());
    };
    if rule != &Value::Bool(false) {
        return Err(format!(
            "{path}: only `unevaluatedProperties: false` is supported"
        ));
    }
    if let Some(map) = instance.as_object() {
        let mut known = Vec::new();
        evaluated_names(root, schema, &mut known);
        if let Some(extra) = map.keys().find(|key| !known.contains(key)) {
            return Err(format!(
                "{path}.{extra}: property is not evaluated by the schema"
            ));
        }
    }
    Ok(())
}

fn type_matches(name: &str, instance: &Value) -> Result<bool, String> {
    Ok(match name {
        "null" => instance.is_null(),
        "boolean" => instance.is_boolean(),
        "string" => instance.is_string(),
        "array" => instance.is_array(),
        "object" => instance.is_object(),
        "number" => instance.is_number(),
        "integer" => instance.is_i64() || instance.is_u64(),
        other => return Err(format!("unknown type name `{other}`")),
    })
}

fn check_type(types: &Value, instance: &Value, path: &str) -> Result<(), String> {
    let names: Vec<&str> = match types {
        Value::String(name) => vec![name.as_str()],
        Value::Array(list) => list
            .iter()
            .map(|t| {
                t.as_str()
                    .ok_or_else(|| format!("{path}: non-string type {t}"))
            })
            .collect::<Result<_, _>>()?,
        other => {
            return Err(format!(
                "{path}: type is neither a string nor an array: {other}"
            ));
        }
    };
    for name in &names {
        if type_matches(name, instance).map_err(|e| format!("{path}: {e}"))? {
            return Ok(());
        }
    }
    Err(format!("{path}: expected type {names:?}, found {instance}"))
}

fn check_bounds(
    object: &serde_json::Map<String, Value>,
    instance: &Value,
    path: &str,
) -> Result<(), String> {
    let Some(number) = instance.as_f64() else {
        return Ok(());
    };
    if let Some(min) = object.get("minimum").and_then(Value::as_f64)
        && number < min
    {
        return Err(format!("{path}: {number} is below the minimum {min}"));
    }
    if let Some(max) = object.get("maximum").and_then(Value::as_f64)
        && number > max
    {
        return Err(format!("{path}: {number} is above the maximum {max}"));
    }
    Ok(())
}

fn check_object(
    root: &Value,
    schema: &serde_json::Map<String, Value>,
    instance: &serde_json::Map<String, Value>,
    path: &str,
    depth: usize,
) -> Result<(), String> {
    if let Some(required) = schema.get("required") {
        for name in required
            .as_array()
            .ok_or_else(|| format!("{path}: required is not an array"))?
        {
            let name = name
                .as_str()
                .ok_or_else(|| format!("{path}: non-string required"))?;
            if !instance.contains_key(name) {
                return Err(format!("{path}: missing required property `{name}`"));
            }
        }
    }
    let empty = serde_json::Map::new();
    let properties = match schema.get("properties") {
        Some(p) => p
            .as_object()
            .ok_or_else(|| format!("{path}: properties is not an object"))?,
        None => &empty,
    };
    for (name, value) in instance {
        let here = format!("{path}.{name}");
        if let Some(names) = schema.get("propertyNames") {
            walk(root, names, &Value::String(name.clone()), &here, depth + 1)?;
        }
        if let Some(property) = properties.get(name) {
            walk(root, property, value, &here, depth + 1)?;
        } else {
            match schema.get("additionalProperties") {
                Some(Value::Bool(false)) => {
                    return Err(format!("{here}: property is not in the schema"));
                }
                Some(Value::Bool(true)) | None => {}
                Some(extra) => walk(root, extra, value, &here, depth + 1)?,
            }
        }
    }
    Ok(())
}

/// The property names `schema` evaluates: its own `properties`, those of its `allOf` branches and
/// of whatever they `$ref`.
fn evaluated_names(root: &Value, schema: &serde_json::Map<String, Value>, out: &mut Vec<String>) {
    if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
        out.extend(properties.keys().cloned());
    }
    if let Some(target) = schema
        .get("$ref")
        .and_then(Value::as_str)
        .and_then(|reference| reference.strip_prefix('#'))
        .and_then(|pointer| root.pointer(pointer))
        .and_then(Value::as_object)
    {
        evaluated_names(root, target, out);
    }
    for branch in schema
        .get("allOf")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some(branch) = branch.as_object() {
            evaluated_names(root, branch, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::validate;

    #[test]
    fn the_walker_checks_what_it_claims_to() {
        let root = json!({"components": {"schemas": {"N": {"type": ["integer", "null"]}}}});
        let schema = json!({
            "type": "object",
            "required": ["a"],
            "properties": {"a": {"$ref": "#/components/schemas/N"}, "b": {"enum": ["x", "y"]}},
            "additionalProperties": false
        });
        assert!(validate(&root, &schema, &json!({"a": null, "b": "x"})).is_ok());
        assert!(validate(&root, &schema, &json!({"a": 1})).is_ok());
        assert!(
            validate(&root, &schema, &json!({})).is_err(),
            "missing required"
        );
        assert!(
            validate(&root, &schema, &json!({"a": "s"})).is_err(),
            "wrong type"
        );
        assert!(
            validate(&root, &schema, &json!({"a": 1, "b": "z"})).is_err(),
            "enum"
        );
        assert!(
            validate(&root, &schema, &json!({"a": 1, "c": 1})).is_err(),
            "extra property"
        );
        assert!(
            validate(&root, &json!({"not": {}}), &json!(1)).is_err(),
            "unknown keyword fails closed"
        );
        let wrapped = json!({
            "allOf": [{"$ref": "#/components/schemas/Body"}, {"properties": {"type": {}}}],
            "unevaluatedProperties": false
        });
        let root = json!({"components": {"schemas": {"Body": {"properties": {"x": {}}}}}});
        assert!(validate(&root, &wrapped, &json!({"type": "t", "x": 1})).is_ok());
        assert!(
            validate(&root, &wrapped, &json!({"type": "t", "y": 1})).is_err(),
            "a property no branch names"
        );
        let one_of = json!({"oneOf": [{"type": "integer"}, {"type": "number"}]});
        assert!(
            validate(&root, &one_of, &json!(1)).is_err(),
            "two branches match"
        );
        assert!(validate(&root, &one_of, &json!(1.5)).is_ok());
        // `allOf` with a tag object: both branches must hold.
        let tagged = json!({"allOf": [
            {"type": "object", "required": ["type"], "properties": {"type": {"const": "frame"}}},
            {"type": "object", "required": ["n"], "properties": {"n": {"type": "integer"}}}
        ]});
        assert!(validate(&root, &tagged, &json!({"type": "frame", "n": 1})).is_ok());
        assert!(
            validate(&root, &tagged, &json!({"type": "hello", "n": 1})).is_err(),
            "tag differs"
        );
        assert!(
            validate(&root, &tagged, &json!({"type": "frame"})).is_err(),
            "second branch fails"
        );
        // `additionalProperties` as a schema constrains the extra properties.
        let open = json!({"type": "object", "additionalProperties": {"type": "integer"}});
        assert!(validate(&root, &open, &json!({"x": 1, "y": 2})).is_ok());
        assert!(validate(&root, &open, &json!({"x": 1, "y": "no"})).is_err());
        // `propertyNames` constrains every key.
        let named = json!({"type": "object", "propertyNames": {"enum": ["a", "b"]}});
        assert!(validate(&root, &named, &json!({"a": 1, "b": 2})).is_ok());
        assert!(validate(&root, &named, &json!({"a": 1, "c": 2})).is_err());
    }
}
