//! Replay: re-executes a recording from tick 0 and verifies its state hashes (REQ-K-7, REQ-K-4,
//! ADR 0054, ASM-628..631).
//!
//! Status: M7 WI-1 stub. Implemented in M7 WI-14; the acceptance tests (`tests/replay.rs`,
//! `tests/replay_determinism.rs` and the network replay test) are ignored until then.

use std::path::{Path, PathBuf};

use umbra_core::{Registry, StateHash};
use umbra_record::RecordError;

use crate::engine::TickView;
use crate::error::EngineError;

/// How to replay.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ReplayOptions {
    /// Plugin directories, searched like `umbra run --plugin-path`.
    pub plugin_paths: Vec<PathBuf>,
    /// A scenario to use when neither recorded path resolves (`--scenario`).
    pub scenario: Option<PathBuf>,
    /// Warn instead of refusing on file or plugin mismatches (`--allow-mismatch`).
    pub allow_mismatch: bool,
    /// Compare hashes; `false` is `--no-verify`.
    pub verify: bool,
    /// Accept a recording without an `end` record (`--partial`).
    pub partial: bool,
}

impl Default for ReplayOptions {
    fn default() -> Self {
        Self {
            plugin_paths: Vec::new(),
            scenario: None,
            allow_mismatch: false,
            verify: true,
            partial: false,
        }
    }
}

/// What a finished replay reports.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ReplaySummary {
    /// Ticks re-executed.
    pub ticks: u64,
    /// The state hash after the last tick, in the recording's hash version.
    pub final_hash: StateHash,
    /// Recorded hashes that were compared and matched.
    pub hashes_verified: u64,
    /// Scenario names of the network adapters that were fed from the recording (a `ReplaySource`
    /// adapter each) instead of opening sockets, in scenario order (ADR 0054).
    pub replayed_adapters: Vec<String>,
}

/// Why a replay did not complete.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ReplayError {
    /// The recording could not be read.
    #[error(transparent)]
    Read(#[from] RecordError),
    /// The scenario files or plugins differ from the recording's, or the recording is not
    /// replayable with the given options.
    #[error("replay refused: {what}")]
    Mismatch {
        /// What differs.
        what: String,
    },
    /// A recorded hash differs from the replayed one.
    #[error("replay diverged at tick {tick}: expected {expected}, got {got}")]
    Diverged {
        /// The first divergent tick.
        tick: u64,
        /// The recorded hash.
        expected: StateHash,
        /// The replayed hash.
        got: StateHash,
    },
    /// The engine failed while replaying.
    #[error(transparent)]
    Engine(#[from] EngineError),
}

/// Replays `recording` against `base` plus the plugins named in the options, calling `on_tick`
/// after each tick.
///
/// # Errors
/// [`ReplayError`]; `Diverged` names the first tick whose hash differs.
pub fn replay(
    base: &Registry,
    recording: &Path,
    options: ReplayOptions,
    on_tick: impl FnMut(&TickView<'_>),
) -> Result<ReplaySummary, ReplayError> {
    let _ = (base, recording, options, on_tick);
    unimplemented!("M7 WI-14")
}
