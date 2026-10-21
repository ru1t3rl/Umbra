//! AT-21 (loopback row): the `umbra_core::net::contract` suite on the loopback adapter, plus the
//! loopback semantics the other tests rely on. Requirements: REQ-N-1 (LSP), ADR 0030.
//!
//! Written in step 4 of M6, before WI-7; run since WI-7.

#![forbid(unsafe_code)]

mod common;

use std::sync::Arc;

use umbra_core::ControlAction;
use umbra_core::net::{KeySpace, NetMessage, NetworkAdapter, NetworkAdapterConfig};
use umbra_net::LoopbackHub;

use common::{cid, config, peer_on, start_ctx, tick};

fn echoing_config() -> Arc<dyn NetworkAdapterConfig> {
    let hub = LoopbackHub::new(16);
    config(
        &hub,
        "test.net.loopback",
        0,
        &serde_json::json!({"echo": true}),
    )
}

// The contract suite, fed through the loopback's `echo` parameter (ASM-280).
mod contract {
    use super::{NetMessage, NetworkAdapter, echoing_config, tick};

    fn feed(adapter: &mut dyn NetworkAdapter, messages: &[NetMessage]) {
        adapter
            .publish(tick(0), messages)
            .unwrap_or_else(|e| panic!("feed publish failed: {e}"));
    }

    umbra_core::network_adapter_contract_tests!(attrs = [], echoing_config(), feed = feed);
}

fn pause() -> NetMessage {
    NetMessage::SimulationControl(ControlAction::Pause)
}

#[test]
fn a_published_message_reaches_the_other_endpoints_but_not_the_sender() {
    let hub = LoopbackHub::new(16);
    let mut a = config(&hub, "test.net.a", 0, &serde_json::json!({}))
        .start(&start_ctx())
        .unwrap();
    let mut b = peer_on(&hub, "test.net.b");
    let mut c = peer_on(&hub, "test.net.c");

    a.publish(tick(0), &[pause()]).unwrap();
    let (mut at_a, mut at_b, mut at_c) = (Vec::new(), Vec::new(), Vec::new());
    a.poll(tick(0), 10, &mut at_a).unwrap();
    b.poll(tick(0), 10, &mut at_b).unwrap();
    c.poll(tick(0), 10, &mut at_c).unwrap();

    assert_eq!((at_a, at_b, at_c), (vec![], vec![pause()], vec![pause()]));
}

#[test]
fn the_echo_parameter_also_delivers_to_the_sender() {
    let mut a = echoing_config().start(&start_ctx()).unwrap();

    a.publish(tick(0), &[pause()]).unwrap();
    let mut at_a = Vec::new();
    a.poll(tick(0), 10, &mut at_a).unwrap();

    assert_eq!(at_a, vec![pause()]);
}

#[test]
fn a_full_queue_drops_the_excess_and_counts_it_at_the_receiver() {
    let hub = LoopbackHub::new(2);
    let mut a = config(&hub, "test.net.a", 0, &serde_json::json!({}))
        .start(&start_ctx())
        .unwrap();
    let mut b = peer_on(&hub, "test.net.b");

    a.publish(tick(0), &[pause(), pause(), pause(), pause(), pause()])
        .unwrap();
    let mut at_b = Vec::new();
    b.poll(tick(0), 10, &mut at_b).unwrap();

    assert_eq!(at_b.len(), 2);
    assert_eq!(b.counters().rx_dropped, 3);
}

#[test]
fn the_key_space_param_overrides_the_factory_default() {
    let hub = LoopbackHub::new(16);
    let factory = hub.factory(cid("test.net.loopback"), KeySpace { base: 5, count: 6 });

    let default = factory.build(&serde_json::json!({})).unwrap();
    let overridden = factory
        .build(&serde_json::json!({"key_space": [100, 20]}))
        .unwrap();

    assert_eq!(default.key_space(), KeySpace { base: 5, count: 6 });
    assert_eq!(
        overridden.key_space(),
        KeySpace {
            base: 100,
            count: 20
        }
    );
}

#[test]
fn captured_lists_every_published_message_in_publish_order() {
    let hub = LoopbackHub::with_capture(16);
    let mut a = config(&hub, "test.net.a", 0, &serde_json::json!({}))
        .start(&start_ctx())
        .unwrap();
    let mut b = peer_on(&hub, "test.net.b");
    let resume = NetMessage::SimulationControl(ControlAction::Resume);

    a.publish(tick(0), &[pause()]).unwrap();
    b.publish(tick(1), std::slice::from_ref(&resume)).unwrap();

    // Each message carries the key-space base of its publisher (a: 0, the peer: 2000).
    assert_eq!(hub.captured(), vec![(0, pause()), (2000, resume)]);
}

#[test]
fn a_hub_without_capture_records_nothing() {
    let hub = LoopbackHub::new(16);
    let mut a = config(&hub, "test.net.a", 0, &serde_json::json!({}))
        .start(&start_ctx())
        .unwrap();

    a.publish(tick(0), &[pause()]).unwrap();

    assert_eq!(hub.captured(), vec![]);
}

#[test]
fn starting_an_endpoint_is_counted_by_the_hub() {
    let hub = LoopbackHub::new(16);
    let cfg = config(&hub, "test.net.a", 0, &serde_json::json!({}));
    assert_eq!(hub.starts(), 0, "building does not start");

    let mut started = cfg.start(&start_ctx()).unwrap();
    started.stop();

    assert_eq!(hub.starts(), 1);
}
