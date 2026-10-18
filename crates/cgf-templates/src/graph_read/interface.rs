//! `[interface]` (ADR 0014): parameters as JSON Schema fragments, output ports, result text.
//!
//! A fragment with a JSON type (`boolean`, `number`, `integer`, `string`) goes into one assembled
//! object schema (`required` = params without `default`, `additionalProperties = false`). A
//! fragment with `type = "entity"` or `"position"` is binding-only: it is listed in
//! `Interface::types` but kept out of the schema and cannot have a `default` (ADR 0014 amendment).

use std::collections::BTreeMap;

use cgf_core::{BbType, Interface, OutputPort, ParamSchema, ParamValue};
use serde_json::{Map, json};

use super::{bb_type, check_name, shown};
use crate::diagnostic::DiagnosticCode;
use crate::reader::Reader;
use crate::schema::CompiledSchema;
use crate::value::{LTable, LValue, Origin};

/// Reads the optional `[interface]` of the graph table; a graph without one has no params and no
/// outputs. `None` when anything in it is wrong.
pub(super) fn read(reader: &mut Reader<'_>, graph: &LTable) -> Option<Interface> {
    let Some(value) = graph.get("interface") else {
        return assemble(reader, Params::default(), graph.origin(), None, None);
    };
    let table = reader.table("interface", value)?;
    reader.unknown_keys(table, &["params", "outputs", "result"]);
    let params = table
        .get("params")
        .map_or_else(|| Some(Params::default()), |v| params(reader, v));
    let outputs = table
        .get("outputs")
        .map_or_else(|| Some(BTreeMap::new()), |v| outputs(reader, v));
    let result = table.get("result").map_or(Some(None), |v| {
        reader.string("result", v).map(|s| Some(s.to_string()))
    });
    assemble(reader, params?, table.origin(), outputs, result?)
}

/// The keys of an `entity` or `position` fragment.
const BINDING_ONLY_KEYS: [&str; 2] = ["type", "description"];

/// What the `params` table yields.
#[derive(Default)]
struct Params {
    /// `properties` of the assembled schema.
    properties: Map<String, ParamValue>,
    required: Vec<String>,
    types: BTreeMap<String, BbType>,
    defaults: BTreeMap<String, ParamValue>,
}

fn assemble(
    reader: &mut Reader<'_>,
    params: Params,
    at: &Origin,
    outputs: Option<BTreeMap<String, OutputPort>>,
    result: Option<String>,
) -> Option<Interface> {
    let mut schema = json!({
        "type": "object",
        "properties": params.properties,
        "additionalProperties": false,
    });
    if !params.required.is_empty() {
        schema["required"] = json!(params.required);
    }
    schema["examples"] = json!([example(&params)]);
    let Ok(schema) = ParamSchema::from_json_str(&schema.to_string()) else {
        let message = "the interface parameters could not be assembled into a schema".to_string();
        reader.error(DiagnosticCode::InvalidValue, message, at);
        return None;
    };
    let mut interface = Interface::new(schema);
    for (name, ty) in params.types {
        interface = interface.with_type(name, ty);
    }
    for (name, default) in params.defaults {
        interface = interface.with_default(name, default);
    }
    for (port, out) in outputs.unwrap_or_default() {
        interface = interface.with_output(port, out);
    }
    if let Some(text) = result {
        interface = interface.with_result(text);
    }
    Some(interface)
}

/// One documented example of the assembled schema (REQ-B-17): every literal param at its
/// `default`, or for a param without one at the simplest value its keywords allow.
fn example(params: &Params) -> ParamValue {
    let object: Map<String, ParamValue> = params
        .properties
        .iter()
        .map(|(name, fragment)| {
            let value = params
                .defaults
                .get(name)
                .cloned()
                .unwrap_or_else(|| simplest_value(fragment));
            (name.clone(), value)
        })
        .collect();
    ParamValue::Object(object)
}

/// The first `enum` member, else the lowest value a numeric fragment allows, else the zero of
/// its type. Best effort: it only has to document the shape.
fn simplest_value(fragment: &ParamValue) -> ParamValue {
    if let Some(first) = fragment.get("enum").and_then(|e| e.get(0)) {
        return first.clone();
    }
    let number = |key: &str| fragment.get(key).and_then(ParamValue::as_f64);
    let integer = fragment.get("type").and_then(ParamValue::as_str) == Some("integer");
    match fragment.get("type").and_then(ParamValue::as_str) {
        Some("boolean") => json!(false),
        Some("string") => json!(""),
        Some("number" | "integer") => {
            let value = number("minimum")
                .or_else(|| number("exclusiveMinimum").map(|m| m.floor() + 1.0))
                .or_else(|| number("maximum"))
                .unwrap_or(0.0);
            if integer {
                format!("{:.0}", value.trunc())
                    .parse::<i64>()
                    .map_or_else(|_| json!(0), |n| json!(n))
            } else {
                json!(value)
            }
        }
        _ => ParamValue::Null,
    }
}

/// `params = { <name> = { type = .., default = .., <keywords> } }`. `None` when any is bad.
fn params(reader: &mut Reader<'_>, value: &LValue) -> Option<Params> {
    let table = reader.table("interface.params", value)?;
    let mut out = Params::default();
    let mut ok = true;
    for (name, entry) in table.iter() {
        ok &= check_name(reader, name, "interface parameter", entry.key_origin());
        ok &= param(reader, &mut out, name, entry.value());
    }
    ok.then_some(out)
}

