//! Scenario loading and run control requests (REQ-X-5, ADR 0026).

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::seed::Seed;
use crate::status::{ClockSetting, RunStatus};

/// The most ticks one `step` request may ask for.
pub const MAX_STEP_COUNT: u64 = 1_000_000;

/// `POST /scenario`. A tick-rate field does not exist: the scenario's rate always applies (ADR 0026).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct LoadScenarioRequest {
    /// The scenario file: relative to the scenario root, or absolute inside it.
    pub path: String,
    /// Replaces the scenario's seed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<Seed>,
    /// Replaces the scenario's clock.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clock: Option<ClockSetting>,
}

/// `POST /run/step`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct StepRequest {
    /// Ticks to run, `1..=1000000`.
    pub count: u64,
}

impl StepRequest {
    /// Whether `count` is in `1..=`[`MAX_STEP_COUNT`]; otherwise the server answers
    /// `422 validation_failed`.
    #[must_use]
    pub fn in_range(&self) -> bool {
        (1..=MAX_STEP_COUNT).contains(&self.count)
    }
}

/// The answer of `POST /run/step`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct StepResult {
    /// The ticks asked for.
    pub requested: u64,
    /// Whether all of them ran; false when the duration or a `stop` ended the run first.
    pub completed: bool,
    /// The tick reached.
    pub reached: u64,
    /// The simulation time reached, in seconds.
    pub time_s: f64,
    /// The state hash at `reached`: 16 lower-case hex digits.
    pub state_hash: String,
    /// The run status after the step.
    pub run: RunStatus,
}

/// A state hash as 16 lower-case hexadecimal digits, the wire form everywhere.
#[must_use]
pub fn hex_state_hash(hash: u64) -> String {
    format!("{hash:016x}")
}
