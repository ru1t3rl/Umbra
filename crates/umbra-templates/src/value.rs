//! The located TOML tree (REQ-T-2, ASM-054, ASM-056, ASM-060).
//!
//! Built from `toml::de::DeTable::parse_recoverable`: every key and value keeps its file, byte
//! span, line and column, so a later check can point at the exact place. Datetimes and
//! non-finite floats are reported as `unsupported_value` (they cannot cross as JSON, REQ-P-6).

use std::collections::BTreeMap;
use std::ops::Range;
use std::path::Path;
use std::sync::Arc;

use toml::Spanned;
use toml::de::{DeTable, DeValue};
use umbra_core::ParamValue;

use crate::diagnostic::{Diagnostic, DiagnosticCode, Location};
use crate::source::SourceFile;

/// Where a key or value is: file, byte span, 1-based line and column.
#[derive(Debug, Clone, PartialEq)]
pub struct Origin {
    path: Arc<Path>,
    span: Range<usize>,
    line: u32,
    column: u32,
}

impl Origin {
    fn new(file: &SourceFile, path: &Arc<Path>, span: Range<usize>) -> Self {
        let (line, column) = file.line_col(span.start);
        Self {
            path: Arc::clone(path),
            span,
            line,
            column,
        }
    }

    /// The file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The byte range in the file.
    pub fn span(&self) -> Range<usize> {
        self.span.clone()
    }

    /// The same place as a diagnostic location.
    pub fn location(&self) -> Location {
        Location {
            path: self.path.to_path_buf(),
            line: self.line,
            column: self.column,
            span: self.span.clone(),
        }
    }
}

/// The payload of a located value.
#[derive(Debug, Clone, PartialEq)]
pub enum LKind {
    /// A string.
    String(String),
    /// An integer.
    Integer(i64),
    /// A float.
    Float(f64),
    /// A boolean.
    Boolean(bool),
    /// An array.
    Array(Vec<LValue>),
    /// A table (inline or header).
    Table(LTable),
    /// A value the format cannot carry (a datetime, a non-finite float, an integer outside 64
    /// bits). `parse` has already reported it as `unsupported_value`; readers and schemas skip it
    /// silently so it causes no follow-on errors.
    Unsupported,
}

impl LKind {
    /// The TOML type name used in messages.
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::String(_) => "string",
            Self::Integer(_) => "integer",
            Self::Float(_) => "float",
            Self::Boolean(_) => "boolean",
            Self::Array(_) => "array",
            Self::Table(_) => "table",
            Self::Unsupported => "unsupported value",
        }
    }
}

/// A TOML value with the origin of itself and everything below it.
#[derive(Debug, Clone, PartialEq)]
pub struct LValue {
    origin: Origin,
    kind: LKind,
}

impl LValue {
    /// A value made by the loader itself (a default), located at `origin`.
    pub(crate) fn synthetic(origin: &Origin, kind: LKind) -> Self {
        Self {
            origin: origin.clone(),
            kind,
        }
    }

    /// Where the value starts.
    pub fn origin(&self) -> &Origin {
        &self.origin
    }

    /// The value.
    pub fn kind(&self) -> &LKind {
        &self.kind
    }

    /// The value as JSON, without losing the integer/float distinction. Unsupported values (which
    /// `parse` reports) become `null`.
    pub fn to_param(&self) -> ParamValue {
        match &self.kind {
            LKind::String(s) => ParamValue::String(s.clone()),
            LKind::Integer(i) => ParamValue::from(*i),
            LKind::Float(f) => {
                serde_json::Number::from_f64(*f).map_or(ParamValue::Null, ParamValue::Number)
            }
            LKind::Boolean(b) => ParamValue::Bool(*b),
            LKind::Array(items) => ParamValue::Array(items.iter().map(Self::to_param).collect()),
            LKind::Table(t) => t.to_param(),
            LKind::Unsupported => ParamValue::Null,
        }
    }
}

