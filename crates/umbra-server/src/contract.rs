//! The committed contract documents (REQ-X-4, REQ-X-6, ADR 0025, ADR 0027): the `OpenAPI` 3.1
//! document, the stream JSON Schema (root `StreamMessage`) and the `AsyncAPI` 3.0 description of the
//! stream channel. `cargo xtask gen-api` prints them through the `gen_api` example and writes or
//! compares `api/*.json`; the served `/api/v1/openapi.json` is the same `OpenAPI` value.
//!
//! Output is deterministic: `serde_json` keeps its maps sorted (the `preserve_order` feature is
//! never enabled) and [`render`] prints them pretty with an LF at the end.

use std::collections::BTreeSet;

use serde_json::{Map, Value, json};
use umbra_api_types::API_VERSION;

use crate::API_BASE;

/// File name of the `OpenAPI` document.
pub const OPENAPI_FILE: &str = "openapi.json";
/// File name of the stream JSON Schema.
pub const STREAM_SCHEMA_FILE: &str = "stream.schema.json";
/// File name of the `AsyncAPI` document.
pub const ASYNCAPI_FILE: &str = "asyncapi.json";

/// `$id` of the stream schema (ADR 0018 C4).
pub const STREAM_SCHEMA_ID: &str = "urn:umbra:schema:stream:v1";

/// The JSON Schema dialect of the stream schema.
const DIALECT: &str = "https://json-schema.org/draft/2020-12/schema";
/// Where `OpenAPI` keeps component schemas, and where the stream schema keeps its definitions.
const COMPONENTS_PREFIX: &str = "#/components/schemas/";
const DEFS_PREFIX: &str = "#/$defs/";
/// The component that is the root of the stream schema.
const STREAM_ROOT: &str = "StreamMessage";

/// Why the documents could not be generated (a bug in the generator, not bad input).
#[derive(Debug, thiserror::Error)]
pub enum ContractError {
    /// A document could not be turned into JSON.
    #[error("cannot serialise the document: {0}")]
    Serialise(#[from] serde_json::Error),
    /// A schema the stream schema needs is not a component of the document.
    #[error("the OpenAPI components have no schema `{0}`, which the stream schema needs")]
    MissingSchema(String),
    /// The stream root is not a JSON object.
    #[error("the schema `{0}` is not a JSON object")]
    NotAnObject(String),
}

/// The three generated documents.
#[derive(Debug, Clone, PartialEq)]
pub struct Documents {
    /// `api/openapi.json`.
    pub openapi: Value,
    /// `api/stream.schema.json`.
    pub stream_schema: Value,
    /// `api/asyncapi.json`.
    pub asyncapi: Value,
}

impl Documents {
    /// `(file name, rendered text)` of every document, in a fixed order.
    ///
    /// # Errors
    /// A document cannot be serialised.
    pub fn files(&self) -> Result<[(&'static str, String); 3], ContractError> {
        Ok([
            (OPENAPI_FILE, render(&self.openapi)?),
            (STREAM_SCHEMA_FILE, render(&self.stream_schema)?),
            (ASYNCAPI_FILE, render(&self.asyncapi)?),
        ])
    }
}

/// Pretty JSON with a trailing newline (the committed form).
///
/// # Errors
/// The value cannot be serialised.
pub fn render(value: &Value) -> Result<String, ContractError> {
    let mut text = serde_json::to_string_pretty(value)?;
    text.push('\n');
    Ok(text)
}

/// The `OpenAPI` document as JSON.
///
/// # Errors
/// The document cannot be serialised.
pub fn openapi_value() -> Result<Value, ContractError> {
    Ok(serde_json::to_value(crate::openapi::document())?)
}

/// Generates the three documents.
///
/// # Errors
/// See [`ContractError`].
pub fn documents() -> Result<Documents, ContractError> {
    let openapi = openapi_value()?;
    let stream_schema = stream_schema(&openapi)?;
    Ok(Documents {
        openapi,
        stream_schema,
        asyncapi: asyncapi(),
    })
}

/// Every `$ref` target name `#/components/schemas/<name>` inside `value`.
fn component_refs(value: &Value, out: &mut BTreeSet<String>) {
    match value {
        Value::Object(map) => {
            for (key, inner) in map {
                match (key.as_str(), inner) {
                    ("$ref", Value::String(target)) => {
                        if let Some(name) = target.strip_prefix(COMPONENTS_PREFIX) {
                            out.insert(name.to_string());
                        }
                    }
                    _ => component_refs(inner, out),
                }
            }
        }
        Value::Array(list) => list.iter().for_each(|v| component_refs(v, out)),
        _ => {}
    }
}

/// `value` with every `#/components/schemas/` reference pointing at `#/$defs/` (ADR 0025).
fn rewrite_refs(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, inner) in map.iter_mut() {
                match inner {
                    Value::String(target) if key == "$ref" => {
                        if let Some(name) = target.strip_prefix(COMPONENTS_PREFIX) {
                            *target = format!("{DEFS_PREFIX}{name}");
                        }
                    }
                    _ => rewrite_refs(inner),
                }
            }
        }
        Value::Array(list) => list.iter_mut().for_each(rewrite_refs),
        _ => {}
    }
}

