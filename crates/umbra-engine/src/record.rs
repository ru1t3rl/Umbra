//! The engine recorder: writes the run's inputs and periodic state hashes to a recording
//! (REQ-K-7, REQ-K-4, REQ-K-8, ADR 0053, ASM-624..626, ASM-633).
//!
//! Status: M7 WI-1 stub. The shapes below are fixed by the step-4 acceptance tests; the
//! implementation is M7 WI-13 (the engine thread owns the recorder; a writer thread owns the
//! file). Tests that need it are `#[ignore = "M7 WI-13"]`.

use std::path::{Path, PathBuf};

use umbra_core::StateHashVersion;
use umbra_record::RecordError;

use crate::engine::Engine;
use crate::plugins::LoadedScenario;

/// How and where to record.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct RecordOptions {
    /// The recording file (recommended suffix `.umbrarec.jsonl`, ASM-622).
    pub path: PathBuf,
    /// Hash cadence in ticks; `None` means the tick rate (once per simulated second, ASM-624).
    pub hash_every: Option<u64>,
    /// The state hash encoding of the `hash` records and the header (ADR 0055).
    pub state_hash: StateHashVersion,
}

impl RecordOptions {
    /// Options for `path` with the default cadence and state hash v2.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            hash_every: None,
            state_hash: StateHashVersion::V2,
        }
    }
}

/// Where a recording stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RecordingState {
    /// Records are being written.
    Active,
    /// The run ended and the file is complete.
    Finished,
    /// A write failed; the file is incomplete (ASM-625).
    Failed,
}

/// A live view of the recorder (REST `RunStatus.recording`, M7 WI-16).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct RecordingStatus {
    /// The state.
    pub state: RecordingState,
    /// The recording file.
    pub file: PathBuf,
    /// Records pushed so far.
    pub records: u64,
    /// The last tick that has a record.
    pub last_tick: u64,
}

/// The recorder an [`Engine`] feeds through [`Engine::attach_recorder`].
#[derive(Debug)]
pub struct Recorder {
    _private: (),
}

impl Recorder {
    /// Creates the file and its header from the loaded scenario at `scenario_path` and its
    /// plugins, with `seed` the effective seed of the run (`None` = the scenario's).
    ///
    /// # Errors
    /// [`RecordError::Io`] if the file cannot be created or a scenario or library file cannot be
    /// read for its digest.
    pub fn create(
        options: RecordOptions,
        scenario_path: &Path,
        loaded: &LoadedScenario,
        seed: Option<u64>,
    ) -> Result<Self, RecordError> {
        let _ = (options, scenario_path, loaded, seed);
        unimplemented!("M7 WI-13")
    }
}

impl Engine {
    /// Attaches the recorder: from now on commands, inbound messages, faults and hashes are
    /// recorded in tick order (ADR 0053). Attach before the first tick.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "M7 WI-1 stub: WI-13 stores the recorder"
    )]
    pub fn attach_recorder(&mut self, recorder: Recorder) {
        let _ = recorder;
        unimplemented!("M7 WI-13")
    }

    /// Writes the `end` record and closes the file; returns the status at the end. `None` without
    /// a recorder.
    ///
    /// # Errors
    /// [`RecordError::Failed`] if any write failed during the run.
    pub fn finish_recording(
        &mut self,
        reason: &str,
    ) -> Result<Option<RecordingStatus>, RecordError> {
        let _ = reason;
        unimplemented!("M7 WI-13")
    }

    /// The recorder's live status, `None` without a recorder.
    ///
    /// M7 WI-1 stub: always `None`, which is the truth until `attach_recorder` exists (M7 WI-13).
    #[must_use]
    pub fn recording(&self) -> Option<RecordingStatus> {
        None
    }
}
