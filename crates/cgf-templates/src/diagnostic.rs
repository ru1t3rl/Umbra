//! Located diagnostics (REQ-T-2, ASM-056). Partly STUB (WI-4): `Display` is not implemented yet.
#![expect(clippy::todo, reason = "stub until WI-4: Display and sorting")]

use core::fmt;
use std::ops::Range;
use std::path::PathBuf;

/// Where a diagnostic points. 1-based line and column (Unicode scalar values).
///
/// The plan derived `PartialOrd`/`Ord` here, but `Range<usize>` has neither: sort by a key tuple.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    /// The file.
    pub path: PathBuf,
    /// 1-based line.
    pub line: u32,
    /// 1-based column in Unicode scalar values.
    pub column: u32,
    /// Byte range in the file.
    pub span: Range<usize>,
}

/// Stable diagnostic codes (ASM-056).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum DiagnosticCode {
    /// A file could not be read.
    Io,
    /// TOML syntax error.
    Syntax,
    /// A datetime or a non-finite float.
    UnsupportedValue,
    /// A key the format does not define.
    UnknownKey,
    /// A required key is missing.
    MissingKey,
    /// A value of the wrong TOML type.
    WrongType,
    /// A value out of range or otherwise invalid.
    InvalidValue,
    /// An ID that breaks the ADR 0002 grammar.
    InvalidId,
    /// `schema_version` is newer than supported.
    UnsupportedSchemaVersion,
    /// Two templates with one ID.
    DuplicateId,
    /// `extends` or `template` names an undefined template.
    UnknownTemplate,
    /// A component ID that is not registered.
    UnknownId,
    /// Templates extend each other in a loop.
    InheritanceCycle,
    /// A required component parameter is missing.
    MissingParam,
    /// A component parameter has the wrong type.
    ParamWrongType,
    /// A component parameter is out of its range.
    ParamOutOfRange,
    /// A component parameter the schema does not define.
    UnknownParam,
    /// A parameter combination the factory rejected, or another schema violation.
    ParamInvalid,
    /// Two entities with one name.
    DuplicateName,
    /// A file given as a scenario is not one.
    NotAScenario,
}

impl DiagnosticCode {
    /// The stable `snake_case` name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Io => "io",
            Self::Syntax => "syntax",
            Self::UnsupportedValue => "unsupported_value",
            Self::UnknownKey => "unknown_key",
            Self::MissingKey => "missing_key",
            Self::WrongType => "wrong_type",
            Self::InvalidValue => "invalid_value",
            Self::InvalidId => "invalid_id",
            Self::UnsupportedSchemaVersion => "unsupported_schema_version",
            Self::DuplicateId => "duplicate_id",
            Self::UnknownTemplate => "unknown_template",
            Self::UnknownId => "unknown_id",
            Self::InheritanceCycle => "inheritance_cycle",
            Self::MissingParam => "missing_param",
            Self::ParamWrongType => "param_wrong_type",
            Self::ParamOutOfRange => "param_out_of_range",
            Self::UnknownParam => "unknown_param",
            Self::ParamInvalid => "param_invalid",
            Self::DuplicateName => "duplicate_name",
            Self::NotAScenario => "not_a_scenario",
        }
    }
}

/// An additional line of a diagnostic, optionally located.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    /// The note text.
    pub message: String,
    /// Where the note points.
    pub location: Option<Location>,
}

/// One located error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// The stable code.
    pub code: DiagnosticCode,
    /// The message.
    pub message: String,
    /// Where; `None` only for an `io` error on a path.
    pub location: Option<Location>,
    /// Extra lines.
    pub notes: Vec<Note>,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!("WI-4")
    }
}

/// A non-empty list of diagnostics, sorted by (path, span start, code, message) without duplicates.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{} error(s)", .0.len())]
pub struct Diagnostics(Vec<Diagnostic>);

#[expect(
    clippy::len_without_is_empty,
    reason = "a Diagnostics value is never empty by construction"
)]
impl Diagnostics {
    /// The diagnostics in order.
    pub fn iter(&self) -> impl Iterator<Item = &Diagnostic> {
        self.0.iter()
    }

    /// How many there are.
    pub fn len(&self) -> usize {
        self.0.len()
    }
}
