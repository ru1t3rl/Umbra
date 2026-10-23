//! M7 WI-12: the replay source (REQ-K-7, REQ-N-1, ASM-628). It passes the network adapter
//! contract, discards publishes, delivers recorded messages at their tick, reproduces recorded
//! faults at their tick, and drives a manager to the same state as the recorded run.

#![forbid(unsafe_code)]
#![expect(clippy::unwrap_used, reason = "test code")]

mod common;
mod tap_support;

use std::collections::BTreeMap;
use std::sync::{Arc, LazyLock};

use umbra_core::ControlAction;
use umbra_core::net::{
    AdapterCounters, KeySpace, NetKey, NetMessage, NetworkAdapter, NetworkAdapterConfig,
};
use umbra_net::{AdapterStatus, LoopbackHub, NetworkManager, ReplaySource};

use common::{FakeWorld, PEER_BASE, eid, settings, slot, state, tick, type_map, view};
use tap_support::{PartialThenFail, RecTap, Seen, log, snapshot};

const KS: KeySpace = KeySpace {
    base: 1000,
    count: 100,
};

fn config_from(source: &Arc<ReplaySource>, name: &str) -> Arc<dyn NetworkAdapterConfig> {
    let input = serde_json::json!({"name": name, "params": {}, "templates": {}});
    source.factory(name, KS).build(&input).unwrap()
}

// The contract suite: the feed pushes into the one source the configs are built from.
mod contract {
    use super::*;

    // One source for the whole module. Only `contract_poll_respects_max` feeds it (once, at tick
    // 0), so sharing it is race-free; a second fed row would have to use its own source.
    static SOURCE: LazyLock<Arc<ReplaySource>> = LazyLock::new(|| Arc::new(ReplaySource::new()));

    fn make() -> Arc<dyn NetworkAdapterConfig> {
        config_from(&SOURCE, "replayed")
    }

    fn feed(_adapter: &mut dyn NetworkAdapter, messages: &[NetMessage]) {
        SOURCE.push(0, "replayed", messages.to_vec());
    }

    umbra_core::network_adapter_contract_tests!(attrs = [], make(), feed = feed);
}

#[test]
fn the_factory_reports_the_recorded_key_space_and_a_valid_schema() {
    let source = Arc::new(ReplaySource::new());
    let factory = source.factory("lb", KS);
    assert!(factory.param_schema().as_json().is_object());
    let input = serde_json::json!({"name": "lb", "params": {"anything": 1}, "templates": {}});
    assert_eq!(factory.build(&input).unwrap().key_space(), KS);
}

#[test]
fn messages_come_out_at_their_tick_only_and_in_recorded_order() {
    let source = Arc::new(ReplaySource::new());
    let (a, b, c) = (
        state(PEER_BASE + 1, None, 1.0),
        state(PEER_BASE + 2, None, 2.0),
        NetMessage::EntityRemoved {
            key: NetKey(PEER_BASE + 1),
        },
    );
    source.push(2, "lb", vec![a.clone()]);
    source.push(2, "lb", vec![b.clone(), c.clone()]);
    source.push(2, "other", vec![c.clone()]);
    let mut adapter = config_from(&source, "lb")
        .start(&common::start_ctx())
        .unwrap();

    let mut got = Vec::new();
    adapter.poll(tick(1), 10, &mut got).unwrap();
    assert_eq!(got, vec![]);
    adapter.poll(tick(2), 2, &mut got).unwrap();
    assert_eq!(got, vec![a, b], "at most max, in recorded order");
    adapter.poll(tick(2), 2, &mut got).unwrap();
    assert_eq!(got.len(), 3);
    assert_eq!(got.last(), Some(&c));
    adapter.poll(tick(3), 10, &mut got).unwrap();
    assert_eq!(got.len(), 3);
}