/// A key of a table with its origin.
#[derive(Debug, Clone, PartialEq)]
pub struct LEntry {
    key_origin: Origin,
    value: LValue,
}

impl LEntry {
    /// An entry made by the loader itself (a default value), located at `origin`.
    pub(crate) fn synthetic(origin: &Origin, value: LValue) -> Self {
        Self {
            key_origin: origin.clone(),
            value,
        }
    }

    /// Where the key is.
    pub fn key_origin(&self) -> &Origin {
        &self.key_origin
    }

    /// The value.
    pub fn value(&self) -> &LValue {
        &self.value
    }
}

/// A TOML table with located keys and values, in key order.
#[derive(Debug, Clone, PartialEq)]
pub struct LTable {
    origin: Origin,
    entries: BTreeMap<String, LEntry>,
}

impl LTable {
    /// Where the table starts (`{` of an inline table, `[` of a header table).
    pub fn origin(&self) -> &Origin {
        &self.origin
    }

    /// The value under `key`.
    pub fn get(&self, key: &str) -> Option<&LValue> {
        self.entries.get(key).map(LEntry::value)
    }

    /// The entry (key origin and value) under `key`.
    pub fn entry(&self, key: &str) -> Option<&LEntry> {
        self.entries.get(key)
    }

    /// All entries in key order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &LEntry)> {
        self.entries.iter().map(|(k, e)| (k.as_str(), e))
    }

    /// A table made of existing entries (used when merging inherited parameters).
    pub(crate) fn from_entries(origin: Origin, entries: BTreeMap<String, LEntry>) -> Self {
        Self { origin, entries }
    }

    /// The table as JSON.
    pub fn to_param(&self) -> ParamValue {
        ParamValue::Object(
            self.entries
                .iter()
                .map(|(k, e)| (k.clone(), e.value.to_param()))
                .collect(),
        )
    }

    /// The whole table as one value (to validate a document root against a schema).
    pub fn into_value(self) -> LValue {
        LValue {
            origin: self.origin.clone(),
            kind: LKind::Table(self),
        }
    }
}

/// Parses a file into the located tree. Returns every syntax error (ASM-054); the table is `None`
/// only when the file cannot be read as a table at all. With syntax errors the table is a partial
/// tree: use it only to recover a top-level `id` (ASM-054 rule 3).
pub fn parse(file: &SourceFile) -> (Option<LTable>, Vec<Diagnostic>) {
    let (document, errors) = DeTable::parse_recoverable(file.text());
    let path: Arc<Path> = Arc::from(file.path());
    let mut builder = Builder {
        file,
        path,
        unsupported: Vec::new(),
    };
    let root = builder.table(document.get_ref(), document.span());
    let mut diagnostics = syntax_diagnostics(file, &errors, &root);
    if diagnostics.is_empty()
        && let Some(first) = errors.first()
    {
        diagnostics.push(syntax_diagnostic(
            file,
            first.span(),
            first.message().to_string(),
        ));
    }
    if diagnostics.is_empty() {
        diagnostics = builder.unsupported;
    }
    diagnostics.sort_by_key(|d| d.location.as_ref().map(|l| l.span.start));
    (Some(root), diagnostics)
}

struct Builder<'a> {
    file: &'a SourceFile,
    path: Arc<Path>,
    unsupported: Vec<Diagnostic>,
}

