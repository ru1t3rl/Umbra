//! AT-18 (M7 compatibility): scenarios that use none of the new features are untouched by M7: no
//! lifecycle intents reach the report, the spawner does nothing, no recorder is attached, the
//! entity set is constant. Requirements: REQ-K-4, ADR 0052, ADR 0053, ASM-605; the goldens
//! themselves are checked by `scripts/gates.sh`, `handle_determinism.rs` and `replay.rs`.
//!
//! Written in M7 WI-1; these rows pass on the stubs and stay green through M7.

#![forbid(unsafe_code)]

mod common;
mod m7_support;

use core::ops::ControlFlow;
use core::time::Duration;

use common::options;
use m7_support::{example, launcher_registry};
use umbra_engine::scenario::{instantiate, load_scenario};
use umbra_engine::{LifecycleCounters, RunUntil};

/// ADR 0052: a scenario that never emits `Spawn` or `Despawn` has an empty lifecycle list on every
/// tick, a constant entity set, zero lifecycle counters and no recording.
#[test]
fn scenarios_without_new_features_have_no_spawner_effect_or_recorder() {
    for name in ["walking_skeleton", "patrol_and_engage"] {
        let registry = launcher_registry();
        let scenario = load_scenario(&registry, &example(name)).expect("load");
        let mut engine = instantiate(
            &registry,
            &scenario,
            options(Duration::from_micros(500), None, None),
        )
        .expect("instantiate")
        .engine;
        let initial = engine.snapshot().entities.len();
        let mut counts = Vec::new();
        let mut lifecycle_seen = 0_usize;

        engine
            .run(RunUntil::Ticks(600), |view| {
                counts.push(view.snapshot().entities.len());
                lifecycle_seen += view.report.lifecycle.len();
                ControlFlow::Continue(())
            })
            .expect("run");

        assert_eq!(
            counts,
            vec![initial; 600],
            "{name}: the entity set is constant"
        );
        assert_eq!(lifecycle_seen, 0, "{name}: no lifecycle intents");
        assert_eq!(
            engine.lifecycle_counters(),
            LifecycleCounters::default(),
            "{name}"
        );
        assert_eq!(engine.recording(), None, "{name}");
    }
}
