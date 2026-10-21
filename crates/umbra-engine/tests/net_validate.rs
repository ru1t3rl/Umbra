//! Validation builds the adapters' configurations and never starts them (M6 WI-9, ASM-251).
//! Requirements: REQ-T-6, ADR 0030.

#![forbid(unsafe_code)]

mod net_support;

use net_support::{fixture, registry_with, start};
use umbra_engine::scenario::validate_path;
use umbra_net::LoopbackHub;

#[test]
fn validate_checks_the_adapter_without_starting_it() {
    let hub = LoopbackHub::new(16);
    let registry = registry_with(&[("test.net.loopback", &hub, 1000)]);

    let report = validate_path(&registry, &fixture("a")).expect("validate");

    assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
    assert_eq!(hub.starts(), 0, "validation opens nothing");
}

#[test]
fn validate_flags_an_adapter_that_is_not_registered() {
    let hub = LoopbackHub::new(16);
    let registry = registry_with(&[("test.net.other", &hub, 1000)]);

    let report = validate_path(&registry, &fixture("a")).expect("validate");

    assert_ne!(
        report.diagnostics.len(),
        0,
        "the unknown adapter is reported"
    );
    assert_eq!(hub.starts(), 0);
}

#[test]
fn instantiating_starts_each_adapter_once() {
    let hub = LoopbackHub::new(16);
    let registry = registry_with(&[("test.net.loopback", &hub, 1000)]);

    let run = start("a", &registry, None);

    assert_eq!(hub.starts(), 1);
    assert!(run.engine.network().is_some());
}

#[test]
fn the_end_of_run_summary_names_the_adapter_and_its_counters() {
    let hub = LoopbackHub::new(16);
    let registry = registry_with(&[("test.net.loopback", &hub, 1000)]);
    let mut run = start("a", &registry, None);
    run.engine
        .step(3, |_| std::ops::ControlFlow::Continue(()))
        .expect("step");

    let status = run.engine.network().expect("link").status();
    let line = umbra_engine::net_summary(&status[0]);

    assert!(line.starts_with("info: net lb: published="), "{line}");
    assert!(status[0].published > 0, "the tank was published");
    assert!(
        line.contains(&format!("published={} ", status[0].published)),
        "{line}"
    );
    assert!(line.contains(" decode_errors=0 "), "{line}");
}

/// REQ-T-4, ADR 0030: the factory's input is `{"name", "params", "templates"}` with the templates'
/// `network.<name>` tables, both in the validation trial build and in the engine's build.
#[test]
fn an_adapter_receives_its_templates_network_tables() {
    let hub = LoopbackHub::new(16);
    let registry = registry_with(&[("test.net.loopback", &hub, 1000)]);
    let expected = serde_json::json!({
        "name": "lb",
        "params": {},
        "templates": { "tank.mbt_generic": { "entity_type": "tank" } },
    });

    let report = validate_path(&registry, &fixture("a")).expect("validate");
    assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
    let after_validate = hub.built_inputs();
    let _run = start("a", &registry, None);
    let after_start = hub.built_inputs();

    assert!(
        after_validate.contains(&expected),
        "validation: {after_validate:?}"
    );
    assert_eq!(
        after_start.last(),
        Some(&expected),
        "engine: {after_start:?}"
    );
}
