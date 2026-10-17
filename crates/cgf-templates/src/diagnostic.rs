//! Located diagnostics (REQ-T-2, ASM-056).

use core::fmt;
use std::ops::Range;
use std::path::{Path, PathBuf};

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
    /// A node name that no `[nodes.*]` table defines (behavior graphs, ADR 0012).
    UnknownNode,
    /// A node with two parents.
    NodeReused,
    /// Nodes reference each other in a loop.
    NodeCycle,
    /// A node not reachable from `root`.
    UnreachableNode,
    /// A `version` or version range that is not semver.
    InvalidVersion,
    /// A subgraph's version does not satisfy the range.
    VersionMismatch,
    /// Graphs call each other in a loop.
    SubgraphCycle,
    /// Subgraphs nest deeper than the limit.
    SubgraphTooDeep,
    /// An FSM without a valid `initial` state.
    NoInitialState,
    /// An FSM state that cannot be reached from `initial`.
    UnreachableState,
    /// An invalid `{ key = .. }` binding, output binding or `{ param = .. }` reference.
    InvalidBinding,
    /// A guard that is not a condition tree.
    InvalidGuard,
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
            Self::UnknownNode => "unknown_node",
            Self::NodeReused => "node_reused",
            Self::NodeCycle => "node_cycle",
            Self::UnreachableNode => "unreachable_node",
            Self::InvalidVersion => "invalid_version",
            Self::VersionMismatch => "version_mismatch",
            Self::SubgraphCycle => "subgraph_cycle",
            Self::SubgraphTooDeep => "subgraph_too_deep",
            Self::NoInitialState => "no_initial_state",
            Self::UnreachableState => "unreachable_state",
            Self::InvalidBinding => "invalid_binding",
            Self::InvalidGuard => "invalid_guard",
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

impl Diagnostic {
    /// A diagnostic without notes.
    pub fn new(
        code: DiagnosticCode,
        message: impl Into<String>,
        location: Option<Location>,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            location,
            notes: Vec::new(),
        }
    }

    /// Adds a note.
    #[must_use]
    pub fn with_note(mut self, message: impl Into<String>, location: Option<Location>) -> Self {
        self.notes.push(Note {
            message: message.into(),
            location,
        });
        self
    }

    /// Sort key: (path, span start, code, message, span end). Unlocated diagnostics sort first.
    fn sort_key(&self) -> (Option<&Path>, usize, DiagnosticCode, &str, usize) {
        let (path, start, end) = self.location.as_ref().map_or((None, 0, 0), |l| {
            (Some(l.path.as_path()), l.span.start, l.span.end)
        });
        (path, start, self.code, self.message.as_str(), end)
    }
}

impl fmt::Display for Location {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}:{}", self.path.display(), self.line, self.column)
    }
}

impl fmt::Display for Diagnostic {
    /// `path:line:col: error[code]: message`, then one `  note: message (path:line:col)` line per
    /// note.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(location) = &self.location {
            write!(f, "{location}: ")?;
        }
        write!(f, "error[{}]: {}", self.code.as_str(), self.message)?;
        for note in &self.notes {
            write!(f, "\n  note: {}", note.message)?;
            if let Some(location) = &note.location {
                write!(f, " ({location})")?;
            }
        }
        Ok(())
    }
}

/// Sorts by (path, span start, code, message) and removes duplicates (same code, location and
/// message), so output is stable (REQ-K-4, ASM-056).
pub fn sort_and_dedup(diagnostics: &mut Vec<Diagnostic>) {
    diagnostics.sort_by(|a, b| a.sort_key().cmp(&b.sort_key()));
    diagnostics
        .dedup_by(|b, a| a.code == b.code && a.location == b.location && a.message == b.message);
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
    /// Sorts and de-duplicates `diagnostics`; `None` when there are none.
    pub fn new(mut diagnostics: Vec<Diagnostic>) -> Option<Self> {
        sort_and_dedup(&mut diagnostics);
        (!diagnostics.is_empty()).then_some(Self(diagnostics))
    }

    /// A list of exactly one diagnostic.
    pub(crate) fn from_one(diagnostic: Diagnostic) -> Self {
        Self(vec![diagnostic])
    }

    /// The diagnostics, consuming `self`.
    pub fn into_vec(self) -> Vec<Diagnostic> {
        self.0
    }

    /// The diagnostics in order.
    pub fn iter(&self) -> impl Iterator<Item = &Diagnostic> {
        self.0.iter()
    }

    /// How many there are.
    pub fn len(&self) -> usize {
        self.0.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(path: &str, line: u32, column: u32, start: usize) -> Location {
        Location {
            path: path.into(),
            line,
            column,
            span: start..start + 1,
        }
    }

    fn d(code: DiagnosticCode, message: &str, location: Location) -> Diagnostic {
        Diagnostic::new(code, message, Some(location))
    }

    #[test]
    fn display_is_path_line_col_error_code_message_then_notes() {
        let diagnostic = d(
            DiagnosticCode::UnknownKey,
            "no such key",
            at("a.toml", 3, 7, 20),
        )
        .with_note("defined here", Some(at("b.toml", 1, 2, 1)))
        .with_note("plain", None);

        assert_eq!(
            diagnostic.to_string(),
            "a.toml:3:7: error[unknown_key]: no such key\n  note: defined here (b.toml:1:2)\n  note: plain"
        );
    }

    #[test]
    fn display_without_location_omits_the_prefix() {
        let diagnostic = Diagnostic::new(DiagnosticCode::Io, "cannot read x", None);

        assert_eq!(diagnostic.to_string(), "error[io]: cannot read x");
    }

    #[test]
    fn diagnostics_sort_by_path_then_offset_then_code_then_message() {
        let list = vec![
            d(DiagnosticCode::Syntax, "z", at("b.toml", 1, 1, 0)),
            d(DiagnosticCode::Syntax, "b", at("a.toml", 2, 1, 10)),
            d(DiagnosticCode::WrongType, "a", at("a.toml", 1, 1, 0)),
            d(DiagnosticCode::Syntax, "m", at("a.toml", 1, 1, 0)),
            d(DiagnosticCode::Syntax, "a", at("a.toml", 1, 1, 0)),
        ];

        let sorted = Diagnostics::new(list)
            .map(Diagnostics::into_vec)
            .unwrap_or_default();

        let order: Vec<&str> = sorted.iter().map(|x| x.message.as_str()).collect();
        assert_eq!(order, ["a", "m", "a", "b", "z"]);
        assert_eq!(sorted[2].code, DiagnosticCode::WrongType);
    }

    #[test]
    fn duplicates_with_equal_code_location_and_message_are_removed() {
        let one = d(DiagnosticCode::Syntax, "m", at("a.toml", 1, 1, 0));
        let other = d(DiagnosticCode::Syntax, "m", at("a.toml", 1, 1, 0));
        let different = d(DiagnosticCode::Syntax, "n", at("a.toml", 1, 1, 0));

        let all = Diagnostics::new(vec![one, different, other]);

        assert_eq!(all.map(|d| d.len()), Some(2));
    }

    #[test]
    fn no_diagnostics_is_none() {
        assert_eq!(Diagnostics::new(Vec::new()), None);
    }
}