impl Builder<'_> {
    fn origin(&self, span: Range<usize>) -> Origin {
        Origin::new(self.file, &self.path, span)
    }

    fn table(&mut self, table: &DeTable<'_>, span: Range<usize>) -> LTable {
        let mut entries = BTreeMap::new();
        for (key, value) in table {
            let entry = LEntry {
                key_origin: self.origin(key.span()),
                value: self.value(value),
            };
            entries.insert(key.get_ref().to_string(), entry);
        }
        LTable {
            origin: self.origin(span),
            entries,
        }
    }

    fn value(&mut self, value: &Spanned<DeValue<'_>>) -> LValue {
        let span = value.span();
        let kind = match value.get_ref() {
            DeValue::String(s) => LKind::String(s.to_string()),
            DeValue::Integer(i) => {
                if let Ok(n) = i64::from_str_radix(i.as_str(), i.radix()) {
                    LKind::Integer(n)
                } else {
                    self.report(&span, "integer does not fit in 64 bits");
                    LKind::Unsupported
                }
            }
            DeValue::Float(f) => match f.as_str().parse::<f64>() {
                Ok(n) if n.is_finite() => LKind::Float(n),
                Ok(n) if n.is_infinite() && !f.as_str().contains("inf") => {
                    self.report(&span, "float out of range");
                    LKind::Unsupported
                }
                _ => {
                    self.report(&span, "non-finite floats (`nan`, `inf`) are not supported");
                    LKind::Unsupported
                }
            },
            DeValue::Boolean(b) => LKind::Boolean(*b),
            DeValue::Datetime(d) => {
                let _ = d;
                self.report(&span, "datetimes are not supported");
                LKind::Unsupported
            }
            DeValue::Array(items) => LKind::Array(items.iter().map(|v| self.value(v)).collect()),
            DeValue::Table(t) => LKind::Table(self.table(t, span.clone())),
        };
        LValue {
            origin: self.origin(span),
            kind,
        }
    }

    fn report(&mut self, span: &Range<usize>, message: &str) {
        let origin = self.origin(span.clone());
        self.unsupported.push(Diagnostic::new(
            DiagnosticCode::UnsupportedValue,
            message,
            Some(origin.location()),
        ));
    }
}

/// ASM-054 rule 1: every parser error is reported (sorted by span). Rule 2: an unclosed array or
/// inline table is reported at its opening bracket and the follow-on errors between the bracket
/// and the end of the file are dropped; an "unclosed" error is never dropped, so nested unclosed
/// containers each keep their own. Invariant: any parser error gives at least one diagnostic.
fn syntax_diagnostics(
    file: &SourceFile,
    errors: &[toml::de::Error],
    root: &LTable,
) -> Vec<Diagnostic> {
    let mut items: Vec<(Option<Range<usize>>, String)> = errors
        .iter()
        .map(|e| (e.span(), e.message().to_string()))
        .collect();
    items.sort_by(|a, b| {
        let key = |i: &(Option<Range<usize>>, String)| {
            (i.0.as_ref().map(|s| (s.start, s.end)), i.1.clone())
        };
        key(a).cmp(&key(b))
    });
    let unclosed: Vec<usize> = (0..items.len())
        .filter(|&i| {
            items
                .get(i)
                .is_some_and(|(_, m)| unclosed_open(m).is_some())
        })
        .collect();
    let mut dropped = vec![false; items.len()];
    let mut moved: BTreeMap<usize, Range<usize>> = BTreeMap::new();
    let mut taken: Vec<usize> = Vec::new();
    for &i in &unclosed {
        let Some((Some(span), message)) = items.get(i) else {
            continue;
        };
        let open = unclosed_open(message).unwrap_or('[');
        let Some(start) = find_open(root, file.text(), open, span.start, &taken) else {
            continue;
        };
        taken.push(start);
        for (j, (other, other_message)) in items.iter().enumerate() {
            if j != i
                && unclosed_open(other_message).is_none()
                && other
                    .as_ref()
                    .is_some_and(|o| o.start > start && o.start <= span.start)
                && let Some(d) = dropped.get_mut(j)
            {
                *d = true;
            }
        }
        moved.insert(i, start..start + 1);
    }
    let diagnostics: Vec<Diagnostic> = items
        .into_iter()
        .enumerate()
        .filter(|(i, _)| !dropped.get(*i).copied().unwrap_or(false))
        .map(|(i, (span, message))| {
            syntax_diagnostic(file, moved.get(&i).cloned().or(span), message)
        })
        .collect();
    diagnostics
}

