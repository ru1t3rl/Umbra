//! M7 WI-12: the inbound tap (REQ-K-7, ASM-627). The tap sees each enabled adapter's poll result
//! in scenario order and before the manager processes it, and every fault with the failing call;
//! and it cannot change what the manager does.

#![forbid(unsafe_code)]

mod common;
mod tap_support;

use std::sync::Arc;

use proptest::prelude::*;
use umbra_core::ControlAction;
use umbra_core::net::{KeySpace, NetKey, NetMessage, NetworkAdapterConfig};
use umbra_net::NetworkManager;

use common::{
    FakeWorld, PEER_BASE, Rig, eid, settings, slot, state, tick, two_adapters, type_map, view,
};
use tap_support::{LoggingWorld, PartialThenFail, RecTap, Seen, log, snapshot};

fn polled(tick: u64, adapter: &str, messages: Vec<NetMessage>) -> Seen {
    Seen::Polled {
        tick,
        adapter: adapter.to_owned(),
        messages,
    }
}

fn faulted(tick: u64, adapter: &str, call: &str) -> Seen {
    Seen::Faulted {
        tick,
        adapter: adapter.to_owned(),
        call: call.to_owned(),
    }
}

#[test]
fn the_tap_sees_each_adapters_poll_in_scenario_order_before_it_is_processed() {
    let (mut manager, mut peers) = two_adapters();
    let seen = log();
    manager.set_tap(Some(Box::new(RecTap(seen.clone()))));
    let m0 = state(2001, Some("tank"), 10.0);
    let m1 = state(2002, Some("tank"), 20.0);
    peers[0]
        .publish(tick(3), std::slice::from_ref(&m0))
        .unwrap();
    peers[1]
        .publish(tick(3), std::slice::from_ref(&m1))
        .unwrap();
    let mut world = LoggingWorld(FakeWorld::default(), seen.clone());

    manager.ingest(tick(3), &mut world, &mut Vec::new());

    assert_eq!(
        snapshot(&seen),
        vec![
            polled(3, "lb0", vec![m0]),
            Seen::WorldCall("create"),
            polled(3, "lb1", vec![m1]),
            Seen::WorldCall("create"),
        ]
    );
}

#[test]
fn an_empty_poll_is_reported_as_an_empty_batch() {
    let mut rig = Rig::new();
    let seen = log();
    rig.manager.set_tap(Some(Box::new(RecTap(seen.clone()))));

    rig.ingest(7);

    assert_eq!(snapshot(&seen), vec![polled(7, "lb", vec![])]);
}

#[test]
fn a_disabled_adapter_is_not_reported_and_removing_the_tap_stops_reports() {
    let mut rig = Rig::new();
    let seen = log();
    rig.manager.set_tap(Some(Box::new(RecTap(seen.clone()))));
    rig.manager.set_enabled("lb", false).unwrap();
    rig.ingest(0);
    assert_eq!(snapshot(&seen), vec![], "a disabled adapter is not polled");

    rig.manager.set_enabled("lb", true).unwrap();
    rig.ingest(1);
    rig.manager.set_tap(None);
    rig.ingest(2);
    assert_eq!(snapshot(&seen), vec![polled(1, "lb", vec![])]);
}

#[test]
fn a_poll_fault_is_reported_once_with_its_tick_and_call() {
    let mut rig = Rig::new();
    let seen = log();
    rig.manager.set_tap(Some(Box::new(RecTap(seen.clone()))));
    rig.ingest(0);
    rig.hub.close();

    rig.ingest(1);
    rig.ingest(2);

    assert_eq!(
        snapshot(&seen),
        vec![polled(0, "lb", vec![]), faulted(1, "lb", "poll")],
        "the faulted adapter is disabled, so tick 2 reports nothing"
    );
}

#[test]
fn a_publish_fault_is_reported_with_the_publish_call() {
    let mut rig = Rig::new();
    let seen = log();
    rig.manager.set_tap(Some(Box::new(RecTap(seen.clone()))));
    rig.hub.close();

    rig.publish(5, &[view(eid(1), 0.0)]);

    assert_eq!(snapshot(&seen), vec![faulted(5, "lb", "publish")]);
}