#[test]
fn publishes_are_discarded_and_open_no_peer() {
    let source = Arc::new(ReplaySource::new());
    let mut adapter = config_from(&source, "lb")
        .start(&common::start_ctx())
        .unwrap();
    let msg = NetMessage::SimulationControl(ControlAction::Pause);

    adapter
        .publish(tick(0), std::slice::from_ref(&msg))
        .unwrap();

    let mut got = Vec::new();
    adapter.poll(tick(0), 10, &mut got).unwrap();
    assert_eq!(got, vec![], "a publish never comes back");
    assert_eq!(adapter.counters(), AdapterCounters::default());
}

#[test]
fn a_recorded_poll_fault_fails_that_tick_only() {
    let source = Arc::new(ReplaySource::new());
    source.fault_at(3, "lb", "poll");
    let mut adapter = config_from(&source, "lb")
        .start(&common::start_ctx())
        .unwrap();
    let mut got = Vec::new();

    assert!(adapter.poll(tick(2), 10, &mut got).is_ok());
    assert!(adapter.poll(tick(3), 10, &mut got).is_err());
    assert!(
        adapter.publish(tick(3), &[]).is_ok(),
        "a poll fault is not a publish fault"
    );
}

#[test]
fn a_recorded_publish_fault_fails_that_tick_only() {
    let source = Arc::new(ReplaySource::new());
    source.fault_at(3, "lb", "publish");
    source.fault_at(3, "lb", "something else");
    let mut adapter = config_from(&source, "lb")
        .start(&common::start_ctx())
        .unwrap();

    assert!(adapter.publish(tick(2), &[]).is_ok());
    assert!(adapter.publish(tick(3), &[]).is_err());
    assert!(adapter.poll(tick(3), 10, &mut Vec::new()).is_ok());
}

#[test]
fn a_recorded_poll_fault_still_delivers_what_the_failing_poll_left_behind() {
    let source = Arc::new(ReplaySource::new());
    let m = NetMessage::EntityRemoved { key: NetKey(5) };
    source.push(4, "cut", vec![m.clone()]);
    source.fault_at(4, "cut", "poll");
    let mut adapter = config_from(&source, "cut")
        .start(&common::start_ctx())
        .unwrap();
    let mut got = Vec::new();

    assert!(adapter.poll(tick(4), 10, &mut got).is_err());

    assert_eq!(got, vec![m], "the live manager applied this batch");
}

#[test]
fn messages_of_past_ticks_are_never_delivered() {
    let source = Arc::new(ReplaySource::new());
    source.push(
        2,
        "lb",
        vec![
            state(PEER_BASE + 1, None, 1.0),
            state(PEER_BASE + 2, None, 2.0),
            state(PEER_BASE + 3, None, 3.0),
        ],
    );
    let mut adapter = config_from(&source, "lb")
        .start(&common::start_ctx())
        .unwrap();
    let mut got = Vec::new();

    adapter.poll(tick(2), 1, &mut got).unwrap();
    assert_eq!(got.len(), 1);
    adapter.poll(tick(3), 10, &mut got).unwrap();

    assert_eq!(
        got.len(),
        1,
        "the two left over at tick 2 are gone at tick 3"
    );
}

#[test]
fn a_recorded_control_fault_fails_run_control_publishes_only() {
    let source = Arc::new(ReplaySource::new());
    source.fault_at(3, "lb", "control");
    let mut adapter = config_from(&source, "lb")
        .start(&common::start_ctx())
        .unwrap();
    let control = NetMessage::SimulationControl(ControlAction::Pause);
    let entity = state(PEER_BASE + 1, None, 1.0);

    assert!(
        adapter
            .publish(tick(3), std::slice::from_ref(&entity))
            .is_ok()
    );
    assert!(
        adapter
            .publish(tick(3), std::slice::from_ref(&control))
            .is_err()
    );
    assert!(
        adapter
            .publish(tick(2), std::slice::from_ref(&control))
            .is_ok()
    );
}