/// One syntax diagnostic. A parser error without a span is not given an invented position.
fn syntax_diagnostic(file: &SourceFile, span: Option<Range<usize>>, message: String) -> Diagnostic {
    let Some(span) = span else {
        return Diagnostic::new(DiagnosticCode::Syntax, message, None)
            .with_note("position not reported by the parser", None);
    };
    let (line, column) = file.line_col(span.start);
    let location = Location {
        path: file.path().to_path_buf(),
        line,
        column,
        span,
    };
    Diagnostic::new(DiagnosticCode::Syntax, message, Some(location))
}

/// The opening bracket of the container an "unclosed ..." message talks about.
fn unclosed_open(message: &str) -> Option<char> {
    if message.starts_with("unclosed array") {
        Some('[')
    } else if message.starts_with("unclosed inline table") {
        Some('{')
    } else {
        None
    }
}

/// Whether a container's span ends with its own closing bracket: the last byte is `close` and it
/// is not the last byte of the last item (`[[1]` ends with the inner `]` only).
fn is_closed(value: &LValue, bytes: &[u8], close: u8) -> bool {
    let end = value.origin.span.end;
    let last_is_close = end
        .checked_sub(1)
        .and_then(|i| bytes.get(i))
        .is_some_and(|&b| b == close);
    let last_child_end = match &value.kind {
        LKind::Array(items) => items.last().map(|v| v.origin.span.end),
        LKind::Table(t) => t.entries.values().map(|e| e.value.origin.span.end).max(),
        _ => None,
    };
    last_is_close && last_child_end.is_none_or(|child| child < end)
}

/// The start of the container an unclosed error at byte `eof` belongs to. The partial tree ends
/// an unclosed container at the error offset, so the candidates are the containers of the right
/// kind that start with `open` and end at `eof`; the unclosed ones are the outermost of those
/// (a closed container cannot contain an unclosed one), so take the outermost not yet taken.
/// When the parser ended the container earlier (an inline table followed by more lines), fall back
/// to the outermost container of the right kind whose span does not end with its closing bracket.
fn find_open(root: &LTable, text: &str, open: char, eof: usize, taken: &[usize]) -> Option<usize> {
    let close = if open == '[' { b']' } else { b'}' };
    let bytes = text.as_bytes();
    let mut ending_here: Vec<usize> = Vec::new();
    let mut unterminated: Vec<usize> = Vec::new();
    let mut stack: Vec<&LValue> = root.entries.values().map(|e| &e.value).collect();
    while let Some(value) = stack.pop() {
        let range = &value.origin.span;
        let kind_matches = match &value.kind {
            LKind::Array(_) => open == '[',
            LKind::Table(_) => open == '{',
            _ => false,
        };
        let opens = bytes
            .get(range.start)
            .is_some_and(|&b| char::from(b) == open);
        if kind_matches && opens && !taken.contains(&range.start) && !is_closed(value, bytes, close)
        {
            if range.end == eof {
                ending_here.push(range.start);
            } else {
                unterminated.push(range.start);
            }
        }
        match &value.kind {
            LKind::Array(items) => stack.extend(items.iter()),
            LKind::Table(t) => stack.extend(t.entries.values().map(|e| &e.value)),
            _ => {}
        }
    }
    ending_here
        .into_iter()
        .min()
        .or_else(|| unterminated.into_iter().min())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_parser_error_without_a_span_gets_no_invented_position() {
        let file = SourceFile::new("a.toml", "x = 1\n");

        let d = syntax_diagnostic(&file, None, "boom".to_string());

        assert_eq!(d.location, None);
        assert_eq!(d.notes.len(), 1);
        assert!(d.to_string().starts_with("error[syntax]: boom"));
    }
}
