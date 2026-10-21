//! AT-12 (engine level), AT-13 (engine level) and AT-5 (end to end): engines over loopback
//! hubs. Requirements: REQ-N-2..5, REQ-K-5, ADR 0030..0032, ASM-224, ASM-248, ASM-267, ASM-272.
//!
//! Step 4 of M6: written before WI-9. Engines are driven in lockstep on one thread: each round
//! ticks A, then B (then the bridge). `Engine::attach_network` is a `todo!()` stub, so the tests
//! that need a link are ignored with that work item.

#![forbid(unsafe_code)]

mod net_support;

use umbra_core::SimEvent;
use umbra_engine::TemplateId;
use umbra_net::LoopbackHub;

use net_support::{Peer, distance, id_of, is_remote_from, registry_with, start};

/// Engines A (keys 1000..) and B (keys 2000..) on one hub.
fn pair() -> (Peer, Peer, Vec<umbra_engine::EntityInfo>) {
    let hub = LoopbackHub::new(4096);
    let (a, _) = Peer::new(start(
        "a",
        &registry_with(&[("test.net.loopback", &hub, 1000)]),
        None,
    ));
    let (b, b_infos) = Peer::new(start(
        "b",
        &registry_with(&[("test.net.loopback", &hub, 2000)]),
        None,
    ));
    (a, b, b_infos)
}

fn round(a: &mut Peer, b: &mut Peer) {
    a.tick();
    b.tick();
}

