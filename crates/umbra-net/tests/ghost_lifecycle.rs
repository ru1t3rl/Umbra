//! AT-13 (unit level): the ghost lifecycle in the manager. Requirements: REQ-N-1..5, REQ-K-5,
//! ADR 0030, ADR 0031, ASM-233, ASM-236, ASM-240, ASM-243, ASM-248, ASM-235, ASM-269.
//!
//! Written in step 4 of M6, before WI-7; run since WI-7. Time rule pinned here (ASM-269): a ghost is removed at the
//! first ingest whose time is strictly more than `timeout` after its last update.

#![forbid(unsafe_code)]

mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use umbra_core::net::{
    AdapterCounters, AdapterStart, KeySpace, NetError, NetKey, NetMessage, NetworkAdapter,
    NetworkAdapterConfig,
};
use umbra_core::{ControlAction, Force, RemoteSource, Seconds, SimEvent, WeaponId};
use umbra_net::{GhostPolicy, NetworkManager};

use common::{
    PEER_BASE, Rig, at, eid, settings, state, tick, tid, type_map, type_map_with_default,
};

fn one_ghost_id(rig: &Rig) -> umbra_core::EntityId {
    assert_eq!(rig.world.ghosts.len(), 1, "exactly one ghost");
    *rig.world
        .ghosts
        .keys()
        .next()
        .unwrap_or_else(|| unreachable!())
}

#[test]
fn ghost_is_created_with_the_mapped_template_force_name_and_source() {
    let mut rig = Rig::new();
    rig.peer_sends(0, &[state(PEER_BASE + 1, Some("tank"), 50.0)]);

    let events = rig.ingest(0);

    let id = one_ghost_id(&rig);
    let ghost = &rig.world.ghosts[&id];
    assert_eq!(ghost.template, tid("tank.mbt"));
    assert_eq!(ghost.force, Force::new("red").ok());
    assert_eq!(ghost.name.as_deref(), Some("peer-2001"));
    assert_eq!(ghost.source, RemoteSource::new("lb"));
    assert_eq!(ghost.remote.state.position, at(50.0));
    assert_eq!(
        events,
        vec![SimEvent::EntityCreated {
            entity: id,
            template: Some(tid("tank.mbt"))
        }]
    );
    assert_eq!(rig.status().ghosts, 1);
}

#[test]
fn a_second_state_of_the_same_key_updates_the_ghost_without_creating_another() {
    let mut rig = Rig::new();
    rig.peer_sends(0, &[state(PEER_BASE + 1, Some("tank"), 50.0)]);
    rig.ingest(0);
    let id = one_ghost_id(&rig);
    rig.peer_sends(1, &[state(PEER_BASE + 1, Some("tank"), 80.0)]);

    let events = rig.ingest(1);

    assert_eq!(one_ghost_id(&rig), id);
    assert_eq!(rig.world.created, 1);
    assert_eq!(rig.world.updated, 1);
    assert_eq!(rig.world.ghosts[&id].remote.state.position, at(80.0));
    assert_eq!(events, vec![]);
}

#[test]
fn entity_removed_removes_the_ghost_and_reports_it_once() {
    let mut rig = Rig::new();
    rig.peer_sends(0, &[state(PEER_BASE + 1, Some("tank"), 50.0)]);
    rig.ingest(0);
    let id = one_ghost_id(&rig);
    rig.peer_sends(
        1,
        &[
            NetMessage::EntityRemoved {
                key: NetKey(PEER_BASE + 1),
            },
            NetMessage::EntityRemoved {
                key: NetKey(PEER_BASE + 1),
            },
        ],
    );

    let events = rig.ingest(1);

    assert!(rig.world.ghosts.is_empty());
    assert_eq!(events, vec![SimEvent::EntityRemoved { entity: id }]);
    assert_eq!(rig.status().ghosts, 0);
}