/// One parameter fragment; returns whether it is good.
fn param(reader: &mut Reader<'_>, out: &mut Params, name: &str, value: &LValue) -> bool {
    let label = format!("interface parameter `{}`", shown(name));
    let Some(fragment) = reader.table(&label, value) else {
        return false;
    };
    let Some(type_value) = fragment.get("type") else {
        let message = format!("{label} is missing required key `type`");
        reader.error(DiagnosticCode::MissingKey, message, fragment.origin());
        return false;
    };
    let Some(ty) = param_type(reader, &label, type_value) else {
        return false;
    };
    out.types.insert(name.to_string(), ty);
    if matches!(ty, BbType::Entity | BbType::Position) {
        return binding_only(reader, &label, fragment);
    }
    literal(reader, out, name, &label, fragment)
}

/// The JSON type vocabulary of a fragment (`boolean`, not `bool`) plus `entity` and `position`.
fn param_type(reader: &mut Reader<'_>, label: &str, value: &LValue) -> Option<BbType> {
    let text = reader.string("type", value)?;
    let ty = match text {
        "boolean" => Some(BbType::Bool),
        "bool" => None,
        other => BbType::from_name(other),
    };
    if ty.is_none() {
        let message = format!(
            "{label}: unsupported type `{}` (use boolean, number, integer, string, entity or position)",
            shown(text)
        );
        reader.error(DiagnosticCode::InvalidValue, message, value.origin());
    }
    ty
}

/// `entity` and `position` params: only `type` and `description`; no `default`.
fn binding_only(reader: &mut Reader<'_>, label: &str, fragment: &LTable) -> bool {
    if let Some(default) = fragment.get("default") {
        let message = format!("{label} is binding-only and cannot have a `default`");
        reader.error(DiagnosticCode::InvalidValue, message, default.origin());
        return false;
    }
    let clean = fragment
        .iter()
        .all(|(key, _)| BINDING_ONLY_KEYS.contains(&key));
    reader.unknown_keys(fragment, &BINDING_ONLY_KEYS);
    clean
}

/// A literal-capable param: compile the fragment, check its `default` against it, and add it to
/// the assembled schema.
fn literal(
    reader: &mut Reader<'_>,
    out: &mut Params,
    name: &str,
    label: &str,
    fragment: &LTable,
) -> bool {
    let json = fragment.to_param();
    let compiled = ParamSchema::from_json_str(&json.to_string())
        .map_err(|e| (String::new(), e.to_string()))
        .and_then(|schema| CompiledSchema::compile(&schema).map_err(|e| (e.pointer, e.message)));
    let compiled = match compiled {
        Ok(compiled) => compiled,
        Err((pointer, message)) => {
            let at = keyword_origin(fragment, &pointer);
            let message = format!("{label}: {message}");
            reader.error(DiagnosticCode::InvalidValue, message, at);
            return false;
        }
    };
    let mut good = true;
    if let Some(default) = fragment.get("default") {
        for found in compiled.validate(default, &format!("{label} default")) {
            let mut found = found;
            found.code = DiagnosticCode::InvalidValue;
            reader.push_diagnostic(found);
            good = false;
        }
        out.defaults.insert(name.to_string(), default.to_param());
    } else {
        out.required.push(name.to_string());
    }
    out.properties.insert(name.to_string(), json);
    good
}

/// The key origin of the keyword a schema error points at (`/keyword/..`), else the fragment.
fn keyword_origin<'t>(fragment: &'t LTable, pointer: &str) -> &'t Origin {
    pointer
        .strip_prefix('/')
        .and_then(|rest| rest.split('/').next())
        .and_then(|keyword| fragment.entry(keyword))
        .map_or_else(|| fragment.origin(), |entry| entry.key_origin())
}

/// `outputs = { <port> = { type = .., from = .. } }`. `None` when any is bad.
fn outputs(reader: &mut Reader<'_>, value: &LValue) -> Option<BTreeMap<String, OutputPort>> {
    let table = reader.table("interface.outputs", value)?;
    let mut out = BTreeMap::new();
    let mut ok = true;
    for (port, entry) in table.iter() {
        ok &= check_name(reader, port, "output port", entry.key_origin());
        match output(reader, port, entry.value()) {
            Some(found) => {
                out.insert(port.to_string(), found);
            }
            None => ok = false,
        }
    }
    ok.then_some(out)
}

fn output(reader: &mut Reader<'_>, port: &str, value: &LValue) -> Option<OutputPort> {
    let label = format!("output port `{}`", shown(port));
    let table = reader.table(&label, value)?;
    reader.unknown_keys(table, &["type", "from"]);
    let ty = required(reader, table, &label, "type").and_then(|v| bb_type(reader, v));
    let from = required(reader, table, &label, "from").and_then(|v| {
        let key = reader.string("from", v)?;
        check_name(reader, key, "blackboard key", v.origin()).then(|| key.to_string())
    });
    Some(OutputPort::new(ty?, from?))
}

fn required<'t>(
    reader: &mut Reader<'_>,
    table: &'t LTable,
    label: &str,
    key: &str,
) -> Option<&'t LValue> {
    let found = table.get(key);
    if found.is_none() {
        let message = format!("{label} is missing required key `{key}`");
        reader.error(DiagnosticCode::MissingKey, message, table.origin());
    }
    found
}
