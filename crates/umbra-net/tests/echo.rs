//! AT-5 (unit level): echo suppression and the Owned-only publish rule. Requirements: REQ-K-5,
//! REQ-N-4, REQ-N-5, ADR 0031.
//!
//! Step 4 of M6: written before WI-7; `NetworkManager` and `LoopbackHub` are `todo!()` stubs.

#![forbid(unsafe_code)]

mod common;

use std::collections::BTreeMap;

use umbra_core::net::{NetKey, NetMessage};
use umbra_core::{SimEvent, WeaponId};
use umbra_net::NetworkManager;

use common::{OWN_BASE, Rig, at, config, eid, peer_on, settings, state, tick, type_map, view};

fn entity_state_keys(msgs: &[NetMessage]) -> Vec<NetKey> {
    msgs.iter()
        .filter_map(|m| match m {
            NetMessage::EntityState(s) => Some(s.key),
            _ => None,
        })
        .collect()
}

#[test]
#[ignore = "WI-7"]
fn remote_entities_are_never_published() {
    let mut rig = Rig::new();
    rig.peer_sends(0, &[state(2001, Some("tank"), 50.0)]);
    rig.ingest(0);
    let ghost = *rig.world.ghosts.keys().next().expect("a ghost was created");
    let owned = eid(1);

    // The ghost's id is wrongly present among the owned views, as a kernel bug would put it.
    rig.publish(0, &[view(owned, 0.0), view(ghost, 50.0)]);
    let received = rig.peer_polls(0);

    let keys = entity_state_keys(&received);
    assert_eq!(
        keys.len(),
        1,
        "only the owned entity is published: {received:?}"
    );
    assert!(
        keys[0].0 > OWN_BASE && keys[0].0 <= OWN_BASE + 100,
        "the published key lies in the own key space: {keys:?}"
    );
}

#[test]
#[ignore = "WI-7"]
fn own_key_space_inbound_is_dropped() {
    let mut rig = Rig::new();
    rig.peer_sends(0, &[state(OWN_BASE + 50, Some("tank"), 10.0)]);

    let events = rig.ingest(0);

    assert!(rig.world.ghosts.is_empty(), "no ghost of an own key");
    assert_eq!(events, vec![]);
    assert_eq!(rig.status().echo_dropped, 1);
    assert_eq!(rig.status().ghosts, 0);
}

/// Owned entities' keys coming back (here by the loopback `echo` parameter) are dropped. The keys
/// lie in the own space, so layers 1 and 3 of ADR 0031 both apply; the layer that matters beyond
/// the space is tested in `manager.rs` (`a_key_bound_as_owned_outside_the_own_space_...`).
#[test]
#[ignore = "WI-7"]
fn keys_of_owned_entities_are_dropped() {
    let mut rig = Rig::with(
        &serde_json::json!({"echo": true}),
        settings("lb"),
        type_map(),
    );
    rig.publish(0, &[view(eid(1), 0.0), view(eid(2), 10.0)]);
    assert_eq!(rig.status().published, 2, "both owned entities went out");

    let events = rig.ingest(0);

    assert!(
        rig.world.ghosts.is_empty(),
        "the echo of own entities is not mirrored"
    );
    assert_eq!(events, vec![]);
    assert_eq!(rig.status().echo_dropped, 2);
}

#[test]
#[ignore = "WI-7"]
fn ghosts_from_one_adapter_are_not_published_on_another() {
    let hub_a = umbra_net::LoopbackHub::new(64);
    let hub_b = umbra_net::LoopbackHub::new(64);
    let cfg_a = config(&hub_a, "test.net.a", OWN_BASE, &serde_json::json!({}));
    let cfg_b = config(&hub_b, "test.net.b", 3000, &serde_json::json!({}));
    let mut manager = NetworkManager::new(
        vec![
            common::slot(settings("a"), cfg_a, type_map()),
            common::slot(settings("b"), cfg_b, type_map()),
        ],
        tick(0),
    )
    .unwrap();
    let mut peer_a = peer_on(&hub_a, "test.net.peer_a");
    let mut peer_b = peer_on(&hub_b, "test.net.peer_b");
    peer_a
        .publish(tick(0), &[state(2001, Some("tank"), 50.0)])
        .unwrap();
    let mut world = common::FakeWorld::default();
    manager.ingest(tick(0), &mut world, &mut Vec::new());
    let ghost = *world.ghosts.keys().next().expect("a ghost from adapter a");

    // The ghost's id is among the views, next to one owned entity.
    manager.publish(
        tick(0),
        &[view(eid(1), 0.0), view(ghost, 50.0)],
        &BTreeMap::new(),
        &[],
    );
    let (mut on_b, mut back_on_a) = (Vec::new(), Vec::new());
    peer_b.poll(tick(0), 100, &mut on_b).unwrap();
    peer_a.poll(tick(0), 100, &mut back_on_a).unwrap();

    let in_space =
        |keys: &[NetKey], base: u64| keys.len() == 1 && keys[0].0 > base && keys[0].0 <= base + 100;
    assert!(
        in_space(&entity_state_keys(&on_b), 3000),
        "b gets exactly the owned entity, under its own key: {on_b:?}"
    );
    assert!(
        in_space(&entity_state_keys(&back_on_a), OWN_BASE),
        "a gets exactly the owned entity, not its own ghost: {back_on_a:?}"
    );
}

