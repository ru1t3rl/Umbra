//! Run control: wire requests to engine commands and engine outcomes to wire results (WI-11).

use umbra_api_types as api;
use umbra_engine::handle::StepOutcome;

use super::status::run_status;

/// The wire result of a step (ADR 0026, M10).
#[must_use]
pub fn step_result(outcome: &StepOutcome) -> api::StepResult {
    api::StepResult {
        requested: outcome.requested,
        completed: outcome.completed,
        reached: outcome.reached.tick(),
        time_s: outcome.reached.as_seconds().0,
        state_hash: api::hex_state_hash(outcome.state_hash.0),
        run: run_status(&outcome.status),
    }
}
