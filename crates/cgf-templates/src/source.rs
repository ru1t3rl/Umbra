//! A source file with a line index (REQ-T-2, ASM-056).

use std::path::{Path, PathBuf};
use std::sync::Arc;

/// A file's path and text, with a byte offset to (line, column) map.
#[derive(Debug, Clone)]
pub struct SourceFile {
    path: PathBuf,
    text: Arc<str>,
    /// Byte offset of the first character of every line; always starts with 0.
    line_starts: Arc<[usize]>,
}

impl SourceFile {
    /// A source file from in-memory text.
    pub fn new(path: impl Into<PathBuf>, text: impl Into<Arc<str>>) -> Self {
        let text: Arc<str> = text.into();
        let mut starts = vec![0];
        starts.extend(
            text.bytes()
                .enumerate()
                .filter(|&(_, b)| b == b'\n')
                .map(|(i, _)| i + 1),
        );
        Self {
            path: path.into(),
            text,
            line_starts: starts.into(),
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
    ///
    /// An offset past the end of the text is clamped to the end; an offset inside a multibyte
    /// character counts the characters that start before it.
    pub fn line_col(&self, byte: usize) -> (u32, u32) {
        let byte = byte.min(self.text.len());
        let line = self.line_starts.partition_point(|&s| s <= byte);
        let start = line
            .checked_sub(1)
            .and_then(|i| self.line_starts.get(i))
            .copied()
            .unwrap_or(0);
        let chars = self
            .text
            .as_bytes()
            .get(start..byte)
            .unwrap_or_default()
            .iter()
            .filter(|&&b| b & 0xC0 != 0x80)
            .count();
        (saturating_u32(line.max(1)), saturating_u32(chars + 1))
    }
}

fn saturating_u32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}
