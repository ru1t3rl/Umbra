//! Manager behaviour beyond the step-4 rows (WI-7): deterministic order, removal messages,
//! re-enabling, faults and the metadata carried by published states. Requirements: REQ-N-1..5,
//! REQ-K-4, ADR 0030, ADR 0031.

#![forbid(unsafe_code)]

mod common;

use std::collections::BTreeMap;
use std::sync::Arc;

use umbra_core::net::{
    AdapterCounters, AdapterStart, KeySpace, NetError, NetKey, NetMessage, NetworkAdapter,
    NetworkAdapterConfig,
};
use umbra_core::{Force, SimEvent, SimTime};
use umbra_net::{AdapterSlot, GhostPolicy, ManagerError, NetworkManager, OwnedMeta};

use common::{OWN_BASE, PEER_BASE, Rig, eid, settings, state, tick, tid, ty, type_map, view};

/// `(key, y metres east)` of every published entity state, in message order.
fn states(msgs: &[NetMessage]) -> Vec<(u64, f64)> {
    msgs.iter()
        .filter_map(|m| match m {
            NetMessage::EntityState(s) => Some((s.key.0, s.position.0.y)),
            _ => None,
        })
        .collect()
}

#[test]
fn owned_entities_are_published_in_id_order_whatever_the_order_of_the_views() {
    let mut rig = Rig::new();

    rig.publish(
        0,
        &[view(eid(3), 3.0), view(eid(1), 1.0), view(eid(2), 2.0)],
    );

    assert_eq!(
        states(&rig.peer_polls(0)),
        vec![
            (OWN_BASE + 1, 1.0),
            (OWN_BASE + 2, 2.0),
            (OWN_BASE + 3, 3.0)
        ]
    );
}

#[test]
fn ghost_timeouts_are_reported_in_scenario_order_of_the_adapters() {
    let hub_a = umbra_net::LoopbackHub::new(64);
    let hub_b = umbra_net::LoopbackHub::new(64);
    let cfg_a = common::config(&hub_a, "test.net.a", OWN_BASE, &serde_json::json!({}));
    let cfg_b = common::config(&hub_b, "test.net.b", 3000, &serde_json::json!({}));
    let mut manager = NetworkManager::new(
        vec![
            common::slot(settings("a"), cfg_a, type_map()),
            common::slot(settings("b"), cfg_b, type_map()),
        ],
        tick(0),
    )
    .unwrap();
    let mut peer_a = common::peer_on(&hub_a, "test.net.peer_a");
    let mut peer_b = common::peer_on(&hub_b, "test.net.peer_b");
    let mut world = common::FakeWorld::default();
    // b's ghost gets the lower id: a is disabled while it is created.
    manager.set_enabled("a", false).unwrap();
    peer_b
        .publish(tick(0), &[state(PEER_BASE + 1, Some("tank"), 1.0)])
        .unwrap();
    manager.ingest(tick(0), &mut world, &mut Vec::new());
    manager.set_enabled("a", true).unwrap();
    peer_a
        .publish(tick(1), &[state(PEER_BASE + 1, Some("tank"), 2.0)])
        .unwrap();
    manager.ingest(tick(1), &mut world, &mut Vec::new());
    let ids: Vec<_> = world.ghosts.keys().copied().collect();
    assert_eq!(ids.len(), 2);

    let mut events = Vec::new();
    manager.ingest(tick(60), &mut world, &mut events);

    // a (scenario first) holds the higher id, so id order would be the reverse.
    assert_eq!(
        events,
        vec![
            SimEvent::EntityRemoved { entity: ids[1] },
            SimEvent::EntityRemoved { entity: ids[0] },
        ]
    );
}

#[test]
fn a_removed_published_entity_sends_one_removal_under_its_key() {
    let mut rig = Rig::new();
    rig.publish(0, &[view(eid(1), 1.0)]);
    rig.peer_polls(0);

    rig.manager.publish(
        tick(1),
        &[],
        &BTreeMap::new(),
        &[
            SimEvent::EntityRemoved { entity: eid(1) },
            SimEvent::EntityRemoved { entity: eid(1) },
            SimEvent::EntityRemoved { entity: eid(9) },
        ],
    );

    assert_eq!(
        rig.peer_polls(1),
        vec![NetMessage::EntityRemoved {
            key: NetKey(OWN_BASE + 1)
        }]
    );
}

