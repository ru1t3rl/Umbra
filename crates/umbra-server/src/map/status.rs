//! Run status, scenario info and diagnostics: engine types to wire types (WI-11).

use umbra_api_types as api;
use umbra_engine::bus::{RunState, RunStatus, StopReason};
use umbra_engine::handle::{EngineStatus, SessionInfo};
use umbra_engine::{Diagnostic, PluginFault, SimTime, TickRate, TickRateSource};

use super::clock_setting;

/// The wire run status.
///
/// A `stopped(error)` status carries no message: the kernel error text is not part of the wire
/// status (ASM-194, ASM-197). The message of a plugin fault was written in display form by the engine.
#[must_use]
pub fn run_status(status: &RunStatus) -> api::RunStatus {
    let (stop_reason, fault) = match &status.stop_reason {
        None => (None, None),
        Some(StopReason::Command) => (Some(api::StopReason::Command), None),
        Some(StopReason::Duration) => (Some(api::StopReason::Duration), None),
        Some(StopReason::PluginFault(fault)) => (
            Some(api::StopReason::PluginFault),
            Some(plugin_fault(fault)),
        ),
        // `Error`, and any reason added later (the enum is non-exhaustive) until it is mapped.
        Some(_) => (Some(api::StopReason::Error), None),
    };
    api::RunStatus {
        state: run_state(status.state),
        stop_reason,
        fault,
        clock: clock_setting(status.clock),
        session: status.session,
        tick: status.time.map(SimTime::tick),
        time_s: status.time.map(|t| t.as_seconds().0),
        step_pending: status.step_pending,
        tick_rate_hz: status.tick_rate.map(TickRate::hz),
        tick_rate_source: status.tick_rate_source.map(tick_rate_source),
    }
}

/// The wire spelling of a run state (`running`), for messages.
#[must_use]
pub fn run_state_name(state: RunState) -> &'static str {
    match run_state(state) {
        api::RunState::Idle => "idle",
        api::RunState::Ready => "ready",
        api::RunState::Running => "running",
        api::RunState::Paused => "paused",
        api::RunState::Stopped => "stopped",
    }
}

fn run_state(state: RunState) -> api::RunState {
    match state {
        RunState::Ready => api::RunState::Ready,
        RunState::Running => api::RunState::Running,
        RunState::Paused => api::RunState::Paused,
        RunState::Stopped => api::RunState::Stopped,
        // `Idle`, and any state added later (the enum is non-exhaustive) until it is mapped.
        _ => api::RunState::Idle,
    }
}

pub(super) fn tick_rate_source(source: TickRateSource) -> api::TickRateSource {
    match source {
        TickRateSource::Scenario => api::TickRateSource::Scenario,
        // `Default`, and any source added later (the enum is non-exhaustive) until it is mapped.
        _ => api::TickRateSource::Default,
    }
}

/// The wire form of a plugin fault.
#[must_use]
pub fn plugin_fault(fault: &PluginFault) -> api::PluginFaultDto {
    api::PluginFaultDto {
        plugin: fault.plugin.to_string(),
        capability: fault.capability.to_string(),
        call: fault.call.to_owned(),
        message: fault.message.clone(),
    }
}

/// The wire scenario info; the engine already shows the path relative to the scenario root.
#[must_use]
pub fn scenario_info(info: &SessionInfo) -> api::ScenarioInfo {
    api::ScenarioInfo {
        session: info.session,
        name: info.name.clone(),
        path: info.path.clone(),
        seed: info.seed.to_string(),
        tick_rate_hz: info.tick_rate.hz(),
        tick_rate_source: tick_rate_source(info.tick_rate_source),
        duration_ticks: info.duration_ticks,
        entity_count: info.entity_count as u64,
    }
}

/// The wire engine status.
#[must_use]
pub fn engine_status(status: &EngineStatus) -> api::EngineStatus {
    api::EngineStatus {
        api_version: api::API_VERSION.to_owned(),
        run: run_status(&status.run),
        scenario: status.scenario.as_ref().map(scenario_info),
    }
}

/// The wire diagnostic. The engine has written the files and quoted paths in display form.
#[must_use]
pub fn diagnostic(diagnostic: &Diagnostic) -> api::Diagnostic {
    let file = |path: &std::path::Path| path.to_string_lossy().into_owned();
    api::Diagnostic {
        code: diagnostic.code.as_str().to_owned(),
        message: diagnostic.message.clone(),
        file: diagnostic.location.as_ref().map(|l| file(&l.path)),
        line: diagnostic.location.as_ref().map(|l| l.line),
        column: diagnostic.location.as_ref().map(|l| l.column),
        notes: diagnostic
            .notes
            .iter()
            .map(|note| api::DiagnosticNote {
                message: note.message.clone(),
                file: note.location.as_ref().map(|l| file(&l.path)),
                line: note.location.as_ref().map(|l| l.line),
                column: note.location.as_ref().map(|l| l.column),
            })
            .collect(),
    }
}
