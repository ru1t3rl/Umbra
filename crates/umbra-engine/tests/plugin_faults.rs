//! AT-4 engine rows (faults per engine, `abort` and `disable_capability`, a fault in a plugin used as
//! a `subgraph` child) and the engine rows of WI-12: `attach_plugins`, the build scope.
//! Requirements: REQ-P-7, REQ-P-10, REQ-K-4, ADR 0021 (R-M3), re-review RR-m2.
//!
//! Uses the fixture plugin (`test.behavior.ok`, `test.behavior.panic_tick`: panics in `tick_batch`
//! on its first tick) through `plugin_support::fault_scenario`. Step 4 of M4: written before WI-12;
//! ignored with WI-12.

#![forbid(unsafe_code)]

mod behavior_support;
mod common;
mod plugin_support;

use core::time::Duration;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use behavior_support::standard;
use common::options;
use umbra_core::StateHash;
use umbra_engine::scenario::instantiate;
use umbra_engine::{
    EngineError, LoadedScenario, RunUntil, instantiate_with_plugins, load_scenario_with_plugins,
};

static FIXTURE: LazyLock<PathBuf> = LazyLock::new(|| plugin_support::fixture(&[]));
const TICKS: u64 = 60;

fn load(scenario: &Path) -> LoadedScenario {
    load_scenario_with_plugins(&standard(), scenario, std::slice::from_ref(&*FIXTURE))
        .unwrap_or_else(|e| panic!("load: {e}"))
}

/// Runs `TICKS` ticks; the per-tick hashes seen by the callback and how the run ended.
fn run(
    loaded: &LoadedScenario,
) -> (
    umbra_engine::Engine,
    Vec<StateHash>,
    Result<umbra_engine::RunSummary, EngineError>,
) {
    let mut engine =
        instantiate_with_plugins(loaded, options(Duration::from_micros(500), None, None))
            .unwrap_or_else(|e| panic!("instantiate: {e}"))
            .engine;
    let mut hashes = Vec::new();
    let result = engine.run(RunUntil::Ticks(TICKS), |v| {
        hashes.push(v.state_hash());
        ControlFlow::Continue(())
    });
    (engine, hashes, result)
}

/// ADR 0021: under `abort` (the default) the run stops after the tick in which the plugin faulted,
/// with `EngineError::PluginFault` naming the plugin, the capability and the call.
#[test]
fn abort_stops_after_the_faulting_tick_with_plugin_fault() {
    let loaded = load(&plugin_support::fault_scenario(
        "abort-direct",
        "abort",
        2.0,
        &["healthy", "direct"],
    ));

    let (engine, _, result) = run(&loaded);

    let Err(EngineError::PluginFault(fault)) = result else {
        panic!("expected EngineError::PluginFault, got {result:?}");
    };
    assert_eq!(fault.plugin.as_str(), "test.plugin.fixture");
    assert_eq!(fault.capability.as_str(), "test.behavior.panic_tick");
    assert_eq!(fault.call, "tick_batch");
    assert_eq!(
        engine.now().tick(),
        1,
        "`panic_tick` faults on its first tick, and the run stops right after it"
    );
}

/// ADR 0021: the state at the faulting tick does not depend on the policy: the `abort` run's hash at
/// the stop equals a `disable_capability` run's hash at the same tick.
#[test]
fn abort_and_disable_capability_agree_on_the_faulting_tick() {
    let abort = load(&plugin_support::fault_scenario(
        "agree-abort",
        "abort",
        2.0,
        &["healthy", "direct"],
    ));
    let disable = load(&plugin_support::fault_scenario(
        "agree-disable",
        "disable_capability",
        2.0,
        &["healthy", "direct"],
    ));

    let (stopped, _, result) = run(&abort);
    let (_, hashes, _) = run(&disable);

    assert!(result.is_err());
    let tick = usize::try_from(stopped.now().tick()).expect("fits");
    assert_eq!(
        stopped.state_hash(),
        hashes[tick - 1],
        "hash at tick {tick}"
    );
}