#[test]
fn a_ghost_goes_at_the_first_ingest_strictly_past_the_timeout() {
    // timeout 1 s at 30 Hz: updated at tick 0, still alive at tick 30, gone at tick 31.
    let mut rig = Rig::new();
    rig.peer_sends(0, &[state(PEER_BASE + 1, Some("tank"), 50.0)]);
    rig.ingest(0);
    let id = one_ghost_id(&rig);

    let at_timeout = rig.ingest(30);
    assert_eq!(rig.world.ghosts.len(), 1, "alive exactly at the timeout");
    assert_eq!(at_timeout, vec![]);
    let past = rig.ingest(31);

    assert!(rig.world.ghosts.is_empty());
    assert_eq!(past, vec![SimEvent::EntityRemoved { entity: id }]);
}

#[test]
fn an_update_restarts_the_timeout() {
    let mut rig = Rig::new();
    rig.peer_sends(0, &[state(PEER_BASE + 1, Some("tank"), 50.0)]);
    rig.ingest(0);
    rig.peer_sends(20, &[state(PEER_BASE + 1, Some("tank"), 60.0)]);
    rig.ingest(20);

    rig.ingest(31);
    assert_eq!(
        rig.world.ghosts.len(),
        1,
        "31 ticks after the first state, 11 after the update"
    );
    rig.ingest(51);

    assert!(rig.world.ghosts.is_empty(), "51 > 20 + 30");
}

#[test]
fn ghosts_beyond_max_are_refused_and_counted() {
    let mut s = settings("lb");
    s.ghosts = GhostPolicy { max: 2, ..s.ghosts };
    let mut rig = Rig::with(&serde_json::json!({}), s, type_map());
    rig.peer_sends(
        0,
        &[
            state(PEER_BASE + 1, Some("tank"), 1.0),
            state(PEER_BASE + 2, Some("tank"), 2.0),
            state(PEER_BASE + 3, Some("tank"), 3.0),
        ],
    );

    rig.ingest(0);

    assert_eq!(rig.world.ghosts.len(), 2);
    assert_eq!(rig.status().ghosts_capped, 1);
    assert_eq!(rig.status().ghosts, 2);
}

#[test]
fn max_inbound_per_tick_leaves_the_rest_queued_for_the_next_ingest() {
    let mut s = settings("lb");
    s.ghosts = GhostPolicy {
        max_inbound_per_tick: 2,
        ..s.ghosts
    };
    let mut rig = Rig::with(&serde_json::json!({}), s, type_map());
    let batch: Vec<_> = (1..=5_u32)
        .map(|n| state(PEER_BASE + u64::from(n), Some("tank"), 10.0 * f64::from(n)))
        .collect();
    rig.peer_sends(0, &batch);

    let counts: Vec<usize> = (0..3)
        .map(|t| {
            rig.ingest(t);
            rig.world.ghosts.len()
        })
        .collect();

    assert_eq!(counts, [2, 4, 5]);
}

#[test]
fn unmapped_types_are_not_mirrored_and_are_counted() {
    let mut rig = Rig::new();
    rig.peer_sends(
        0,
        &[
            state(PEER_BASE + 1, Some("helicopter"), 1.0),
            state(PEER_BASE + 2, None, 2.0),
        ],
    );

    let events = rig.ingest(0);

    assert!(rig.world.ghosts.is_empty());
    assert_eq!(events, vec![]);
    assert_eq!(rig.status().unmapped, 2);
}

#[test]
fn unmapped_types_use_the_default_template_when_there_is_one() {
    let mut rig = Rig::with(
        &serde_json::json!({}),
        settings("lb"),
        type_map_with_default("truck.cargo"),
    );
    rig.peer_sends(0, &[state(PEER_BASE + 1, Some("helicopter"), 1.0)]);

    rig.ingest(0);

    let id = one_ghost_id(&rig);
    assert_eq!(rig.world.ghosts[&id].template, tid("truck.cargo"));
    assert_eq!(rig.status().unmapped, 0);
}

