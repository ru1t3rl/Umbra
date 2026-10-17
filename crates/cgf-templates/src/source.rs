//! STUB (WI-4): a source file with a line index (ASM-056).
#![expect(clippy::todo, reason = "stub until WI-4: signatures only")]

use std::path::{Path, PathBuf};
use std::sync::Arc;

/// A file's path and text, with a byte offset to (line, column) map.
#[derive(Debug, Clone)]
pub struct SourceFile {
    path: PathBuf,
    text: Arc<str>,
}

impl SourceFile {
    /// A source file from in-memory text.
    pub fn new(path: impl Into<PathBuf>, text: impl Into<Arc<str>>) -> Self {
        Self {
            path: path.into(),
            text: text.into(),
        }
    }

    /// The path used in diagnostics.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The text.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// 1-based line and column (in Unicode scalar values) of a byte offset; CRLF is one line break.
    pub fn line_col(&self, _byte: usize) -> (u32, u32) {
        todo!("WI-4")
    }
}