#[test]
fn enabling_an_adapter_again_sends_every_owned_entity_afresh() {
    let mut rig = Rig::new();
    rig.publish(0, &[view(eid(1), 1.0)]);
    rig.manager.set_enabled("lb", false).unwrap();
    rig.manager.set_enabled("lb", true).unwrap();

    rig.publish(1, &[view(eid(1), 1.0)]);

    // Same key as before, sent again although nothing moved.
    assert_eq!(
        states(&rig.peer_polls(1)),
        vec![(OWN_BASE + 1, 1.0), (OWN_BASE + 1, 1.0)]
    );
}

#[test]
fn published_states_carry_the_templates_entity_type_name_and_force() {
    let hub = umbra_net::LoopbackHub::new(16);
    let cfg = common::config(&hub, "test.net.a", OWN_BASE, &serde_json::json!({}));
    let types = BTreeMap::from([(tid("tank.mbt"), ty("tank"))]);
    let mut manager = NetworkManager::new(
        vec![AdapterSlot::new(settings("lb"), cfg, type_map(), types)],
        tick(0),
    )
    .unwrap();
    let mut peer = common::peer_on(&hub, "test.net.peer");
    let meta = BTreeMap::from([(
        eid(1),
        OwnedMeta {
            template: Some(tid("tank.mbt")),
            name: Some(Arc::from("alpha")),
        },
    )]);

    manager.publish(tick(0), &[view(eid(1), 1.0), view(eid(2), 2.0)], &meta, &[]);
    let mut got = Vec::new();
    peer.poll(tick(0), 10, &mut got).unwrap();

    let details: Vec<_> = got
        .iter()
        .filter_map(|m| match m {
            NetMessage::EntityState(s) => Some((
                s.entity_type.clone(),
                s.name.as_deref().map(str::to_owned),
                s.force.clone(),
            )),
            _ => None,
        })
        .collect();
    let blue = Force::new("blue").ok();
    assert_eq!(
        details,
        vec![
            (Some(ty("tank")), Some("alpha".to_owned()), blue.clone()),
            (None, None, blue),
        ]
    );
}

/// An adapter whose `poll` always fails.
struct Failing;
struct FailingAdapter;

impl NetworkAdapterConfig for Failing {
    fn key_space(&self) -> KeySpace {
        KeySpace { base: 0, count: 10 }
    }
    fn start(&self, _: &AdapterStart) -> Result<Box<dyn NetworkAdapter>, NetError> {
        Ok(Box::new(FailingAdapter))
    }
}

impl NetworkAdapter for FailingAdapter {
    fn publish(&mut self, _: SimTime, _: &[NetMessage]) -> Result<(), NetError> {
        Ok(())
    }
    fn poll(&mut self, _: SimTime, _: usize, _: &mut Vec<NetMessage>) -> Result<(), NetError> {
        Err(NetError::Io("broken".to_owned()))
    }
    fn counters(&self) -> AdapterCounters {
        AdapterCounters::default()
    }
    fn stop(&mut self) {}
}

#[test]
fn an_adapter_error_disables_the_adapter_and_it_cannot_be_enabled_again() {
    let cfg: Arc<dyn NetworkAdapterConfig> = Arc::new(Failing);
    let mut manager = NetworkManager::new(
        vec![common::slot(settings("bad"), cfg, type_map())],
        tick(0),
    )
    .unwrap();

    manager.ingest(tick(0), &mut common::FakeWorld::default(), &mut Vec::new());

    let status = manager.status().remove(0);
    assert!(status.faulted);
    assert!(!status.enabled);
    assert!(
        status
            .fault_reason
            .as_deref()
            .is_some_and(|r| r.contains("broken")),
        "{:?}",
        status.fault_reason
    );
    assert_eq!(
        manager.set_enabled("bad", true),
        Err(ManagerError::Faulted("bad".to_owned()))
    );
    assert!(!manager.set_enabled("bad", false).unwrap().enabled);
}

