//! Run-control publishing at engine level (M6 WI-9, ASM-244): `Engine::publish_control` sends a
//! `SimulationControl` only on adapters whose `[publish] control` is true.

#![forbid(unsafe_code)]

mod net_support;

use net_support::{registry_with, start};
use umbra_core::ControlAction;
use umbra_core::net::NetMessage;
use umbra_net::LoopbackHub;

fn controls(hub: &LoopbackHub) -> Vec<ControlAction> {
    hub.captured()
        .into_iter()
        .filter_map(|(_, m)| match m {
            NetMessage::SimulationControl(a) => Some(a),
            _ => None,
        })
        .collect()
}

#[test]
fn control_is_published_when_the_adapter_asks_for_it() {
    let hub = LoopbackHub::with_capture(64);
    let registry = registry_with(&[("test.net.loopback", &hub, 1000)]);
    let mut run = start("control", &registry, None);

    run.engine.publish_control(ControlAction::Pause);

    assert_eq!(controls(&hub), vec![ControlAction::Pause]);
}

#[test]
fn control_is_not_published_by_default() {
    let hub = LoopbackHub::with_capture(64);
    let registry = registry_with(&[("test.net.loopback", &hub, 1000)]);
    let mut run = start("a", &registry, None);

    run.engine.publish_control(ControlAction::Pause);

    assert_eq!(controls(&hub), Vec::<ControlAction>::new());
}

/// The sequence `umbra run` follows: Start before the steps, Stop after them.
#[test]
fn a_run_announces_start_and_stop_around_its_steps() {
    let hub = LoopbackHub::with_capture(64);
    let registry = registry_with(&[("test.net.loopback", &hub, 1000)]);
    let mut run = start("control", &registry, None);

    assert_eq!(run.engine.publish_control(ControlAction::Start), None);
    run.engine
        .step(3, |_| std::ops::ControlFlow::Continue(()))
        .expect("step");
    assert_eq!(run.engine.publish_control(ControlAction::Stop), None);

    assert_eq!(
        controls(&hub),
        vec![ControlAction::Start, ControlAction::Stop]
    );
}