#[test]
fn a_recorded_publish_fault_does_not_fail_run_control_publishes() {
    let source = Arc::new(ReplaySource::new());
    source.fault_at(3, "lb", "publish");
    let mut adapter = config_from(&source, "lb")
        .start(&common::start_ctx())
        .unwrap();
    let control = NetMessage::SimulationControl(ControlAction::Pause);

    assert!(
        adapter
            .publish(tick(3), std::slice::from_ref(&control))
            .is_ok()
    );
    assert!(
        adapter
            .publish(tick(3), &[state(PEER_BASE + 1, None, 1.0)])
            .is_err()
    );
}

#[test]
fn a_manager_reproduces_a_recorded_fault_at_its_tick() {
    let source = Arc::new(ReplaySource::new());
    source.fault_at(2, "lb", "poll");
    let mut manager = NetworkManager::new(
        vec![slot(settings("lb"), config_from(&source, "lb"), type_map())],
        tick(0),
    )
    .unwrap();
    let mut world = FakeWorld::default();

    manager.ingest(tick(1), &mut world, &mut Vec::new());
    assert!(!manager.status()[0].faulted);
    manager.ingest(tick(2), &mut world, &mut Vec::new());

    assert!(manager.status()[0].faulted);
}

/// Records a run of two adapters of one kind (ASM-410) with the tap, replays it from the source,
/// and compares ghosts and statuses tick by tick. Adapter `a` is a loopback with a peer whose
/// publish fails at tick 4 (the hub closes between ingest and publish); adapter `b` delivers a
/// state at tick 3 and fails that same poll. Counters and `fault_reason` are zeroed on both
/// sides: the replay adapter has no queues or sockets, and `FaultRecord` carries no reason text
/// (only whether there is one is compared).
#[test]
fn a_replayed_run_of_two_adapters_matches_the_recorded_run() {
    const KS_A: KeySpace = KeySpace {
        base: 1000,
        count: 100,
    };
    const KS_B: KeySpace = KeySpace {
        base: 3000,
        count: 100,
    };
    let hub = LoopbackHub::new(64);
    let cfg_a = common::config(&hub, "test.net.a", KS_A.base, &serde_json::json!({}));
    let mut peer = common::peer_on(&hub, "test.net.peer");
    let cfg_b: Arc<dyn NetworkAdapterConfig> = Arc::new(PartialThenFail {
        message: state(PEER_BASE + 9, Some("tank"), 9.0),
        fail_from: 3,
        key_space: KS_B,
    });
    let mut live = NetworkManager::new(
        vec![
            slot(settings("a"), cfg_a, type_map()),
            slot(settings("b"), cfg_b, type_map()),
        ],
        tick(0),
    )
    .unwrap();
    let seen = log();
    live.set_tap(Some(Box::new(RecTap(seen.clone()))));
    let mut world = FakeWorld::default();
    let mut recorded = Vec::new();
    for n in 0..8u32 {
        let t = tick(u64::from(n));
        if n <= 4 {
            peer.publish(
                t,
                &[state(
                    PEER_BASE + 1 + u64::from(n % 3),
                    Some("tank"),
                    f64::from(n),
                )],
            )
            .unwrap();
        }
        live.ingest(t, &mut world, &mut Vec::new());
        if n == 4 {
            hub.close();
        }
        live.publish(t, &[view(eid(1), f64::from(n))], &BTreeMap::new(), &[]);
        recorded.push((world.ghosts.clone(), normalised(live.status())));
    }

    let source = Arc::new(ReplaySource::new());
    for ev in snapshot(&seen) {
        match ev {
            Seen::Polled {
                tick,
                adapter,
                messages,
            } => source.push(tick, &adapter, messages),
            Seen::Faulted {
                tick,
                adapter,
                call,
            } => source.fault_at(tick, &adapter, &call),
            Seen::WorldCall(_) => {}
        }
    }
    let mut replay = NetworkManager::new(
        vec![
            slot(settings("a"), config_for(&source, "a", KS_A), type_map()),
            slot(settings("b"), config_for(&source, "b", KS_B), type_map()),
        ],
        tick(0),
    )
    .unwrap();
    let mut world = FakeWorld::default();
    let mut replayed = Vec::new();
    for n in 0..8u32 {
        let t = tick(u64::from(n));
        replay.ingest(t, &mut world, &mut Vec::new());
        replay.publish(t, &[view(eid(1), f64::from(n))], &BTreeMap::new(), &[]);
        replayed.push((world.ghosts.clone(), normalised(replay.status())));
    }

    assert!(
        recorded
            .last()
            .is_some_and(|(_, s)| s.iter().all(|a| a.faulted)),
        "both faulted"
    );
    for (n, (live, replayed)) in recorded.into_iter().zip(replayed).enumerate() {
        assert_eq!(live.0, replayed.0, "ghosts at tick {n}");
        assert_eq!(live.1, replayed.1, "status at tick {n}");
    }
}