#[test]
fn a_disabled_adapter_is_neither_polled_nor_published_to_and_its_ghosts_time_out() {
    let mut rig = Rig::new();
    rig.peer_sends(0, &[state(PEER_BASE + 1, Some("tank"), 50.0)]);
    rig.ingest(0);
    let id = one_ghost_id(&rig);
    let status = rig.manager.set_enabled("lb", false).unwrap();
    assert!(!status.enabled);
    rig.peer_sends(1, &[state(PEER_BASE + 2, Some("tank"), 5.0)]);

    rig.publish(1, &[common::view(eid(1), 0.0)]);
    let events = rig.ingest(31);

    assert_eq!(
        rig.peer_polls(31),
        vec![],
        "nothing published while disabled"
    );
    assert_eq!(rig.world.created, 1, "the disabled adapter was not polled");
    assert_eq!(events, vec![SimEvent::EntityRemoved { entity: id }]);
}

#[test]
fn set_enabled_on_an_unknown_adapter_is_an_error() {
    let mut rig = Rig::new();

    let result = rig.manager.set_enabled("nope", true);

    assert!(result.is_err());
}

#[test]
fn inbound_fire_becomes_an_event_only_when_shooter_and_target_are_mapped() {
    let mut rig = Rig::new();
    let weapon = WeaponId::new("example.weapon.main_gun").unwrap();
    rig.peer_sends(
        0,
        &[
            state(PEER_BASE + 1, Some("tank"), 1.0),
            state(PEER_BASE + 2, Some("tank"), 2.0),
        ],
    );
    rig.ingest(0);
    let ids: Vec<_> = rig.world.ghosts.keys().copied().collect();
    let fire = |shooter: u64, target: u64| NetMessage::Fire {
        shooter: NetKey(shooter),
        target: Some(NetKey(target)),
        weapon: weapon.clone(),
        position: at(1.0),
    };
    rig.peer_sends(
        1,
        &[
            fire(PEER_BASE + 1, PEER_BASE + 2),
            fire(PEER_BASE + 1, PEER_BASE + 9),
        ],
    );

    let events = rig.ingest(1);

    assert_eq!(
        events,
        vec![SimEvent::Fire {
            shooter: ids[0],
            target: ids[1],
            weapon
        }]
    );
}

#[test]
fn inbound_detonation_passes_through_with_whatever_maps() {
    let mut rig = Rig::new();
    rig.peer_sends(
        0,
        &[NetMessage::Detonation {
            shooter: Some(NetKey(PEER_BASE + 7)),
            target: None,
            position: at(3.0),
        }],
    );

    let events = rig.ingest(0);

    assert_eq!(
        events,
        vec![SimEvent::Detonation {
            shooter: None,
            target: None,
            position: at(3.0)
        }]
    );
}

#[test]
fn inbound_run_control_is_ignored_and_counted() {
    let mut rig = Rig::new();
    rig.peer_sends(0, &[NetMessage::SimulationControl(ControlAction::Stop)]);

    let events = rig.ingest(0);

    assert_eq!(events, vec![]);
    assert_eq!(rig.status().control_ignored, 1);
}

/// Counts `stop` calls on the adapters it starts.
struct Counting {
    stops: Arc<AtomicUsize>,
}

struct CountingAdapter {
    stops: Arc<AtomicUsize>,
}

impl NetworkAdapterConfig for Counting {
    fn key_space(&self) -> KeySpace {
        KeySpace { base: 0, count: 10 }
    }
    fn start(&self, _: &AdapterStart) -> Result<Box<dyn NetworkAdapter>, NetError> {
        Ok(Box::new(CountingAdapter {
            stops: Arc::clone(&self.stops),
        }))
    }
}

impl NetworkAdapter for CountingAdapter {
    fn publish(&mut self, _: umbra_core::SimTime, _: &[NetMessage]) -> Result<(), NetError> {
        Ok(())
    }
    fn poll(
        &mut self,
        _: umbra_core::SimTime,
        _: usize,
        _: &mut Vec<NetMessage>,
    ) -> Result<(), NetError> {
        Ok(())
    }
    fn counters(&self) -> AdapterCounters {
        AdapterCounters::default()
    }
    fn stop(&mut self) {
        self.stops.fetch_add(1, Ordering::SeqCst);
    }
}

