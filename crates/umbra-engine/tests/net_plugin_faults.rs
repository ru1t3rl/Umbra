//! The engine's reaction to a faulting plugin network adapter (M6 WI-12, moved from WI-11 by
//! ASM-407). Requirements: REQ-N-1, REQ-P-7, ADR 0021, ADR 0033, ASM-252. The adapter is
//! `test.network.net_panics_in_poll` of the fixture plugin built with the feature `network`
//! (`umbra-test-plugin`, SDK-built), whose `poll` panics from tick `panic_at_tick` = 5 on: under
//! `abort` the run ends after the tick of the fault with `EngineError::PluginFault` naming the plugin,
//! the capability and `poll`; under `disable_capability` the adapter is disabled and the run goes
//! on; a `configure` refusing its params is a located `ScenarioError` naming the capability. The
//! `abort` row also runs through the engine handle (the session path, `Pumped::Aborted`), whose run
//! stops with `StopReason::PluginFault` (review of WI-12).

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod handle_support;
mod net_support;
mod plugin_support;

use std::ops::ControlFlow;
use std::path::PathBuf;
use std::sync::LazyLock;

use umbra_core::StateHash;
use umbra_engine::builtins::builtin_registry;
use umbra_engine::bus::{RunState, StopReason};
use umbra_engine::scenario::ScenarioError;
use umbra_engine::{EngineError, load_scenario_with_plugins};
use umbra_templates::DiagnosticCode;

use net_support::{fixture, start_with_plugins};

/// The fixture plugin with its network capabilities.
static PLUGINS: LazyLock<PathBuf> = LazyLock::new(|| plugin_support::fixture(&["network"]));

/// The tick whose `poll` panics (`panic_at_tick` in the fixtures): `poll` runs before the kernel
/// tick, at the time the tick starts from, so the fault happens in tick `PANIC_AT + 1`.
const PANIC_AT: u64 = 5;

/// Runs `net_pair/<dir>` for at most `ticks` ticks; the engine, the per-tick hashes and the result.
fn run(
    dir: &str,
    ticks: u64,
) -> (
    umbra_engine::Engine,
    Vec<StateHash>,
    Result<umbra_engine::RunSummary, EngineError>,
) {
    let registry = builtin_registry().expect("the built-in registry builds");
    let mut engine =
        start_with_plugins(dir, &registry, std::slice::from_ref(&*PLUGINS), None).engine;
    let mut hashes = Vec::new();
    // The fixtures run on the stepped clock: `step` requests the ticks.
    let result = engine.step(ticks, |v| {
        hashes.push(v.state_hash());
        ControlFlow::Continue(())
    });
    (engine, hashes, result)
}

#[test]
fn under_abort_a_plugin_adapter_fault_ends_the_run_after_the_tick() {
    let (engine, hashes, result) = run("plugin_fault_abort", 60);

    let Err(EngineError::PluginFault(fault)) = result else {
        panic!("expected EngineError::PluginFault, got {result:?}");
    };
    assert_eq!(
        (fault.plugin.as_str(), fault.capability.as_str(), fault.call),
        (
            "test.plugin.fixture",
            "test.network.net_panics_in_poll",
            "poll"
        )
    );
    assert!(
        fault.message.contains("poll panics on purpose"),
        "{}",
        fault.message
    );
    assert_eq!(
        engine.now().tick(),
        PANIC_AT + 1,
        "the run stops right after the faulting tick"
    );
    assert_eq!(
        hashes.len() as u64,
        PANIC_AT + 1,
        "the faulting tick completed and was reported"
    );
}

#[test]
fn under_disable_capability_the_adapter_is_disabled_and_the_run_goes_on() {
    let (engine, hashes, result) = run("plugin_fault_disable", 60);

    let summary = result.expect("the run goes on");
    assert_eq!(summary.ticks_run, 60);
    assert_eq!(hashes.len(), 60);
    let status = engine.network().expect("link").status();
    assert_eq!(status.len(), 1);
    assert!(status[0].faulted && !status[0].enabled, "{:?}", status[0]);
    assert!(
        status[0]
            .fault_reason
            .as_deref()
            .is_some_and(|r| r.contains("test.network.net_panics_in_poll")),
        "{:?}",
        status[0].fault_reason
    );
}

#[test]
fn abort_and_disable_capability_agree_on_the_faulting_tick() {
    let (stopped, _, result) = run("plugin_fault_abort", 60);
    let (_, hashes, _) = run("plugin_fault_disable", 60);

    assert!(result.is_err());
    let tick = usize::try_from(stopped.now().tick()).expect("fits");
    assert_eq!(
        stopped.state_hash(),
        hashes[tick - 1],
        "hash at tick {tick}"
    );
}

#[test]
fn configure_with_bad_params_is_a_located_scenario_error_naming_the_capability() {
    let registry = builtin_registry().expect("the built-in registry builds");
    let path = fixture("plugin_bad_params");

    let err = load_scenario_with_plugins(&registry, &path, std::slice::from_ref(&*PLUGINS))
        .err()
        .expect("the load must fail");

    let ScenarioError::Invalid(diagnostics) = err else {
        panic!("expected ScenarioError::Invalid, got {err}");
    };
    let diagnostics = diagnostics.into_vec();
    let [d] = &diagnostics[..] else {
        panic!("one diagnostic: {diagnostics:?}");
    };
    assert_eq!(d.code, DiagnosticCode::ParamInvalid);
    assert!(d.message.contains("test.network.net_ok"), "{}", d.message);
    assert!(d.message.contains("remote_offset"), "{}", d.message);
    let at = d.location.as_ref().expect("located");
    assert_eq!(at.path, path);
    let text = std::fs::read_to_string(&path).expect("read the fixture");
    let line = text
        .lines()
        .nth(at.line as usize - 1)
        .expect("the line exists");
    assert!(
        line.starts_with("params = { remote_offset = 7 }"),
        "points at the params: {line:?}"
    );
}

/// Review of WI-12: on the engine handle (the session path), the same fault stops the run with
/// `StopReason::PluginFault` naming the plugin, the capability and the call.
#[test]
fn on_the_engine_handle_a_plugin_adapter_fault_under_abort_stops_the_run_with_plugin_fault() {
    let _lock = handle_support::plugin_lock();
    let mut config = handle_support::frozen_config();
    config.base_registry = builtin_registry().expect("the built-in registry builds");
    config.load.extra_plugin_paths = vec![PLUGINS.clone()];
    let (handle, _thread) = handle_support::start_engine(config);
    handle
        .load(
            fixture("plugin_fault_abort"),
            None,
            Some(umbra_engine::ClockMode::Stepped),
        )
        .expect("load");
    handle.start().expect("start");

    let stepped = handle_support::guarded_step(&handle, 60);

    assert!(stepped.is_err(), "the step is cut short: {stepped:?}");
    let run = handle.status().expect("status").run;
    assert_eq!(run.state, RunState::Stopped);
    let Some(StopReason::PluginFault(fault)) = &run.stop_reason else {
        panic!(
            "expected StopReason::PluginFault, got {:?}",
            run.stop_reason
        );
    };
    assert_eq!(
        (fault.plugin.as_str(), fault.capability.as_str(), fault.call),
        (
            "test.plugin.fixture",
            "test.network.net_panics_in_poll",
            "poll"
        )
    );
    assert_eq!(
        run.time.map(umbra_core::SimTime::tick),
        Some(PANIC_AT + 1),
        "stopped right after the faulting tick"
    );
}