#[test]
fn a_run_control_publish_fault_is_reported_with_the_control_call() {
    let mut s = settings("lb");
    s.publish.control = true;
    let mut rig = Rig::with(&serde_json::json!({}), s, type_map());
    let seen = log();
    rig.manager.set_tap(Some(Box::new(RecTap(seen.clone()))));
    rig.hub.close();

    rig.manager.publish_control(tick(6), ControlAction::Pause);

    assert_eq!(snapshot(&seen), vec![faulted(6, "lb", "control")]);
}

#[test]
fn messages_a_failing_poll_left_behind_are_reported_before_the_fault() {
    let cfg: Arc<dyn NetworkAdapterConfig> = Arc::new(PartialThenFail {
        message: NetMessage::EntityRemoved {
            key: NetKey(PEER_BASE),
        },
        fail_from: 0,
        key_space: KeySpace { base: 0, count: 10 },
    });
    let mut manager =
        NetworkManager::new(vec![slot(settings("cut"), cfg, type_map())], tick(0)).unwrap();
    let seen = log();
    manager.set_tap(Some(Box::new(RecTap(seen.clone()))));

    manager.ingest(tick(4), &mut FakeWorld::default(), &mut Vec::new());

    assert_eq!(
        snapshot(&seen),
        vec![
            polled(
                4,
                "cut",
                vec![NetMessage::EntityRemoved {
                    key: NetKey(PEER_BASE)
                }]
            ),
            faulted(4, "cut", "poll"),
        ]
    );
}

/// One round of a loopback run: what the peer sends and what the engine owns.
#[derive(Debug, Clone)]
struct Round {
    /// `(key offset, y, remove)`.
    sends: Vec<(u64, f64, bool)>,
    owned_y: Vec<f64>,
}

/// The rounds and, maybe, the round at whose start the hub closes (every adapter call then
/// faults, so the fault paths of ingest, publish and `publish_control` run).
fn runs() -> impl Strategy<Value = (Vec<Round>, Option<usize>)> {
    rounds().prop_flat_map(|r| {
        let n = r.len();
        (Just(r), prop::option::of(0..n))
    })
}

fn rounds() -> impl Strategy<Value = Vec<Round>> {
    let send = (1u64..5, -200.0..200.0f64, any::<bool>());
    let round = (
        prop::collection::vec(send, 0..4),
        prop::collection::vec(-200.0..200.0f64, 0..3),
    )
        .prop_map(|(sends, owned_y)| Round { sends, owned_y });
    prop::collection::vec(round, 1..14)
}

/// Runs the rounds and returns everything observable.
fn run(rounds: &[Round], close_at: Option<usize>, tapped: bool) -> Vec<String> {
    let mut control = settings("lb");
    control.publish.control = true;
    let mut rig = Rig::with(&serde_json::json!({}), control, type_map());
    let seen = log();
    if tapped {
        rig.manager.set_tap(Some(Box::new(RecTap(seen))));
    }
    let mut out = Vec::new();
    for (i, round) in rounds.iter().enumerate() {
        let n = i as u64 * 10;
        if close_at == Some(i) {
            rig.hub.close();
        }
        let msgs: Vec<NetMessage> = round
            .sends
            .iter()
            .map(|&(k, y, remove)| {
                if remove {
                    NetMessage::EntityRemoved {
                        key: NetKey(PEER_BASE + k),
                    }
                } else {
                    state(PEER_BASE + k, Some("tank"), y)
                }
            })
            .collect();
        // The raw peer's own endpoint is closed with the hub.
        let open = close_at.is_none_or(|k| i < k);
        if open {
            rig.peer_sends(n, &msgs);
        }
        let events = rig.ingest(n);
        let owned: Vec<_> = round
            .owned_y
            .iter()
            .enumerate()
            .map(|(j, &y)| view(eid(j as u64 + 1), y))
            .collect();
        rig.publish(n, &owned);
        rig.manager.publish_control(tick(n), ControlAction::Pause);
        let peer = if open { rig.peer_polls(n) } else { Vec::new() };
        out.push(format!(
            "{events:?} {:?} {:?} {peer:?}",
            rig.world.ghosts,
            rig.status(),
        ));
    }
    out
}

proptest! {
    #[test]
    fn a_manager_with_a_tap_behaves_exactly_like_one_without((rounds, close_at) in runs()) {
        prop_assert_eq!(run(&rounds, close_at, true), run(&rounds, close_at, false));
    }
}
