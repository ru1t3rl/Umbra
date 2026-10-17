//! AT-10 (world part): spawning, ids, snapshots. Requirements: REQ-K-1, REQ-C-2, REQ-K-5,
//! REQ-K-6 (partial), REQ-M-4, ASM-036, ASM-037.

#![forbid(unsafe_code)]

mod common;

use cgf_core::{EntityId, Meters, MetersPerSecond, Ownership, RemoteSource, Vec3};
use cgf_kernel::{EntityInit, KernelError, RouteInit};
use common::{Behavior, Stub, at, kernel, owned, route, state_at};

/// REQ-C-2: ids start at 1 and increase by one per spawn.
#[test]
fn entity_ids_start_at_one_and_increase() {
    let mut k = kernel(1);
    let m = Stub::new("test.stub.still", Behavior::Stationary);

    let ids: Vec<Option<u64>> = (0..3)
        .map(|i| k.spawn(owned(at(f64::from(i)), &m)).ok().map(EntityId::get))
        .collect();

    assert_eq!(ids, [Some(1), Some(2), Some(3)]);
}

/// REQ-K-1: the kernel counts its entities.
#[test]
fn entity_count_follows_spawns() {
    let mut k = kernel(1);
    let m = Stub::new("test.stub.still", Behavior::Stationary);
    assert_eq!(k.entity_count(), 0);

    assert!(k.spawn(owned(at(0.0), &m)).is_ok());
    assert!(k.spawn(owned(at(1.0), &m)).is_ok());

    assert_eq!(k.entity_count(), 2);
}

/// REQ-K-4: the kernel exposes the configured seed through `rng()`.
#[test]
fn kernel_rng_carries_the_configured_seed() {
    assert_eq!(kernel(42).rng().seed(), 42);
}

/// REQ-K-2 (route validation): a route needs at least one waypoint.
#[test]
fn spawn_rejects_empty_route() {
    let mut k = kernel(1);
    let m = Stub::new("test.stub.still", Behavior::Stationary);
    let mut init = owned(at(0.0), &m);
    init.route = Some(route(vec![], 5.0));

    let got = k.spawn(init);

    assert_eq!(got, Err(KernelError::EmptyRoute));
    assert_eq!(k.entity_count(), 0);
}

/// REQ-K-2 (route validation): a non-positive or non-finite arrival radius or cruise speed is
/// rejected. The plan's `KernelError` leaves the variant open ("invalid arrival radius/speed").
#[test]
fn spawn_rejects_invalid_arrival_radius_and_cruise_speed() {
    let m = Stub::new("test.stub.still", Behavior::Stationary);
    let bad_routes = [
        RouteInit {
            arrival_radius: Meters(-1.0),
            ..route(vec![at(10.0)], 5.0)
        },
        RouteInit {
            arrival_radius: Meters(f64::NAN),
            ..route(vec![at(10.0)], 5.0)
        },
        RouteInit {
            cruise_speed: MetersPerSecond(f64::NAN),
            ..route(vec![at(10.0)], 5.0)
        },
        RouteInit {
            cruise_speed: MetersPerSecond(-3.0),
            ..route(vec![at(10.0)], 5.0)
        },
    ];

    for bad in bad_routes {
        let mut k = kernel(1);
        let mut init = owned(at(0.0), &m);
        init.route = Some(bad.clone());

        assert!(k.spawn(init).is_err(), "{bad:?}");
        assert_eq!(k.entity_count(), 0);
    }
}

/// REQ-K-6 (partial), ASM-037: a snapshot lists entities in ascending id order, whatever shapes
/// the entities have (with or without routes, owned or remote), and mirrors their state.
#[test]
fn snapshot_sorted_by_id_and_matches_state() {
    let mut k = kernel(1);
    let still = Stub::new("test.stub.still", Behavior::Stationary);
    let drift = Stub::new(
        "test.stub.drift",
        Behavior::Drift(Vec3 {
            x: 0.0,
            y: 1.0,
            z: 0.0,
        }),
    );
    let remote = Ownership::Remote {
        source: RemoteSource::new("dis.test"),
    };
    let inits = vec![
        owned(at(0.0), &still),
        EntityInit {
            route: Some(route(vec![at(50.0), at(60.0)], 4.0)),
            ..owned(at(1.0), &drift)
        },
        EntityInit {
            ownership: remote.clone(),
            ..owned(at(2.0), &still)
        },
        EntityInit {
            route: Some(route(vec![at(90.0)], 4.0)),
            ..owned(at(3.0), &still)
        },
    ];
    for init in inits {
        assert!(k.spawn(init).is_ok());
    }

    let snap = k.snapshot();

    let ids: Vec<u64> = snap.entities.iter().map(|e| e.id.get()).collect();
    assert_eq!(ids, [1, 2, 3, 4]);
    assert_eq!(snap.time, k.now());
    let positions: Vec<Vec3> = snap.entities.iter().map(|e| e.state.position.0).collect();
    assert_eq!(positions, [at(0.0).0, at(1.0).0, at(2.0).0, at(3.0).0]);
    assert_eq!(snap.entities[0].state, state_at(at(0.0)));
    let owners: Vec<Ownership> = snap.entities.iter().map(|e| e.ownership.clone()).collect();
    assert_eq!(
        owners,
        [Ownership::Owned, Ownership::Owned, remote, Ownership::Owned]
    );
    let models: Vec<&str> = snap
        .entities
        .iter()
        .map(|e| e.motion_model.as_str())
        .collect();
    assert_eq!(
        models,
        [
            "test.stub.still",
            "test.stub.drift",
            "test.stub.still",
            "test.stub.still"
        ]
    );
    let routes: Vec<Option<(usize, usize)>> = snap.entities.iter().map(|e| e.route_next).collect();
    assert_eq!(routes, [None, Some((0, 2)), None, Some((0, 1))]);
}

/// REQ-K-6 (partial): a snapshot is an immutable copy; ticking afterwards does not change it.
#[test]
fn snapshot_is_unaffected_by_later_ticks() {
    let mut k = kernel(1);
    let drift = Stub::new(
        "test.stub.drift",
        Behavior::Drift(Vec3 {
            x: 0.0,
            y: 30.0,
            z: 0.0,
        }),
    );
    assert!(k.spawn(owned(at(0.0), &drift)).is_ok());
    let before = k.snapshot();
    let copy = before.clone();

    assert!(k.tick().is_ok());
    let after = k.snapshot();

    assert_eq!(before, copy);
    assert_ne!(before, after);
    assert_eq!(after.time.tick(), before.time.tick() + 1);
}

/// REQ-M-4: the model is chosen per entity instance; the kernel never names a concrete model.
#[test]
fn each_entity_uses_its_own_motion_model_instance() {
    let mut k = kernel(1);
    let drift = Stub::new(
        "test.stub.drift",
        Behavior::Drift(Vec3 {
            x: 0.0,
            y: 30.0,
            z: 0.0,
        }),
    );
    let still = Stub::new("test.stub.still", Behavior::Stationary);
    assert!(k.spawn(owned(at(0.0), &drift)).is_ok());
    assert!(k.spawn(owned(at(100.0), &still)).is_ok());

    assert!(k.tick().is_ok());

    let snap = k.snapshot();
    assert_eq!(
        snap.entities[0].state.position.0.y, 1.0,
        "30 m/s over one 30 Hz tick"
    );
    assert_eq!(snap.entities[1].state.position, at(100.0));
    assert_eq!(drift.calls().len(), 1);
    assert_eq!(still.calls().len(), 1);
}
