//! STUB (WI-5): the JSON Schema subset validator (ADR 0009).
#![expect(clippy::todo, reason = "stub until WI-5: signatures only")]

use cgf_core::ParamSchema;

use crate::diagnostic::Diagnostic;
use crate::value::LValue;

/// A schema compiled against the ADR 0009 subset.
#[derive(Debug, Clone)]
pub struct CompiledSchema {
    _opaque: (),
}

/// A schema outside the subset or invalid.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("unsupported or invalid JSON Schema at {pointer}: {message}")]
pub struct SchemaError {
    /// JSON pointer of the offending keyword.
    pub pointer: String,
    /// What is wrong.
    pub message: String,
}

impl CompiledSchema {
    /// Compiles `schema`, failing closed on any keyword outside the subset.
    pub fn compile(_schema: &ParamSchema) -> Result<Self, SchemaError> {
        todo!("WI-5")
    }

    /// Every violation of `value`, located at the offending value (or its object for a missing key).
    pub fn validate(&self, _value: &LValue, _context: &str) -> Vec<Diagnostic> {
        todo!("WI-5")
    }
}
