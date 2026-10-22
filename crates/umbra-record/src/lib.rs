//! `umbra-record`: the recording file format v1, its writer and its reader.
//!
//! Responsibility: a recording is the input stream of a run (scenario and plugin identity, seed,
//! state-changing commands, network inbound messages) plus periodic state hashes, stored as UTF-8
//! JSON Lines so a run can be replayed deterministically (REQ-K-7, REQ-K-4, ADR 0053, ADR 0054,
//! ASM-621..625, ASM-635).
//!
//! Dependency rule (REQ-A-2, ASM-635): depends on `umbra-core` only among workspace crates (plus
//! `serde`, `serde_json`, `thiserror`). It contains no `unsafe` code.
//!
//! Status: M7 WI-1 skeleton. The types and signatures below are final (ADR 0053); the writer, the
//! reader and the schema are `unimplemented!` stubs until M7 WI-11, which also adds the encoder and
//! decoder. Acceptance tests that need them are `#[ignore = "M7 WI-11"]`.

#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};

use umbra_core::net::{KeySpace, NetMessage};
use umbra_core::{EntityId, Force, MetersPerSecond, Position, Radians, StateHash, TemplateId};

/// The `format` member of the header line.
pub const FORMAT: &str = "umbra.recording";
/// The format version this crate reads and writes. A reader rejects a newer one.
pub const FORMAT_VERSION: u32 = 1;
/// Records the writer queues before `push` blocks (ASM-625).
pub const DEFAULT_QUEUE: usize = 4096;

/// The first line of a recording.
#[derive(Debug, Clone, PartialEq)]
pub struct Header {
    /// The engine version that wrote the file.
    pub umbra: String,
    /// The scenario and the digests of the files it was loaded from.
    pub scenario: ScenarioRef,
    /// The effective seed of the run.
    pub seed: u64,
    /// The effective tick rate, Hz.
    pub tick_rate_hz: u32,
    /// Where the tick rate came from (ADR 0029).
    pub tick_rate_source: String,
    /// The state hash version the `hash` records use (ADR 0055).
    pub state_hash_version: u32,
    /// Hash cadence in ticks (ASM-624).
    pub hash_every: u64,
    /// The scenario's plugin fault policy.
    pub on_plugin_fault: String,
    /// The plugins the run loaded.
    pub plugins: Vec<PluginRef>,
    /// The network adapters of the run, in scenario order.
    pub network_adapters: Vec<AdapterRef>,
}

/// The scenario a recording was made from.
#[derive(Debug, Clone, PartialEq)]
pub struct ScenarioRef {
    /// The scenario file as given (canonical absolute).
    pub path: PathBuf,
    /// The same, relative to the recording's directory when possible (ASM-633).
    pub relative_path: Option<PathBuf>,
    /// Every file the scenario was loaded from.
    pub files: Vec<FileDigest>,
}

/// A file and its FNV-1a 64 digest (ASM-623).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileDigest {
    /// The file.
    pub path: PathBuf,
    /// FNV-1a 64 over the bytes.
    pub fnv1a64: u64,
}

/// A plugin the run loaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginRef {
    /// The plugin id.
    pub id: String,
    /// The plugin version.
    pub version: String,
    /// The ABI the plugin declared, `(major, minor)`.
    pub abi: (u32, u32),
    /// FNV-1a 64 over the library bytes.
    pub library_fnv1a64: u64,
}

/// A network adapter of the run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterRef {
    /// The scenario name of the adapter.
    pub name: String,
    /// The adapter component id.
    pub adapter: String,
    /// The keys it issues.
    pub key_space: KeySpace,
}

/// One line after the header.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Record {
    /// A command applied between two ticks.
    Command(CommandRecord),
    /// Messages one adapter delivered in one tick.
    Inbound(InboundRecord),
    /// A network adapter fault seen live.
    Fault(FaultRecord),
    /// The state hash after a tick.
    Hash {
        /// The tick that just ran.
        tick: u64,
        /// The hash, in the header's version.
        hash: StateHash,
    },
    /// The end of the run.
    End(EndRecord),
}

/// A command and where it was applied.
#[derive(Debug, Clone, PartialEq)]
pub struct CommandRecord {
    /// Ticks completed when the command was applied.
    pub before_tick: u64,
    /// Application order within one boundary.
    pub seq: u64,
    /// The command.
    pub command: RecordedCommand,
}