#[test]
#[ignore = "WI-7"]
fn inbound_fire_with_an_own_space_shooter_is_counted_as_an_echo() {
    let mut rig = Rig::new();
    rig.peer_sends(
        0,
        &[NetMessage::Fire {
            shooter: NetKey(OWN_BASE + 5),
            target: Some(NetKey(2001)),
            weapon: WeaponId::new("example.weapon.main_gun").unwrap(),
            position: at(1.0),
        }],
    );

    let events = rig.ingest(0);

    assert_eq!(events, vec![]);
    assert_eq!(rig.status().echo_dropped, 1);
}

/// Only events whose source is an Owned entity go out; a ghost's `Fire` or `Detonation` never does,
/// and the target of an owned shooter is sent under the peer's key.
#[test]
#[ignore = "WI-7"]
fn fire_and_detonation_are_published_only_for_owned_sources() {
    let mut rig = Rig::new();
    rig.peer_sends(0, &[state(2001, Some("tank"), 50.0)]);
    rig.ingest(0);
    let ghost = *rig.world.ghosts.keys().next().expect("a ghost");
    let owned = eid(1);
    let weapon = WeaponId::new("example.weapon.main_gun").unwrap();
    let events = [
        SimEvent::Fire {
            shooter: owned,
            target: ghost,
            weapon: weapon.clone(),
        },
        SimEvent::Fire {
            shooter: ghost,
            target: owned,
            weapon,
        },
        SimEvent::Detonation {
            shooter: Some(owned),
            target: Some(ghost),
            position: at(2.0),
        },
        SimEvent::Detonation {
            shooter: Some(ghost),
            target: None,
            position: at(3.0),
        },
    ];

    rig.manager
        .publish(tick(0), &[view(owned, 0.0)], &BTreeMap::new(), &events);
    let received = rig.peer_polls(0);

    let in_own_space = |k: &NetKey| k.0 > OWN_BASE && k.0 <= OWN_BASE + 100;
    let fires: Vec<_> = received
        .iter()
        .filter_map(|m| match m {
            NetMessage::Fire {
                shooter, target, ..
            } => Some((*shooter, *target)),
            _ => None,
        })
        .collect();
    let detonations: Vec<_> = received
        .iter()
        .filter_map(|m| match m {
            NetMessage::Detonation {
                shooter, target, ..
            } => Some((*shooter, *target)),
            _ => None,
        })
        .collect();
    assert_eq!(fires.len(), 1, "{received:?}");
    assert!(in_own_space(&fires[0].0));
    assert_eq!(fires[0].1, Some(NetKey(2001)));
    assert_eq!(detonations.len(), 1, "{received:?}");
    assert!(detonations[0].0.as_ref().is_some_and(in_own_space));
    assert_eq!(detonations[0].1, Some(NetKey(2001)));
}

#[test]
#[ignore = "WI-7"]
fn publishing_more_entities_than_keys_counts_keys_exhausted_and_skips_the_rest() {
    let mut rig = Rig::new(); // 100 keys
    let views: Vec<_> = (1..=101_u32)
        .map(|n| view(eid(u64::from(n)), f64::from(n)))
        .collect();

    rig.publish(0, &views);
    let received = rig.peer_polls(0);

    assert_eq!(entity_state_keys(&received).len(), 100);
    assert_eq!(rig.status().keys_exhausted, 1);
}
