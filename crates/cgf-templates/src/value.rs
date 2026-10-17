//! STUB (WI-4): the located TOML tree.
//!
//! Tests only need `parse` and `LTable::get`; the plan's `LValue { kind, origin }` is opaque here.
#![expect(clippy::todo, reason = "stub until WI-4: signatures only")]

use std::collections::BTreeMap;

use crate::diagnostic::Diagnostic;
use crate::source::SourceFile;

/// A TOML value with the origin (file and span) of itself and everything below it.
#[derive(Debug, Clone, PartialEq)]
pub struct LValue {
    _span: std::ops::Range<usize>,
}

/// A TOML table with located values.
#[derive(Debug, Clone, PartialEq)]
pub struct LTable {
    entries: BTreeMap<String, LValue>,
}

impl LTable {
    /// The value under `key`.
    pub fn get(&self, key: &str) -> Option<&LValue> {
        self.entries.get(key)
    }

    /// The whole table as one value (to validate a document root against a schema).
    pub fn into_value(self) -> LValue {
        todo!("WI-4")
    }
}

/// Parses a file into the located tree. Returns every syntax error (ASM-054); the table is `None`
/// when the file cannot be read as a table at all.
pub fn parse(_file: &SourceFile) -> (Option<LTable>, Vec<Diagnostic>) {
    todo!("WI-4")
}