/// ADR 0021: under `disable_capability` the run goes to the end; the capability is marked faulted
/// in this host, the fault is recorded once, and the healthy entity keeps ticking.
#[test]
fn disable_capability_runs_to_the_end_and_records_one_fault() {
    let loaded = load(&plugin_support::fault_scenario(
        "disable-direct",
        "disable_capability",
        2.0,
        &["healthy", "direct"],
    ));

    let (engine, hashes, result) = run(&loaded);

    let summary = result.unwrap_or_else(|e| panic!("run: {e}"));
    assert_eq!(summary.ticks_run, TICKS);
    assert_eq!(hashes.len(), 60);
    assert_eq!(engine.now().tick(), TICKS);
    let faults = loaded.plugins.faults();
    assert_eq!(faults.len(), 1, "{faults:?}");
    let faulted: Vec<&str> = loaded.plugins.plugins()[0]
        .capabilities
        .iter()
        .filter(|c| c.faulted)
        .map(|c| c.id.as_str())
        .collect();
    assert_eq!(faulted, ["test.behavior.panic_tick"]);
}

/// AT-4 faults row: a plugin behavior used as a `subgraph` child of a file graph (it runs inside its
/// caller's graph, unbatched) faults like a top-level one: `abort` stops the run.
#[test]
fn fault_in_a_plugin_subgraph_child_under_abort_stops_the_run() {
    let loaded = load(&plugin_support::fault_scenario(
        "abort-child",
        "abort",
        2.0,
        &["healthy", "via-graph"],
    ));

    let (_, _, result) = run(&loaded);

    let Err(EngineError::PluginFault(fault)) = result else {
        panic!("expected EngineError::PluginFault, got {result:?}");
    };
    assert_eq!(fault.capability.as_str(), "test.behavior.panic_tick");
}

/// AT-4 faults row, the other policy: the child faults, the capability is disabled and the run ends
/// normally.
#[test]
fn fault_in_a_plugin_subgraph_child_under_disable_capability_runs_on() {
    let loaded = load(&plugin_support::fault_scenario(
        "disable-child",
        "disable_capability",
        2.0,
        &["healthy", "via-graph"],
    ));

    let (_, _, result) = run(&loaded);

    assert_eq!(result.expect("run").ticks_run, TICKS);
    let faults = loaded.plugins.faults();
    assert_eq!(faults.len(), 1, "{faults:?}");
    assert_eq!(faults[0].capability.as_str(), "test.behavior.panic_tick");
}

/// ADR 0021: a host belongs to one engine. A second engine refuses a host that is already attached.
#[test]
fn attach_plugins_refuses_a_host_already_attached() {
    let loaded = load(&plugin_support::fault_scenario(
        "attach-twice",
        "abort",
        1.0,
        &["healthy"],
    ));
    let _first = instantiate_with_plugins(&loaded, options(Duration::from_micros(500), None, None))
        .expect("the first engine takes the host");
    let mut second = common::example_run().engine;

    let err = second
        .attach_plugins(loaded.plugins.clone())
        .expect_err("the host is in use");

    assert!(matches!(err, EngineError::PluginHostInUse), "{err:?}");
}

/// RR-m2: factories of a run-mode host build only inside the build scope of the engine the host is
/// attached to. Plain `instantiate` on the loaded registry fails with `BehaviorBuild`; the same
/// scenario instantiates with `instantiate_with_plugins`.
#[test]
fn plain_instantiate_with_run_mode_plugin_factories_fails_with_behavior_build() {
    let loaded = load(&plugin_support::fault_scenario(
        "build-scope",
        "abort",
        1.0,
        &["healthy"],
    ));

    let Err(err) = instantiate(
        &loaded.registry,
        &loaded.scenario,
        options(Duration::from_micros(500), None, None),
    ) else {
        panic!("a plain instantiate must not build plugin behaviors");
    };

    assert!(matches!(err, EngineError::BehaviorBuild { .. }), "{err:?}");
    assert!(
        err.to_string()
            .contains("plugin host not attached to this engine"),
        "{err}"
    );
    assert!(
        instantiate_with_plugins(&loaded, options(Duration::from_micros(500), None, None)).is_ok()
    );
}
