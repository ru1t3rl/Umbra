//! AT-3 (kernel row) and AT-14: the Behavior phase with an instantiated behavior.
//! Requirements: REQ-B-1, REQ-B-2, REQ-B-5, REQ-K-2, REQ-K-4, REQ-K-5, ADR 0013, ADR 0016, ASM-082.
//!
//! Step 4 of M3: written before WI-6. `Kernel::spawn_with` is a `todo!()` stub, so every test that
//! spawns an agent is ignored with that work item. Behaviors here are closures (`agents::Scripted`);
//! the kernel may not use `cgf-behavior` (ASM-005).

#![forbid(unsafe_code)]

mod agents;
mod common;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use cgf_core::{
    ComponentId, EntityId, Force, Intent, KinematicState, Ownership, ParamValue, Radians,
    RemoteSource, Status, TemplateId, Vec3, WeaponId,
};
use cgf_kernel::{AgentInit, Kernel};

use agents::{RangeSensor, Scripted};
use common::{Behavior as Motion, Stub, at, kernel, owned, route};

/// What the behavior saw: force, `(next, len, complete)` of the route, contact ids.
type Looked = (Option<Force>, Option<(usize, usize, bool)>, Vec<EntityId>);

fn agent(b: Arc<dyn cgf_core::Behavior>) -> AgentInit {
    AgentInit {
        behavior: Some(b),
        ..AgentInit::default()
    }
}

/// A behavior that appends `intents` every tick and returns Running.
fn emitting(id: &str, intents: Vec<Intent>) -> Arc<dyn cgf_core::Behavior> {
    Scripted::arc(id, move || {
        let intents = intents.clone();
        Box::new(move |_, _, out| {
            for i in &intents {
                out.push(i.clone());
            }
            Status::Running
        })
    })
}

/// ADR 0016, ASM-082: a behavior that emits `FollowRoute` every tick gives, tick by tick, the same
/// intents, route progress, states and hashes as the M1 stand-in of an entity without a behavior.
#[test]
#[ignore = "WI-6"]
fn follow_route_matches_stand_in_tick_by_tick() {
    let model = Stub::new("test.model.drift", Motion::Drift(Vec3::new(0.0, 10.0, 0.0)));
    let mut standin = kernel(9);
    let mut agent_kernel = kernel(9);
    let mut init = owned(at(0.0), &model);
    init.route = Some(route(vec![at(20.0), at(40.0), at(60.0)], 8.0));
    standin.spawn(init.clone()).expect("spawn");
    agent_kernel
        .spawn_with(
            init,
            agent(emitting("test.behavior.follow", vec![Intent::FollowRoute])),
        )
        .expect("spawn_with");

    for tick in 1..=150 {
        standin.tick().expect("tick");
        agent_kernel.tick().expect("tick");
        let (a, b) = (standin.snapshot(), agent_kernel.snapshot());
        assert_eq!(a.entities[0].intents, b.entities[0].intents, "tick {tick}");
        assert_eq!(
            a.entities[0].route_next, b.entities[0].route_next,
            "tick {tick}"
        );
        assert_eq!(a.entities[0].state, b.entities[0].state, "tick {tick}");
        assert_eq!(
            standin.state_hash(),
            agent_kernel.state_hash(),
            "tick {tick}"
        );
    }
    assert_eq!(
        agent_kernel.snapshot().entities[0].route_next,
        Some((2, 3)),
        "the route was followed to its last waypoint"
    );
}

/// ADR 0016: `FollowRoute` of an entity without a route is dropped (no intent reaches motion).
#[test]
#[ignore = "WI-6"]
fn follow_route_without_route_is_dropped() {
    let model = Stub::new("test.model.still", Motion::Stationary);
    let mut k = kernel(1);
    k.spawn_with(
        owned(at(0.0), &model),
        agent(emitting("test.behavior.follow", vec![Intent::FollowRoute])),
    )
    .expect("spawn_with");

    k.tick().expect("tick");

    assert_eq!(k.snapshot().entities[0].intents, Vec::<Intent>::new());
    assert_eq!(model.calls(), vec![Vec::<Intent>::new()]);
}

/// ADR 0016: only the first `FollowRoute` is expanded, in place; later ones are dropped, and the
/// intents around it keep their order.
#[test]
#[ignore = "WI-6"]
fn first_follow_route_is_expanded_in_place_and_later_ones_are_dropped() {
    let model = Stub::new("test.model.still", Motion::Stationary);
    let mut k = kernel(1);
    let mut init = owned(at(0.0), &model);
    init.route = Some(route(vec![at(500.0)], 6.0));
    let intents = vec![
        Intent::Hold,
        Intent::FollowRoute,
        Intent::Despawn,
        Intent::FollowRoute,
    ];
    k.spawn_with(init, agent(emitting("test.behavior.many", intents)))
        .expect("spawn_with");

    k.tick().expect("tick");
    let got = k.snapshot().entities[0].intents.clone();

    assert_eq!(got.len(), 4, "Hold, SetSpeed, MoveTo, Despawn: {got:?}");
    assert_eq!(got[0], Intent::Hold);
    assert!(matches!(got[1], Intent::SetSpeed(_)));
    assert!(matches!(got[2], Intent::MoveTo { .. }));
    assert_eq!(got[3], Intent::Despawn);
}