/// The stream JSON Schema (2020-12): the root is the `StreamMessage` component and `$defs` holds
/// every component it reaches, with the references rewritten from `#/components/schemas/` to
/// `#/$defs/`. The schemas come from the same `utoipa` output as the `OpenAPI` document, so the two
/// cannot disagree (ADR 0025 option 1).
///
/// # Errors
/// The root or a component it reaches is not in `openapi`.
pub fn stream_schema(openapi: &Value) -> Result<Value, ContractError> {
    let empty = Map::new();
    let components = openapi["components"]["schemas"]
        .as_object()
        .unwrap_or(&empty);
    // Follow the references from the root, not the whole component list: the schema holds the
    // stream types only.
    let mut reached = BTreeSet::new();
    let mut todo = BTreeSet::from([STREAM_ROOT.to_string()]);
    while let Some(name) = todo.pop_first() {
        if !reached.insert(name.clone()) {
            continue;
        }
        let schema = components
            .get(&name)
            .ok_or_else(|| ContractError::MissingSchema(name.clone()))?;
        let mut inner = BTreeSet::new();
        component_refs(schema, &mut inner);
        todo.extend(inner);
    }
    let mut defs = Map::new();
    for name in reached.iter().filter(|name| *name != STREAM_ROOT) {
        if let Some(schema) = components.get(name) {
            defs.insert(name.clone(), schema.clone());
        }
    }
    let mut root = components
        .get(STREAM_ROOT)
        .cloned()
        .ok_or_else(|| ContractError::MissingSchema(STREAM_ROOT.to_string()))?;
    let Value::Object(map) = &mut root else {
        return Err(ContractError::NotAnObject(STREAM_ROOT.to_string()));
    };
    map.insert("$schema".to_string(), json!(DIALECT));
    map.insert("$id".to_string(), json!(STREAM_SCHEMA_ID));
    map.insert("title".to_string(), json!(STREAM_ROOT));
    map.insert("$defs".to_string(), Value::Object(defs));
    rewrite_refs(&mut root);
    Ok(root)
}