#[test]
fn an_adapter_disabled_from_the_start_is_started_but_not_polled() {
    let mut s = settings("lb");
    s.enabled = false;
    let mut rig = Rig::with(&serde_json::json!({}), s, type_map());
    rig.peer_sends(0, &[state(PEER_BASE + 1, Some("tank"), 1.0)]);

    rig.ingest(0);

    assert!(rig.world.ghosts.is_empty());
    assert_eq!(rig.status().received, 0);
    assert_eq!(rig.hub.starts(), 2, "the adapter and the raw peer started");
    let enabled = rig.manager.set_enabled("lb", true).unwrap();
    assert!(enabled.enabled);
    rig.ingest(1);
    assert_eq!(rig.world.ghosts.len(), 1);
}

#[test]
fn a_ghosts_fire_is_not_published_even_when_the_ghost_is_among_the_views() {
    let mut rig = Rig::new();
    rig.peer_sends(0, &[state(PEER_BASE + 1, Some("tank"), 50.0)]);
    rig.ingest(0);
    let ghost = *rig.world.ghosts.keys().next().expect("a ghost");
    let weapon = umbra_core::WeaponId::new("example.weapon.main_gun").unwrap();

    rig.manager.publish(
        tick(0),
        &[view(eid(1), 0.0), view(ghost, 50.0)],
        &BTreeMap::new(),
        &[SimEvent::Fire {
            shooter: ghost,
            target: eid(1),
            weapon,
        }],
    );

    let fires = rig
        .peer_polls(0)
        .into_iter()
        .filter(|m| matches!(m, NetMessage::Fire { .. }))
        .count();
    assert_eq!(fires, 0);
}

#[test]
fn inbound_detonation_with_an_own_space_shooter_is_counted_as_an_echo() {
    let mut rig = Rig::new();
    rig.peer_sends(
        0,
        &[NetMessage::Detonation {
            shooter: Some(NetKey(OWN_BASE + 5)),
            target: None,
            position: common::at(1.0),
        }],
    );

    let events = rig.ingest(0);

    assert_eq!(events, vec![]);
    assert_eq!(rig.status().echo_dropped, 1);
}

#[test]
fn an_inbound_detonation_at_a_non_finite_point_is_refused_and_counted() {
    let mut rig = Rig::new();
    let detonation = |position| NetMessage::Detonation {
        shooter: None,
        target: None,
        position,
    };
    let mut nan = common::at(1.0);
    nan.0.x = f64::NAN;
    let mut infinite = common::at(1.0);
    infinite.0.z = f64::INFINITY;
    rig.peer_sends(0, &[detonation(nan), detonation(infinite)]);

    let events = rig.ingest(0);

    assert_eq!(
        events,
        vec![],
        "no event leaves for a point that is not finite"
    );
    assert_eq!(rig.status().invalid_state, 2);
    // A finite one still passes.
    rig.peer_sends(1, &[detonation(common::at(1.0))]);
    assert_eq!(rig.ingest(1).len(), 1);
    assert_eq!(rig.status().invalid_state, 2);
}

#[test]
fn a_refused_update_removes_the_ghost_instead_of_orphaning_it() {
    let mut rig = Rig::new();
    rig.peer_sends(0, &[state(PEER_BASE + 1, Some("tank"), 1.0)]);
    rig.ingest(0);
    let id = *rig.world.ghosts.keys().next().expect("a ghost");
    rig.world.refuse_updates = true;
    rig.peer_sends(1, &[state(PEER_BASE + 1, Some("tank"), 2.0)]);

    let events = rig.ingest(1);

    assert!(rig.world.ghosts.is_empty(), "no ghost is left in the world");
    assert_eq!(events, vec![SimEvent::EntityRemoved { entity: id }]);
    assert_eq!(rig.status().ghosts, 0);
    assert_eq!(rig.status().world_errors, 1);
    // The next state of the key starts afresh.
    rig.world.refuse_updates = false;
    rig.peer_sends(2, &[state(PEER_BASE + 1, Some("tank"), 3.0)]);
    rig.ingest(2);
    assert_eq!(rig.world.ghosts.len(), 1);
    assert_eq!(rig.world.created, 2);
}

