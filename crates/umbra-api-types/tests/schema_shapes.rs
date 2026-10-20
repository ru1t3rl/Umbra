//! Baseline of the generated `OpenAPI` schema shapes (utoipa 6) for the WI-12 `gen-api` `$defs`
//! rewrite, and the check that every DTO is a component (review m6). The expectations were captured
//! from the generator's output and reviewed once. An additive field changes a baseline here, so
//! the expectation must be updated deliberately in the same change; descriptions are stripped so doc edits do not
//! move them.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

use serde_json::{Value, json};
use umbra_api_types::ApiSchemas;
use utoipa::OpenApi;

fn document() -> Value {
    serde_json::to_value(ApiSchemas::openapi()).expect("serialise the document")
}

/// Removes every `description` and `title` (schema keywords, not properties named so).
fn strip_prose(value: &mut Value) {
    match value {
        Value::Object(map) => {
            map.remove("description");
            map.remove("title");
            for (key, child) in map.iter_mut() {
                if key == "properties" {
                    // Property names are free; strip inside each property schema only.
                    if let Value::Object(props) = child {
                        props.values_mut().for_each(strip_prose);
                    }
                } else {
                    strip_prose(child);
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(strip_prose),
        _ => {}
    }
}

fn shape(name: &str) -> Value {
    let mut schema = document()["components"]["schemas"][name].clone();
    assert!(schema.is_object(), "{name} is a component");
    strip_prose(&mut schema);
    schema
}

#[test]
fn the_document_is_openapi_3_1_0() {
    assert_eq!(document()["openapi"], "3.1.0");
}

#[test]
fn an_optional_field_is_nullable_and_not_required() {
    // `RunStatus.tick`: omitted when absent.
    let status = shape("RunStatus");
    assert_eq!(
        status["properties"]["tick"],
        json!({"format": "int64", "minimum": 0, "type": ["integer", "null"]})
    );
    assert_eq!(
        status["required"],
        json!(["state", "clock", "session", "step_pending"])
    );
    assert_eq!(
        status["properties"]["stop_reason"],
        json!({"oneOf": [{"$ref": "#/components/schemas/StopReason"}, {"type": "null"}]})
    );
}

#[test]
fn a_nullable_required_field_is_listed_as_required() {
    // `EntityPage.next_after_id`: always present, null on the last page.
    assert_eq!(
        shape("EntityPage"),
        json!({
            "properties": {
                "items": {"items": {"$ref": "#/components/schemas/EntitySummary"}, "type": "array"},
                "next_after_id": {"format": "int64", "minimum": 0, "type": ["integer", "null"]},
                "tick": {"format": "int64", "minimum": 0, "type": "integer"},
                "total": {"format": "int64", "minimum": 0, "type": "integer"}
            },
            "required": ["tick", "total", "items", "next_after_id"],
            "type": "object"
        })
    );
}

#[test]
fn an_internally_tagged_enum_is_a_one_of_with_the_tag_in_every_branch() {
    let branch = |mode: &str| {
        json!({
            "additionalProperties": false,
            "properties": {"mode": {"enum": [mode], "type": "string"}},
            "required": ["mode"],
            "type": "object"
        })
    };
    assert_eq!(
        shape("ClockSetting"),
        json!({
            "oneOf": [
                branch("afap"),
                branch("realtime"),
                branch("stepped"),
                {
                    "additionalProperties": false,
                    "properties": {
                        "mode": {"enum": ["scaled"], "type": "string"},
                        "scale": {"maximum": 1_000_000, "minimum": 0.001, "type": "number"}
                    },
                    "required": ["mode", "scale"],
                    "type": "object"
                }
            ]
        })
    );
    // A newtype variant over a struct combines the struct with the tag.
    assert_eq!(
        shape("ClientMessage"),
        json!({
            "oneOf": [{
                "allOf": [
                    {"$ref": "#/components/schemas/Subscription"},
                    {"properties": {"type": {"enum": ["subscribe"], "type": "string"}}, "required": ["type"], "type": "object"}
                ]
            }]
        })
    );
}

#[test]
fn seed_is_a_one_of_string_or_safe_integer() {
    assert_eq!(
        shape("Seed"),
        json!({
            "oneOf": [
                {"pattern": "^[0-9]{1,20}$", "type": "string"},
                {"maximum": 9_007_199_254_740_991_u64, "minimum": 0, "type": "integer"}
            ]
        })
    );
}

#[test]
fn the_stream_root_references_both_directions() {
    assert_eq!(
        shape("StreamMessage"),
        json!({"oneOf": [
            {"$ref": "#/components/schemas/ClientMessage"},
            {"$ref": "#/components/schemas/ServerMessage"}
        ]})
    );
}

#[test]
fn every_dto_is_a_component() {
    let doc = document();
    let schemas = doc["components"]["schemas"].as_object().expect("schemas");
    let listed: Vec<&str> = schemas.keys().map(String::as_str).collect();
    // Every `$ref` target resolves, so no DTO a schema uses is missing.
    let mut refs = Vec::new();
    collect_refs(&doc["components"], &mut refs);
    assert_ne!(refs.len(), 0, "the schemas reference each other");
    for target in refs {
        assert!(
            listed.contains(&target.as_str()),
            "{target} is referenced but not a component"
        );
    }
    for expected in [
        "ClientMessage",
        "ServerMessage",
        "Frame",
        "EventDto",
        "Hello",
        "StreamError",
        "StreamFilter",
        "GeoBox",
        "Subscription",
        "StreamMessage",
        "Problem",
        "ProblemCode",
        "EngineStatus",
        "ScenarioInfo",
        "RunStatus",
        "RunState",
        "StopReason",
        "ClockSetting",
        "LoadScenarioRequest",
        "StepRequest",
        "StepResult",
        "EntitySummary",
        "EntityDetail",
        "EntityPage",
        "SpawnRequest",
        "TemplateSummary",
        "TemplateDetail",
        "ComponentEntry",
        "BehaviorCatalog",
        "ValidateGraphRequest",
        "ValidationResult",
        "PluginReport",
        "PluginDto",
        "CapabilityDto",
        "PluginFaultDto",
        "Seed",
    ] {
        assert!(listed.contains(&expected), "{expected} is not a component");
    }
}

fn collect_refs(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                match (key.as_str(), child) {
                    ("$ref", Value::String(target)) => {
                        out.push(
                            target
                                .trim_start_matches("#/components/schemas/")
                                .to_string(),
                        );
                    }
                    _ => collect_refs(child, out),
                }
            }
        }
        Value::Array(items) => items.iter().for_each(|i| collect_refs(i, out)),
        _ => {}
    }
}

#[test]
fn a_request_refuses_extra_properties_in_its_schema() {
    // `deny_unknown_fields` shows as `additionalProperties: false`.
    let request = shape("LoadScenarioRequest");
    assert_eq!(request["additionalProperties"], json!(false));
    assert_eq!(request["required"], json!(["path"]));
    assert_eq!(
        request["properties"]["seed"],
        json!({"oneOf": [{"$ref": "#/components/schemas/Seed"}, {"type": "null"}]})
    );
}

#[test]
fn server_message_is_a_tagged_one_of_over_the_three_messages() {
    let message = shape("ServerMessage");
    let branches = message["oneOf"].as_array().expect("oneOf");
    assert_eq!(branches.len(), 3);
    for (branch, (name, tag)) in branches.iter().zip([
        ("Hello", "hello"),
        ("Frame", "frame"),
        ("StreamError", "error"),
    ]) {
        assert_eq!(
            *branch,
            json!({"allOf": [
                {"$ref": format!("#/components/schemas/{name}")},
                {"properties": {"type": {"enum": [tag], "type": "string"}}, "required": ["type"], "type": "object"}
            ]})
        );
    }
}