/// A spawn as recorded: the full template spawn (ASM-626).
#[derive(Debug, Clone, PartialEq)]
pub struct SpawnRecord {
    /// The template.
    pub template: TemplateId,
    /// Where the entity started.
    pub position: Position,
    /// Initial heading.
    pub heading: Radians,
    /// Initial speed.
    pub speed: MetersPerSecond,
    /// Optional unique name.
    pub name: Option<String>,
    /// Optional group.
    pub group: Option<String>,
    /// Optional force.
    pub force: Option<Force>,
}

/// A recorded command (ASM-626): the first three change state, the rest are informational.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum RecordedCommand {
    /// Spawn from a template; `result` is the id the live run assigned.
    Spawn {
        /// What was spawned.
        spec: SpawnRecord,
        /// The id it received.
        result: EntityId,
    },
    /// Remove an entity.
    Remove {
        /// The entity.
        entity: EntityId,
    },
    /// Enable or disable a network adapter.
    SetAdapterEnabled {
        /// The adapter's scenario name.
        name: String,
        /// The new state.
        enabled: bool,
    },
    /// Informational: the run started.
    Start,
    /// Informational: the run paused.
    Pause,
    /// Informational: the run resumed.
    Resume,
    /// Informational: stepped this many ticks.
    Step(u64),
    /// Informational: the clock mode changed.
    SetClock(String),
    /// Informational: the run stopped.
    Stop,
}

/// Messages one adapter delivered in one tick, exactly as polled (ASM-627).
#[derive(Debug, Clone, PartialEq)]
pub struct InboundRecord {
    /// The tick about to run.
    pub tick: u64,
    /// The adapter's scenario name.
    pub adapter: String,
    /// The canonical messages.
    pub messages: Vec<NetMessage>,
}

/// An adapter fault seen live.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FaultRecord {
    /// The tick about to run.
    pub tick: u64,
    /// The adapter's scenario name.
    pub adapter: String,
    /// The adapter call that failed (`poll` or `publish`).
    pub call: String,
}

/// The last record of a complete recording.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndRecord {
    /// The last tick that ran.
    pub tick: u64,
    /// Why the run ended (`completed`, `stopped`, `plugin_fault`, ...).
    pub reason: String,
    /// The final state hash.
    pub hash: StateHash,
}

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
    #[error("recording format version {found} is newer than supported version {FORMAT_VERSION}")]
    Version {
        /// The version found.
        found: u32,
    },
    /// The file ends inside a line or has no `end` record where one is required.
    #[error("recording is truncated")]
    Truncated,
    /// The writer thread failed earlier; the recording is incomplete.
    #[error("recording failed: {0}")]
    Failed(String),
}

/// Writes a recording through a bounded queue to a writer thread; lossless, with backpressure
/// (ASM-625).
#[derive(Debug)]
pub struct Writer {
    _private: (),
}

impl Writer {
    /// Creates the file and writes the header; `queue` is the number of records buffered.
    ///
    /// # Errors
    /// [`RecordError::Io`] if the file cannot be created.
    pub fn create(path: &Path, header: &Header, queue: usize) -> Result<Self, RecordError> {
        let _ = (path, header, queue);
        unimplemented!("M7 WI-11")
    }

    /// Queues a record; blocks while the queue is full, never drops.
    ///
    /// # Errors
    /// [`RecordError::Failed`] once the writer thread failed.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "M7 WI-1 stub: WI-11 moves the record into the queue"
    )]
    pub fn push(&self, record: Record) -> Result<(), RecordError> {
        let _ = record;
        unimplemented!("M7 WI-11")
    }

    /// Flushes everything and joins the writer thread.
    ///
    /// # Errors
    /// [`RecordError::Failed`] if any write failed.
    pub fn finish(self) -> Result<WriteSummary, RecordError> {
        unimplemented!("M7 WI-11")
    }
}

/// Reads a recording record by record.
#[derive(Debug)]
pub struct Reader {
    _private: (),
}

impl Reader {
    /// Opens the file and parses the header.
    ///
    /// # Errors
    /// [`RecordError`] with the line number for a malformed header or a newer version.
    pub fn open(path: &Path) -> Result<(Header, Self), RecordError> {
        let _ = path;
        unimplemented!("M7 WI-11")
    }
}

impl Iterator for Reader {
    type Item = Result<Record, RecordError>;

    fn next(&mut self) -> Option<Self::Item> {
        unimplemented!("M7 WI-11")
    }
}

/// The JSON Schema (2020-12) of recording format v1, `recording.v1.schema.json`.
#[must_use]
pub fn schema() -> &'static str {
    unimplemented!("M7 WI-11")
}