/// A manager over two counting adapters, `a` and `b`.
#[expect(clippy::unwrap_used, reason = "test helper")]
fn counting_manager(stops: &[Arc<AtomicUsize>; 2]) -> NetworkManager {
    let slot = |name: &str, stops: &Arc<AtomicUsize>| {
        let cfg: Arc<dyn NetworkAdapterConfig> = Arc::new(Counting {
            stops: Arc::clone(stops),
        });
        common::slot(settings(name), cfg, type_map())
    };
    NetworkManager::new(vec![slot("a", &stops[0]), slot("b", &stops[1])], tick(0)).unwrap()
}

fn two_counters() -> [Arc<AtomicUsize>; 2] {
    [Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0))]
}

#[test]
fn stop_reaches_every_adapter_and_may_be_repeated() {
    let stops = two_counters();
    let mut manager = counting_manager(&stops);

    manager.stop();
    let after_first: Vec<usize> = stops.iter().map(|s| s.load(Ordering::SeqCst)).collect();
    manager.stop();

    assert!(after_first.iter().all(|&n| n >= 1), "{after_first:?}");
}

#[test]
fn dropping_the_manager_stops_every_adapter() {
    let stops = two_counters();

    let manager = counting_manager(&stops);
    std::mem::drop(manager);

    assert!(stops.iter().all(|s| s.load(Ordering::SeqCst) >= 1));
}

#[test]
fn inbound_detonation_keeps_a_mapped_shooter() {
    let mut rig = Rig::new();
    rig.peer_sends(0, &[state(PEER_BASE + 7, Some("tank"), 1.0)]);
    rig.ingest(0);
    let ghost = one_ghost_id(&rig);
    rig.peer_sends(
        1,
        &[NetMessage::Detonation {
            shooter: Some(NetKey(PEER_BASE + 7)),
            target: None,
            position: at(3.0),
        }],
    );

    let events = rig.ingest(1);

    assert_eq!(
        events,
        vec![SimEvent::Detonation {
            shooter: Some(ghost),
            target: None,
            position: at(3.0)
        }]
    );
}

#[test]
fn a_key_resent_after_its_ghost_timed_out_creates_a_new_ghost() {
    let mut rig = Rig::new();
    rig.peer_sends(0, &[state(PEER_BASE + 1, Some("tank"), 50.0)]);
    rig.ingest(0);
    let first = one_ghost_id(&rig);
    rig.ingest(31);
    assert!(rig.world.ghosts.is_empty());
    rig.peer_sends(40, &[state(PEER_BASE + 1, Some("tank"), 60.0)]);

    let events = rig.ingest(40);

    let second = one_ghost_id(&rig);
    assert_ne!(second, first, "ids are never reused");
    assert_eq!(rig.world.created, 2);
    assert_eq!(
        events,
        vec![SimEvent::EntityCreated {
            entity: second,
            template: Some(tid("tank.mbt"))
        }]
    );
}

fn control_rig(control: bool) -> Rig {
    let mut s = settings("lb");
    s.publish.control = control;
    Rig::with(&serde_json::json!({}), s, type_map())
}

#[test]
fn run_control_is_published_only_when_the_policy_allows_it() {
    let (mut off, mut on) = (control_rig(false), control_rig(true));

    off.manager.publish_control(tick(0), ControlAction::Pause);
    on.manager.publish_control(tick(0), ControlAction::Pause);

    assert_eq!(off.peer_polls(0), vec![]);
    assert_eq!(
        on.peer_polls(0),
        vec![NetMessage::SimulationControl(ControlAction::Pause)]
    );
}

#[test]
fn a_zero_timeout_removes_a_ghost_at_the_next_tick() {
    let mut s = settings("lb");
    s.ghosts.timeout = Seconds(0.0);
    let mut rig = Rig::with(&serde_json::json!({}), s, type_map());
    rig.peer_sends(0, &[state(PEER_BASE + 1, Some("tank"), 1.0)]);
    rig.ingest(0);
    assert_eq!(rig.world.ghosts.len(), 1);

    rig.ingest(1);

    assert!(rig.world.ghosts.is_empty());
}
