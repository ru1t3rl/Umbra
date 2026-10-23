//! The recording reader: the header, then one record per line (ADR 0053).

use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;

use crate::{Header, Record, RecordError, decode};

/// The longest line the reader accepts, bytes. A bound against a hostile file; a real record is
/// far smaller (ASM-727).
pub const MAX_LINE_BYTES: u64 = 16 * 1024 * 1024;

/// Reads a recording record by record.
///
/// After the first error the iterator ends: a damaged line says nothing about the lines after it.
pub struct Reader {
    input: Box<dyn BufRead + Send>,
    line: u64,
    done: bool,
}

impl std::fmt::Debug for Reader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Reader")
            .field("line", &self.line)
            .field("done", &self.done)
            .finish_non_exhaustive()
    }
}

impl Reader {
    /// Opens the file and parses the header.
    ///
    /// # Errors
    /// [`RecordError`] with the line number for a malformed header or a newer version.
    pub fn open(path: &Path) -> Result<(Header, Self), RecordError> {
        Self::from_reader(BufReader::new(File::open(path)?))
    }

    /// Like [`Reader::open`] over any buffered input.
    ///
    /// # Errors
    /// See [`Reader::open`].
    pub fn from_reader<R: BufRead + Send + 'static>(
        input: R,
    ) -> Result<(Header, Self), RecordError> {
        let mut reader = Self {
            input: Box::new(input),
            line: 0,
            done: false,
        };
        let text = reader.next_line()?.ok_or_else(|| RecordError::Syntax {
            line: 1,
            message: "the file is empty: it has no header".to_owned(),
        })?;
        let header = decode::header(&text)?;
        Ok((header, reader))
    }

    /// The next complete line without its newline, or `None` at a clean end of file.
    fn next_line(&mut self) -> Result<Option<String>, RecordError> {
        let mut bytes = Vec::new();
        let read = (&mut self.input)
            .take(MAX_LINE_BYTES + 1)
            .read_until(b'\n', &mut bytes)?;
        if read == 0 {
            return Ok(None);
        }
        self.line += 1;
        if bytes.pop() != Some(b'\n') {
            let message = if read as u64 > MAX_LINE_BYTES {
                format!("line is longer than {MAX_LINE_BYTES} bytes")
            } else {
                "truncated: the file ends inside this line (no newline)".to_owned()
            };
            return Err(RecordError::Syntax {
                line: self.line,
                message,
            });
        }
        String::from_utf8(bytes)
            .map(Some)
            .map_err(|_| RecordError::Syntax {
                line: self.line,
                message: "line is not valid UTF-8".to_owned(),
            })
    }

    fn read_record(&mut self) -> Result<Option<Record>, RecordError> {
        let Some(text) = self.next_line()? else {
            return Ok(None);
        };
        decode::record(&text, self.line).map(Some)
    }
}

impl Iterator for Reader {
    type Item = Result<Record, RecordError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        let item = self.read_record().transpose();
        if !matches!(item, Some(Ok(_))) {
            self.done = true;
        }
        item
    }
}
