//! Component parameters and their schemas (REQ-T-3, ADR 0008).
//!
//! Parameters are JSON values ([`ParamValue`]); the JSON object map is a `BTreeMap`, so key order
//! is deterministic. Schemas are hand-written JSON Schema documents shipped by each component.

use std::sync::Arc;

/// A component parameter value (the JSON form that also crosses the plugin ABI, REQ-P-6).
pub use serde_json::Value as ParamValue;

/// A JSON Schema document with an object at the root.
///
/// Whether it stays inside the supported keyword subset (ADR 0009) is checked by
/// `cgf-templates`, not here. Cloning shares the document.
#[derive(Debug, Clone, PartialEq)]
pub struct ParamSchema(Arc<ParamValue>);

/// Why a string was rejected as a [`ParamSchema`].
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum ParamSchemaError {
    /// The text is not valid JSON.
    #[error("schema is not valid JSON: {0}")]
    Json(String),
    /// The JSON root is not an object.
    #[error("schema root must be a JSON object")]
    NotAnObject,
}

impl ParamSchema {
    /// Parses a schema document.
    ///
    /// # Errors
    /// [`ParamSchemaError::Json`] for invalid JSON, [`ParamSchemaError::NotAnObject`] when the
    /// root is not an object.
    pub fn from_json_str(s: &str) -> Result<Self, ParamSchemaError> {
        let value: ParamValue =
            serde_json::from_str(s).map_err(|e| ParamSchemaError::Json(e.to_string()))?;
        if value.is_object() {
            Ok(Self(Arc::new(value)))
        } else {
            Err(ParamSchemaError::NotAnObject)
        }
    }

    /// The schema document.
    pub fn as_json(&self) -> &ParamValue {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_object_root() {
        let s = ParamSchema::from_json_str(r#"{"type":"object","examples":[{}]}"#)
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(s.as_json()["type"], "object");
        assert_eq!(s.clone(), s);
    }

    #[test]
    fn rejects_invalid_json() {
        assert!(matches!(
            ParamSchema::from_json_str("{"),
            Err(ParamSchemaError::Json(_))
        ));
    }

    #[test]
    fn rejects_non_object_roots() {
        for text in ["[]", "true", "1", "null", "\"x\""] {
            assert_eq!(
                ParamSchema::from_json_str(text),
                Err(ParamSchemaError::NotAnObject),
                "{text}"
            );
        }
    }

    #[test]
    fn object_keys_iterate_sorted() {
        // Guards against `serde_json/preserve_order` being unified in (determinism).
        let v: ParamValue = serde_json::json!({"b": 1, "a": 2});
        let keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(keys, ["a", "b"]);
    }
}