/// ADR 0016: `Fire`, `Spawn`, `Despawn` and `Custom` are recorded in the snapshot and change
/// nothing: same entity count and the same state hash as an entity that emits nothing.
#[test]
#[ignore = "WI-6"]
fn fire_spawn_despawn_custom_are_recorded_not_applied() {
    let model = Stub::new("test.model.still", Motion::Stationary);
    let target = EntityId::new(1).expect("non-zero");
    let recorded = vec![
        Intent::Fire {
            target,
            weapon: WeaponId::new("example.weapon.main_gun").expect("valid weapon id"),
        },
        Intent::Spawn {
            template: TemplateId::new("example.tank.blue").expect("valid template id"),
            position: at(5.0),
            heading: Radians(0.5),
        },
        Intent::Despawn,
        Intent::Custom {
            kind: ComponentId::new("example.custom.ping").expect("valid id"),
            payload: ParamValue::from(3),
        },
    ];
    let mut with = kernel(4);
    let mut without = kernel(4);
    with.spawn_with(
        owned(at(0.0), &model),
        agent(emitting("test.behavior.loud", recorded.clone())),
    )
    .expect("spawn_with");
    without
        .spawn_with(
            owned(at(0.0), &model),
            agent(emitting("test.behavior.loud", Vec::new())),
        )
        .expect("spawn_with");

    for _ in 0..3 {
        with.tick().expect("tick");
        without.tick().expect("tick");
    }

    assert_eq!(with.snapshot().entities[0].intents, recorded);
    assert_eq!(with.entity_count(), 1, "Spawn and Despawn were not applied");
    assert_eq!(with.state_hash(), without.state_hash());
}

/// Intents are per tick: an intent emitted in tick 1 is gone in tick 2.
#[test]
#[ignore = "WI-6"]
fn intents_of_the_previous_tick_do_not_leak_into_the_next() {
    let model = Stub::new("test.model.still", Motion::Stationary);
    let mut k = kernel(1);
    let behavior = Scripted::arc("test.behavior.once", || {
        let mut first = true;
        Box::new(move |_, _, out| {
            if std::mem::take(&mut first) {
                out.push(Intent::Despawn);
            }
            Status::Running
        })
    });
    k.spawn_with(owned(at(0.0), &model), agent(behavior))
        .expect("spawn_with");

    k.tick().expect("tick");
    assert_eq!(k.snapshot().entities[0].intents, vec![Intent::Despawn]);
    k.tick().expect("tick");

    assert_eq!(k.snapshot().entities[0].intents, Vec::<Intent>::new());
}

/// REQ-K-5: a Remote entity's behavior is never ticked; an Owned twin's is, once per tick.
#[test]
#[ignore = "WI-6"]
fn remote_entity_behavior_never_ticks() {
    let model = Stub::new("test.model.still", Motion::Stationary);
    let remote_ticks = Arc::new(AtomicUsize::new(0));
    let owned_ticks = Arc::new(AtomicUsize::new(0));
    let counting = |counter: &Arc<AtomicUsize>| {
        let counter = Arc::clone(counter);
        Scripted::arc("test.behavior.count", move || {
            let counter = Arc::clone(&counter);
            Box::new(move |_, _, _| {
                counter.fetch_add(1, Ordering::SeqCst);
                Status::Running
            })
        })
    };
    let mut k = kernel(1);
    let mut remote = owned(at(0.0), &model);
    remote.ownership = Ownership::Remote {
        source: RemoteSource::new("net.test"),
    };
    k.spawn_with(remote, agent(counting(&remote_ticks)))
        .expect("spawn_with");
    k.spawn_with(owned(at(50.0), &model), agent(counting(&owned_ticks)))
        .expect("spawn_with");

    for _ in 0..10 {
        k.tick().expect("tick");
    }

    assert_eq!(remote_ticks.load(Ordering::SeqCst), 0);
    assert_eq!(owned_ticks.load(Ordering::SeqCst), 10);
    assert_eq!(k.snapshot().entities[0].status, None);
}