#[test]
fn a_non_finite_inbound_state_is_refused_and_counted() {
    let mut rig = Rig::new();
    let mut bad = state(PEER_BASE + 1, Some("tank"), 1.0);
    if let NetMessage::EntityState(s) = &mut bad {
        s.velocity.z = f64::NAN;
    }
    let mut also_bad = state(PEER_BASE + 2, Some("tank"), 1.0);
    if let NetMessage::EntityState(s) = &mut also_bad {
        s.orientation.yaw = umbra_core::Radians(f64::INFINITY);
    }
    rig.peer_sends(0, &[bad, also_bad]);

    let events = rig.ingest(0);

    assert!(rig.world.ghosts.is_empty());
    assert_eq!(events, vec![]);
    assert_eq!(rig.status().invalid_state, 2);
}

#[test]
fn at_the_cap_an_unmapped_state_counts_as_unmapped_not_capped() {
    let mut s = settings("lb");
    s.ghosts = GhostPolicy { max: 1, ..s.ghosts };
    let mut rig = Rig::with(&serde_json::json!({}), s, type_map());
    rig.peer_sends(
        0,
        &[
            state(PEER_BASE + 1, Some("tank"), 1.0),
            state(PEER_BASE + 2, Some("helicopter"), 2.0),
        ],
    );

    rig.ingest(0);

    let status = rig.status();
    assert_eq!(
        (status.ghosts, status.unmapped, status.ghosts_capped),
        (1, 1, 0)
    );
}

#[test]
fn unmapped_types_are_listed_ascending_and_bounded_but_all_counted() {
    let mut rig = Rig::new();
    let msgs: Vec<_> = (0..70_u64)
        .map(|n| {
            let ty = format!("type{:02}", 69 - n);
            state(PEER_BASE + 1 + n, Some(&ty), 1.0)
        })
        .collect();
    rig.peer_sends(0, &msgs);
    rig.peer_sends(0, &[state(PEER_BASE + 99, Some("type69"), 1.0)]);

    rig.ingest(0);

    let status = rig.status();
    assert_eq!(status.unmapped, 71);
    let listed: Vec<_> = status
        .unmapped_types
        .iter()
        .map(|t| t.as_str().to_owned())
        .collect();
    assert_eq!(listed.len(), 64);
    assert!(
        listed.windows(2).all(|w| w[0] < w[1]),
        "ascending, distinct"
    );
    // The first 64 distinct tokens seen (type69 down to type06) are kept.
    assert_eq!(listed.first().map(String::as_str), Some("type06"));
    assert_eq!(listed.last().map(String::as_str), Some("type69"));
}

#[test]
fn a_fire_whose_owned_shooter_is_not_among_the_views_is_not_published() {
    let mut rig = Rig::new();
    rig.publish(0, &[view(eid(1), 0.0), view(eid(2), 5.0)]);
    rig.peer_polls(0);
    let weapon = umbra_core::WeaponId::new("example.weapon.main_gun").unwrap();

    rig.manager.publish(
        tick(1),
        &[view(eid(2), 5.0)],
        &BTreeMap::new(),
        &[SimEvent::Fire {
            shooter: eid(1),
            target: eid(2),
            weapon,
        }],
    );

    let fires = rig
        .peer_polls(1)
        .into_iter()
        .filter(|m| matches!(m, NetMessage::Fire { .. }))
        .count();
    assert_eq!(fires, 0);
}

#[test]
fn a_ghost_the_kernel_removed_itself_is_forgotten_silently() {
    let mut rig = Rig::new();
    rig.peer_sends(0, &[state(PEER_BASE + 1, Some("tank"), 1.0)]);
    rig.ingest(0);
    let id = *rig.world.ghosts.keys().next().expect("a ghost");
    rig.world.ghosts.remove(&id);

    rig.manager.publish(
        tick(1),
        &[],
        &BTreeMap::new(),
        &[SimEvent::EntityRemoved { entity: id }],
    );
    let events = rig.ingest(40);

    assert_eq!(rig.peer_polls(1), vec![], "nothing is announced");
    assert_eq!(events, vec![], "the timeout finds nothing left");
    assert_eq!(rig.status().ghosts, 0);
    assert_eq!(rig.status().world_errors, 0);
}
