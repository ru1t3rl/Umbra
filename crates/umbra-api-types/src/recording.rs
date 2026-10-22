//! The session recording status (added in API 1.2.0, ADR 0057, REQ-X-5).

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Where a session recording stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RecordingState {
    /// Records are being written.
    Recording,
    /// Writing failed; the recording is incomplete.
    Failed,
    /// The session ended and the recording was closed.
    Finished,
}

/// The recording of the current session, in `RunStatus.recording`; absent when not recording.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct RecordingDto {
    /// Where the recording stands.
    pub state: RecordingState,
    /// The file name inside the record directory, `/` separators, never absolute (ADR 0053, ADR 0058).
    pub file: String,
    /// Records written so far. Exact below 2^53.
    pub records: u64,
    /// The last tick recorded. Exact below 2^53.
    pub last_tick: u64,
}