/// A run-control publish fault after a successful entity publish at the same tick: the replay
/// must fail only the control call, so `published` matches at every tick (review round 2).
#[test]
fn a_replayed_control_fault_after_a_good_publish_matches_the_recorded_run() {
    const KS_A: KeySpace = KeySpace {
        base: 1000,
        count: 100,
    };
    let mut with_control = settings("a");
    with_control.publish.control = true;
    let hub = LoopbackHub::new(64);
    let cfg = common::config(&hub, "test.net.a", KS_A.base, &serde_json::json!({}));
    let mut live =
        NetworkManager::new(vec![slot(with_control.clone(), cfg, type_map())], tick(0)).unwrap();
    let seen = log();
    live.set_tap(Some(Box::new(RecTap(seen.clone()))));
    let mut world = FakeWorld::default();
    let mut recorded = Vec::new();
    for n in 0..6u32 {
        let t = tick(u64::from(n));
        live.ingest(t, &mut world, &mut Vec::new());
        live.publish(t, &[view(eid(1), f64::from(n))], &BTreeMap::new(), &[]);
        if n == 3 {
            hub.close();
        }
        live.publish_control(t, ControlAction::Pause);
        recorded.push(normalised(live.status()));
    }
    assert!(
        recorded[3][0].faulted && recorded[3][0].published > 0,
        "{:?}",
        recorded[3]
    );

    let source = Arc::new(ReplaySource::new());
    for ev in snapshot(&seen) {
        match ev {
            Seen::Polled {
                tick,
                adapter,
                messages,
            } => source.push(tick, &adapter, messages),
            Seen::Faulted {
                tick,
                adapter,
                call,
            } => source.fault_at(tick, &adapter, &call),
            Seen::WorldCall(_) => {}
        }
    }
    let mut replay = NetworkManager::new(
        vec![slot(
            with_control,
            config_for(&source, "a", KS_A),
            type_map(),
        )],
        tick(0),
    )
    .unwrap();
    let mut world = FakeWorld::default();
    for (n, want) in recorded.iter().enumerate() {
        let t = tick(n as u64);
        replay.ingest(t, &mut world, &mut Vec::new());
        replay.publish(
            t,
            &[view(eid(1), f64::from(u32::try_from(n).unwrap()))],
            &BTreeMap::new(),
            &[],
        );
        replay.publish_control(t, ControlAction::Pause);
        assert_eq!(&normalised(replay.status()), want, "status at tick {n}");
    }
}

fn config_for(
    source: &Arc<ReplaySource>,
    name: &str,
    ks: KeySpace,
) -> Arc<dyn NetworkAdapterConfig> {
    let input = serde_json::json!({"name": name, "params": {}, "templates": {}});
    source.factory(name, ks).build(&input).unwrap()
}

/// Zeroes what differs by design; the reason text becomes "some reason".
fn normalised(mut status: Vec<AdapterStatus>) -> Vec<AdapterStatus> {
    for s in &mut status {
        s.counters = AdapterCounters::default();
        s.fault_reason = s.fault_reason.take().map(|_| "reason".to_owned());
    }
    status
}