#[test]
#[ignore = "WI-9"]
fn ghosts_appear_with_mapped_template() {
    let (mut a, mut b, _) = pair();

    for _ in 0..5 {
        round(&mut a, &mut b);
    }

    let ghosts = a.ghosts();
    assert_eq!(ghosts.len(), 1, "only b-tank maps: {ghosts:?}");
    assert!(is_remote_from(&ghosts[0], "lb"));
    assert_eq!(
        ghosts[0].force,
        umbra_core::Force::new("red").ok(),
        "force of b's entities"
    );
    let created: Vec<_> = a
        .all_events()
        .into_iter()
        .filter_map(|e| match e {
            SimEvent::EntityCreated { entity, template } => Some((*entity, template.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(
        created,
        vec![(
            ghosts[0].id,
            Some(TemplateId::new("tank.mbt_generic").expect("valid id"))
        )]
    );
}

#[test]
#[ignore = "WI-9"]
fn unmapped_types_are_not_mirrored() {
    let (mut a, mut b, _) = pair();

    for _ in 0..5 {
        round(&mut a, &mut b);
    }

    // b-truck declares no entity type and A has no default template: counted, never mirrored.
    assert_eq!(a.entity_count(), a.owned().len() + 1);
    let status = a.engine.network().expect("link").status();
    assert!(status[0].unmapped >= 1, "{status:?}");
    assert_eq!(status[0].ghosts, 1);
}

#[test]
#[ignore = "WI-9"]
fn receiver_error_stays_within_threshold_every_tick() {
    let (mut a, mut b, b_infos) = pair();
    let b_tank = id_of(&b_infos, "b-tank");
    // threshold 1 m (default) + the tank's cruise speed over a three-tick lag.
    let bound = 1.0 + 14.0 * 3.0 / 30.0;
    let first = {
        for _ in 0..5 {
            round(&mut a, &mut b);
        }
        a.ghosts()[0].state.position
    };

    // 1500 rounds: the first leg (500 m at 14 m/s) ends near round 1070, so the run includes a turn.
    for tick in 5..1500 {
        round(&mut a, &mut b);

        let ghost = a.ghosts();
        assert_eq!(ghost.len(), 1, "tick {tick}: the ghost stays");
        let truth = b
            .engine
            .snapshot()
            .entities
            .into_iter()
            .find(|e| e.id == b_tank)
            .expect("b-tank")
            .state
            .position;
        let error = distance(ghost[0].state.position, truth);
        assert!(
            error <= bound,
            "tick {tick}: error {error} m exceeds {bound} m"
        );
    }
    let last = a.ghosts()[0].state.position;
    assert!(
        distance(first, last) > 10.0,
        "the tank moved, so the bound was exercised"
    );
}

#[test]
#[ignore = "WI-9"]
fn stale_ghosts_are_removed_after_timeout() {
    let (mut a, mut b, _) = pair();
    b.ticks(0);
    for _ in 0..10 {
        round(&mut a, &mut b);
    }
    assert_eq!(a.ghosts().len(), 1);

    // B goes silent. The timeout is 3 s = 90 ticks.
    a.ticks(60);
    assert_eq!(
        a.ghosts().len(),
        1,
        "still alive 60 ticks after the last update"
    );
    a.ticks(50);

    assert_eq!(a.ghosts().len(), 0);
    let removed = a
        .all_events()
        .into_iter()
        .filter(|e| matches!(e, SimEvent::EntityRemoved { .. }))
        .count();
    assert_eq!(removed, 1);
}

#[test]
#[ignore = "WI-9"]
fn removed_owner_removes_ghost() {
    let (mut a, mut b, b_infos) = pair();
    for _ in 0..10 {
        round(&mut a, &mut b);
    }
    let ghost = a.ghosts()[0].id;

    b.engine.remove(id_of(&b_infos, "b-tank")).expect("remove");
    for _ in 0..5 {
        round(&mut a, &mut b);
    }

    assert!(
        a.ghosts().is_empty(),
        "removed well before the 90 tick timeout"
    );
    assert!(
        a.all_events()
            .contains(&&SimEvent::EntityRemoved { entity: ghost })
    );
}

/// AT-5 end to end: C bridges hub 1 (A) and hub 2 (B) and owns one tank. It mirrors A's and B's
/// tanks but must never republish them, so A never sees B's entities nor the reverse.
#[test]
#[ignore = "WI-9"]
fn bridge_engine_never_republishes_ghosts() {
    let (hub1, hub2) = (
        LoopbackHub::with_capture(4096),
        LoopbackHub::with_capture(4096),
    );
    let (mut a, _) = Peer::new(start(
        "a",
        &registry_with(&[("test.net.loopback", &hub1, 1000)]),
        None,
    ));
    let (mut b, _) = Peer::new(start(
        "b",
        &registry_with(&[("test.net.loopback", &hub2, 2000)]),
        None,
    ));
    let (mut c, _) = Peer::new(start(
        "c",
        &registry_with(&[
            ("test.net.left", &hub1, 3000),
            ("test.net.right", &hub2, 4000),
        ]),
        None,
    ));
    let counts =
        |a: &Peer, b: &Peer, c: &Peer| [a.ghosts().len(), b.ghosts().len(), c.ghosts().len()];

    for _ in 0..300 {
        a.tick();
        b.tick();
        c.tick();
    }
    let converged = counts(&a, &b, &c);
    let entities = [a.entity_count(), b.entity_count(), c.entity_count()];
    assert_eq!(
        converged,
        [1, 1, 2],
        "A and B see only C's tank; C sees both tanks"
    );

    for round in 300..10_000_u32 {
        a.tick();
        b.tick();
        c.tick();
        if round % 500 == 0 {
            assert_eq!(counts(&a, &b, &c), converged, "round {round}");
            assert_eq!(
                [a.entity_count(), b.entity_count(), c.entity_count()],
                entities,
                "round {round}"
            );
        }
    }

    let in_space = |key: u64, base: u64| key > base && key <= base + 100;
    for (hub, spaces) in [(&hub1, [1000, 3000]), (&hub2, [2000, 4000])] {
        for (_, message) in hub.captured() {
            if let umbra_core::net::NetMessage::EntityState(s) = message {
                assert!(
                    spaces.iter().any(|&base| in_space(s.key.0, base)),
                    "key {} on a hub it does not belong to",
                    s.key.0
                );
            }
        }
    }
}