/// The `AsyncAPI` 3.0 description of the stream channel. Its message payloads reference the stream
/// schema file.
#[must_use]
pub fn asyncapi() -> Value {
    let message = |name: &str, summary: &str| {
        json!({
            "name": name,
            "summary": summary,
            "contentType": "application/json",
            "payload": {"$ref": format!("{STREAM_SCHEMA_FILE}#/$defs/{name}")},
        })
    };
    json!({
        "asyncapi": "3.0.0",
        "info": {
            "title": "Umbra control stream",
            "version": API_VERSION,
            "description": "The WebSocket stream of the Umbra control API. JSON text frames; the \
                            connection parameters (rate, encoding, filters) are the query of the \
                            upgrade request and `subscribe` messages (ADR 0027). No authentication \
                            in v1: the server binds to localhost by default.",
        },
        "servers": {
            "local": {
                "host": "127.0.0.1",
                "protocol": "ws",
                "description": "The default bind address; the port is the `[server]` setting.",
            },
        },
        "channels": {
            "stream": {
                "address": format!("{API_BASE}/stream"),
                "messages": {
                    "ClientMessage": message("ClientMessage", "Client to server: subscription changes."),
                    "ServerMessage": message("ServerMessage", "Server to client: hello, frames and errors."),
                },
            },
        },
        "operations": {
            "sendClientMessage": {
                "action": "receive",
                "summary": "The server receives client messages.",
                "channel": {"$ref": "#/channels/stream"},
                "messages": [{"$ref": "#/channels/stream/messages/ClientMessage"}],
            },
            "receiveServerMessage": {
                "action": "send",
                "summary": "The server sends hello, frames and errors.",
                "channel": {"$ref": "#/channels/stream"},
                "messages": [{"$ref": "#/channels/stream/messages/ServerMessage"}],
            },
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrite_changes_only_component_references() {
        let mut value = json!({
            "a": {"$ref": "#/components/schemas/X"},
            "b": [{"$ref": "#/$defs/Y"}, {"$ref": "other.json#/z"}],
            "properties": {"$ref": {"type": "string"}},
            "c": {"$ref": {"x": {"$ref": "#/components/schemas/Z"}}},
        });
        rewrite_refs(&mut value);
        assert_eq!(value["a"]["$ref"], "#/$defs/X");
        assert_eq!(value["b"][0]["$ref"], "#/$defs/Y");
        assert_eq!(value["b"][1]["$ref"], "other.json#/z");
        assert_eq!(value["properties"]["$ref"], json!({"type": "string"}));
        assert_eq!(value["c"]["$ref"]["x"]["$ref"], "#/$defs/Z");
    }

    #[test]
    fn stream_schema_has_the_root_and_only_stream_definitions() {
        let schema = stream_schema(&openapi_value().expect("document")).expect("schema");
        assert_eq!(schema["$id"], STREAM_SCHEMA_ID);
        assert_eq!(schema["$schema"], DIALECT);
        let defs = schema["$defs"]
            .as_object()
            .map(|m| m.keys().cloned().collect::<Vec<_>>());
        let defs = defs.unwrap_or_default();
        for name in [
            "ClientMessage",
            "ServerMessage",
            "Frame",
            "Hello",
            "StreamError",
        ] {
            assert!(defs.iter().any(|d| d == name), "{name} in {defs:?}");
        }
        assert!(!defs.iter().any(|d| d == "CatalogEntry"), "{defs:?}");
        assert!(!render(&schema).expect("render").contains("#/components/"));
    }

    #[test]
    fn a_missing_component_is_an_error_not_a_gap() {
        let doc = |schemas: Value| json!({"components": {"schemas": schemas}});
        let root = json!({"oneOf": [{"$ref": "#/components/schemas/ClientMessage"}]});
        // The root references a component that is not there.
        match stream_schema(&doc(json!({"StreamMessage": root.clone()}))) {
            Err(ContractError::MissingSchema(name)) => assert_eq!(name, "ClientMessage"),
            other => panic!("expected MissingSchema(ClientMessage), got {other:?}"),
        }
        // The root itself is missing.
        match stream_schema(&doc(json!({"ClientMessage": {"type": "object"}}))) {
            Err(ContractError::MissingSchema(name)) => assert_eq!(name, "StreamMessage"),
            other => panic!("expected MissingSchema(StreamMessage), got {other:?}"),
        }
        // Complete: ok.
        let ok = stream_schema(&doc(
            json!({"StreamMessage": root, "ClientMessage": {"type": "object"}}),
        ));
        assert!(ok.is_ok(), "{ok:?}");
    }

    #[test]
    fn render_ends_with_one_newline_and_has_sorted_keys() {
        let text = render(&json!({"b": 1, "a": 2})).expect("render");
        assert_eq!(text, "{\n  \"a\": 2,\n  \"b\": 1\n}\n");
    }
}
