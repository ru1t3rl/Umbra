//! `umbra-record`: the recording file format v1, its writer and its reader.
//!
//! Responsibility: a recording is the input stream of a run (scenario and plugin identity, seed,
//! state-changing commands, network inbound messages) plus periodic state hashes, stored as UTF-8
//! JSON Lines so a run can be replayed deterministically (REQ-K-7, REQ-K-4, ADR 0053, ADR 0054,
//! ASM-621..625, ASM-635).
//!
//! Layout: [`Header`] and [`Record`] are the data model; `encode` and `decode` map them to and
//! from JSON lines (keys in a fixed order, no wall-clock data, so equal inputs give equal bytes);
//! [`Writer`] and [`Reader`] move lines to and from a file.
//!
//! Dependency rule (REQ-A-2, ASM-635): depends on `umbra-core` only among workspace crates (plus
//! `serde_json`, `thiserror`). It contains no `unsafe` code.

#![forbid(unsafe_code)]

mod decode;
mod encode;
mod header;
mod reader;
mod records;
mod writer;

pub use header::{AdapterRef, FileDigest, Header, PluginRef, ScenarioRef};
pub use reader::{MAX_LINE_BYTES, Reader};
pub use records::{
    CommandRecord, EndRecord, FaultRecord, InboundRecord, Record, RecordedCommand, SpawnRecord,
};
pub use writer::Writer;

/// The `format` member of the header line.
pub const FORMAT: &str = "umbra.recording";
/// The format version this crate reads and writes. A reader rejects a newer one.
pub const FORMAT_VERSION: u32 = 1;
/// Records the writer queues before `push` blocks (ASM-625).
pub const DEFAULT_QUEUE: usize = 4096;

/// The JSON Schema (2020-12) of one line of recording format v1, `recording.v1.schema.json`.
pub const RECORDING_FILE_SCHEMA: &str = include_str!("../schemas/recording.v1.schema.json");

/// What a finished writer reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct WriteSummary {
    /// Records written, header excluded.
    pub records: u64,
    /// Bytes written, header included.
    pub bytes: u64,
}

/// Why reading or writing a recording failed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RecordError {
    /// The file could not be read or written.
    #[error("recording I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// A line is not a valid record.
    #[error("recording line {line}: {message}")]
    Syntax {
        /// 1-based line number.
        line: u64,
        /// What is wrong.
        message: String,
    },
    /// The file is a newer format than this reader supports.
    #[error(
        "recording line 1: format version {found} is newer than supported version {FORMAT_VERSION}"
    )]
    Version {
        /// The version found.
        found: u64,
    },
    /// The recording has no `end` record where one is required. The reader does not raise this
    /// (it streams what is there; a cut inside a line is a located [`RecordError::Syntax`]); the
    /// replay does when it needs a complete run (ASM-631).
    #[error("recording is truncated: it has no end record")]
    Truncated,
    /// A record's line is longer than the reader accepts ([`MAX_LINE_BYTES`]); nothing was queued.
    /// The recorder splits large inbound batches into several records of the same tick.
    #[error("recording record is {bytes} bytes encoded; the limit is {limit}")]
    LineTooLong {
        /// The encoded size, without the newline.
        bytes: u64,
        /// The limit, [`MAX_LINE_BYTES`].
        limit: u64,
    },
    /// A record cannot be written in format v1 (a value from a newer `umbra-core`).
    #[error("recording cannot encode: {0}")]
    Unsupported(String),
    /// The writer thread failed earlier; the recording is incomplete.
    #[error("recording failed: {0}")]
    Failed(String),
}