/// REQ-B-1, AT-3: a behavior sees the pre-tick state, and the only way it affects the entity is
/// through intents: the state it sees in tick k is the state after tick k-1, whatever it emits.
#[test]
#[ignore = "WI-6"]
fn behavior_cannot_change_state_except_through_intents() {
    let model = Stub::new("test.model.drift", Motion::Drift(Vec3::new(0.0, 2.0, 0.0)));
    let seen: Arc<Mutex<Vec<KinematicState>>> = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&seen);
    let behavior = Scripted::arc("test.behavior.watch", move || {
        let log = Arc::clone(&log);
        Box::new(move |_, view, out| {
            log.lock().expect("log lock").push(*view.state);
            out.push(Intent::Hold);
            Status::Running
        })
    });
    let mut k = kernel(1);
    k.spawn_with(owned(at(0.0), &model), agent(behavior))
        .expect("spawn_with");
    let mut expected = vec![k.snapshot().entities[0].state];

    for _ in 0..5 {
        k.tick().expect("tick");
        expected.push(k.snapshot().entities[0].state);
    }

    let seen = seen.lock().expect("log lock").clone();
    assert_eq!(seen.len(), 5);
    for (tick, state) in seen.iter().enumerate() {
        assert_eq!(*state, expected[tick], "state seen in tick {}", tick + 1);
    }
    let motion_calls = model.calls();
    assert!(motion_calls.iter().all(|c| c == &vec![Intent::Hold]));
}

/// ADR 0013 / REQ-K-4: behavior RNG draws come from the entity's hashed stream: drawing changes
/// the hash, the same seed gives the same hash, another seed another hash.
#[test]
#[ignore = "WI-6"]
fn behavior_rng_draws_change_the_hash_and_equal_seeds_give_equal_hashes() {
    let model = Stub::new("test.model.still", Motion::Stationary);
    let drawing = || {
        Scripted::arc("test.behavior.draw", || {
            Box::new(|ctx, _, _| {
                let _ = ctx.rng.next_u64();
                Status::Running
            })
        })
    };
    let quiet = || emitting("test.behavior.quiet", Vec::new());
    let run = |seed: u64, behavior: Arc<dyn cgf_core::Behavior>| {
        let mut k: Kernel = kernel(seed);
        k.spawn_with(owned(at(0.0), &model), agent(behavior))
            .expect("spawn_with");
        k.tick().expect("tick");
        k.state_hash()
    };

    let drew = run(11, drawing());

    assert_eq!(drew, run(11, drawing()), "same seed, same hash");
    assert_ne!(
        drew,
        run(11, quiet()),
        "a draw changes the entity's RNG state"
    );
    assert_ne!(drew, run(12, drawing()), "another seed, another hash");
}

/// REQ-B-5: the behavior is given the force, the route and the sorted contacts of its entity;
/// the snapshot carries its status, intents and contacts of the last tick.
#[test]
#[ignore = "WI-6"]
fn behavior_view_and_snapshot_carry_force_route_contacts_status_and_intents() {
    let model = Stub::new("test.model.still", Motion::Stationary);
    let views: Arc<Mutex<Vec<Looked>>> = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&views);
    let behavior = Scripted::arc("test.behavior.look", move || {
        let log = Arc::clone(&log);
        Box::new(move |_, view, out| {
            log.lock().expect("log lock").push((
                view.force.cloned(),
                view.route.map(|r| (r.next, r.waypoints.len(), r.complete)),
                view.contacts.iter().map(|c| c.entity).collect(),
            ));
            out.push(Intent::Despawn);
            Status::Failure
        })
    });
    let mut k = kernel(1);
    let mut init = owned(at(0.0), &model);
    init.route = Some(route(vec![at(900.0)], 5.0));
    let observer = AgentInit {
        force: Some(Force::new("blue").expect("valid force")),
        behavior: Some(behavior),
        sensors: vec![Arc::new(RangeSensor::new("test.sensor.range", 100.0))],
    };
    k.spawn_with(init, observer).expect("spawn_with");
    k.spawn(owned(at(10.0), &model)).expect("spawn");
    k.spawn(owned(at(20.0), &model)).expect("spawn");
    k.spawn(owned(at(5000.0), &model)).expect("spawn");

    k.tick().expect("tick");

    let seen = views.lock().expect("log lock").clone();
    let ids = |raw: &[u64]| -> Vec<EntityId> {
        raw.iter()
            .map(|&n| EntityId::new(n).expect("non-zero"))
            .collect()
    };
    assert_eq!(
        seen,
        vec![(Force::new("blue").ok(), Some((0, 1, false)), ids(&[2, 3]))]
    );
    let snap = &k.snapshot().entities[0];
    assert_eq!(snap.status, Some(Status::Failure));
    assert_eq!(snap.intents, vec![Intent::Despawn]);
    assert_eq!(
        snap.contacts.iter().map(|c| c.entity).collect::<Vec<_>>(),
        ids(&[2, 3])
    );
}
