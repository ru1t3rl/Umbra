//! Typed access to the located tree that records errors instead of stopping at the first
//! (REQ-T-2). Typed getters plus unknown-key detection; no serde (ADR 0008, ASM-054).

use umbra_core::IdError;

use crate::diagnostic::{Diagnostic, DiagnosticCode, Location};
use crate::source::SourceFile;
use crate::value::{LKind, LTable, LValue, Origin};

/// Collects the diagnostics of one file while it is read.
pub(crate) struct Reader<'f> {
    file: &'f SourceFile,
    out: Vec<Diagnostic>,
}

impl<'f> Reader<'f> {
    /// A reader for `file` with no diagnostics yet.
    pub(crate) fn new(file: &'f SourceFile) -> Self {
        Self {
            file,
            out: Vec::new(),
        }
    }

    /// The diagnostics collected so far, in the order they were found.
    pub(crate) fn into_diagnostics(self) -> Vec<Diagnostic> {
        self.out
    }

    /// Records diagnostics found elsewhere (a component check).
    pub(crate) fn extend(&mut self, diagnostics: Vec<Diagnostic>) {
        self.out.extend(diagnostics);
    }

    /// Records a finished diagnostic.
    pub(crate) fn push_diagnostic(&mut self, diagnostic: Diagnostic) {
        self.out.push(diagnostic);
    }

    /// Records an error at `at`.
    pub(crate) fn error(&mut self, code: DiagnosticCode, message: String, at: &Origin) {
        self.out
            .push(Diagnostic::new(code, message, Some(at.location())));
    }

    /// `value` as a table, or `wrong_type`.
    pub(crate) fn table<'t>(&mut self, name: &str, value: &'t LValue) -> Option<&'t LTable> {
        match value.kind() {
            LKind::Table(table) => Some(table),
            other => {
                self.wrong_type(name, "a table", other, value.origin());
                None
            }
        }
    }

    /// `value` as a string, or `wrong_type`.
    pub(crate) fn string<'t>(&mut self, name: &str, value: &'t LValue) -> Option<&'t str> {
        match value.kind() {
            LKind::String(s) => Some(s),
            other => {
                self.wrong_type(name, "a string", other, value.origin());
                None
            }
        }
    }

    /// `value` as a number (integer or float), or `wrong_type`.
    pub(crate) fn number(&mut self, name: &str, value: &LValue) -> Option<f64> {
        match value.kind() {
            #[expect(
                clippy::cast_precision_loss,
                reason = "TOML integers become f64 parameters"
            )]
            LKind::Integer(i) => Some(*i as f64),
            LKind::Float(f) => Some(*f),
            other => {
                self.wrong_type(name, "a number", other, value.origin());
                None
            }
        }
    }

    /// `value` as a boolean, or `wrong_type`.
    pub(crate) fn boolean(&mut self, name: &str, value: &LValue) -> Option<bool> {
        match value.kind() {
            LKind::Boolean(b) => Some(*b),
            other => {
                self.wrong_type(name, "a boolean", other, value.origin());
                None
            }
        }
    }

    /// `value` as an integer, or `wrong_type`.
    pub(crate) fn integer(&mut self, name: &str, value: &LValue) -> Option<i64> {
        match value.kind() {
            LKind::Integer(i) => Some(*i),
            other => {
                self.wrong_type(name, "an integer", other, value.origin());
                None
            }
        }
    }

    /// `value` as an array, or `wrong_type`.
    pub(crate) fn array<'t>(&mut self, name: &str, value: &'t LValue) -> Option<&'t [LValue]> {
        match value.kind() {
            LKind::Array(items) => Some(items),
            other => {
                self.wrong_type(name, "an array", other, value.origin());
                None
            }
        }
    }

    /// `value` as a string ID built by `make` (an ADR 0002 grammar check), or `wrong_type` or
    /// `invalid_id`, the latter at the offending character where it can be found.
    pub(crate) fn id<T>(
        &mut self,
        name: &str,
        value: &LValue,
        make: impl FnOnce(&str) -> Result<T, IdError>,
    ) -> Option<T> {
        let text = self.string(name, value)?;
        match make(text) {
            Ok(id) => Some(id),
            Err(error) => {
                let location = self.id_location(value, text, &error);
                let message = format!("`{name}` is not a valid ID: {error}");
                self.out.push(Diagnostic::new(
                    DiagnosticCode::InvalidId,
                    message,
                    Some(location),
                ));
                None
            }
        }
    }

    /// Reports every key of `table` that is not in `known` as `unknown_key`, at the key.
    pub(crate) fn unknown_keys(&mut self, table: &LTable, known: &[&str]) {
        for (key, entry) in table.iter() {
            if !known.contains(&key) {
                let message = format!(
                    "unknown key `{key}` (expected one of: {})",
                    known.join(", ")
                );
                self.error(DiagnosticCode::UnknownKey, message, entry.key_origin());
            }
        }
    }

    fn wrong_type(&mut self, name: &str, wanted: &str, found: &LKind, at: &Origin) {
        if matches!(found, LKind::Unsupported) {
            return; // already reported as `unsupported_value`
        }
        let message = format!("`{name}` must be {wanted}, found {}", found.type_name());
        self.error(DiagnosticCode::WrongType, message, at);
    }

    /// The offending character inside a plain quoted string, else the start of the value.
    fn id_location(&self, value: &LValue, text: &str, error: &IdError) -> Location {
        let origin = value.origin();
        let index = match error {
            IdError::EmptySegment { index, .. }
            | IdError::InvalidChar { index, .. }
            | IdError::InvalidSegmentStart { index, .. } => Some(*index),
            _ => None,
        };
        let Some(index) = index else {
            return origin.location();
        };
        let span = origin.span();
        let source = self.file.text();
        let inner = span
            .start
            .checked_add(1)
            .zip(span.end.checked_sub(1))
            .and_then(|(from, to)| source.get(from..to));
        let quoted = source
            .as_bytes()
            .get(span.start)
            .is_some_and(|&b| b == b'"' || b == b'\'');
        if !(quoted && inner == Some(text)) {
            return origin.location();
        }
        let byte = span.start + 1 + index;
        let (line, column) = self.file.line_col(byte);
        Location {
            path: self.file.path().to_path_buf(),
            line,
            column,
            span: byte..byte + 1,
        }
    }
}
