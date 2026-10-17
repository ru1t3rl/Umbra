//! AT-6 (kernel part): the state hash content and stability. Requirements: REQ-K-4, ADR 0007.
//!
//! One field at a time is varied against a baseline; each variation must change the hash.
//! Hash content (ADR 0007, version 1): tick, tick rate, entity count, then per entity in id order:
//! id, ownership, motion model id, kinematic state, route state (waypoint count, next index),
//! RNG state. Intents and wall timings are never hashed.

#![forbid(unsafe_code)]

mod common;

use core::time::Duration;

use cgf_core::{
    Ecef, KinematicState, Ownership, Quaternion, RemoteSource, StateHash, TickRate, Vec3,
};
use cgf_core::{EntityId, FlatTerrain, Meters, SimRng, StateHasher, StreamKey};
use cgf_kernel::{ClockMode, EntityInit, Kernel, KernelConfig, ManualWallClock, RouteInit};
use common::{Behavior, Stub, at, kernel, owned, route, state_at};

/// Everything that goes into one single-entity kernel.
#[derive(Clone)]
struct Spec {
    seed: u64,
    hz: u32,
    extra_entity: bool,
    state: KinematicState,
    ownership: Ownership,
    model_id: &'static str,
    route: Option<RouteInit>,
    ticks: u64,
}

impl Spec {
    fn baseline() -> Self {
        Self {
            seed: 7,
            hz: 30,
            extra_entity: false,
            state: state_at(at(0.0)),
            ownership: Ownership::Owned,
            model_id: "test.stub.still",
            route: Some(route(vec![at(100.0), at(200.0)], 5.0)),
            ticks: 0,
        }
    }

    #[expect(clippy::expect_used, reason = "test helper")]
    fn hash(&self) -> StateHash {
        let config = KernelConfig {
            tick_rate: TickRate::new(self.hz).expect("non-zero rate"),
            seed: self.seed,
            clock: ClockMode::AsFastAsPossible,
        };
        let mut k = Kernel::new(
            config,
            Box::new(FlatTerrain {
                height: Meters(0.0),
            }),
            Box::new(ManualWallClock::new()),
        );
        let m = Stub::new(self.model_id, Behavior::Stationary);
        let init = EntityInit {
            state: self.state,
            ownership: self.ownership.clone(),
            motion: m.clone(),
            route: self.route.clone(),
        };
        k.spawn(init).expect("spawn");
        if self.extra_entity {
            k.spawn(owned(at(500.0), &m)).expect("spawn extra");
        }
        for _ in 0..self.ticks {
            k.tick().expect("tick");
        }
        k.state_hash()
    }
}

/// REQ-K-4: asking twice gives the same hash (it has no side effects).
#[test]
fn state_hash_twice_is_equal() {
    let k = kernel(1);

    assert_eq!(k.state_hash(), k.state_hash());
}

/// REQ-K-4: two kernels built the same way hash alike, now and after the same ticks.
#[test]
fn identical_kernels_hash_alike() {
    let mut spec = Spec::baseline();
    let a = spec.hash();
    spec.ticks = 5;

    assert_eq!(a, Spec::baseline().hash());
    assert_eq!(spec.hash(), spec.clone().hash());
}

/// ADR 0007: every hashed field changes the hash when varied alone (tick, tick rate, entity
/// count, position, velocity, orientation, ownership, remote source, motion model id, route
/// waypoint count, RNG state through the seed).
#[test]
fn state_hash_changes_with_each_hashed_field() {
    let base_hash = Spec::baseline().hash();

    for (field, spec) in variations(&Spec::baseline()) {
        assert_ne!(spec.hash(), base_hash, "hash ignores {field}");
    }
}

/// One variation of `base` per hashed field.
fn variations(base: &Spec) -> Vec<(&'static str, Spec)> {
    vec![
        (
            "tick",
            Spec {
                ticks: 1,
                ..base.clone()
            },
        ),
        (
            "tick rate",
            Spec {
                hz: 60,
                ..base.clone()
            },
        ),
        (
            "entity count",
            Spec {
                extra_entity: true,
                ..base.clone()
            },
        ),
        (
            "position",
            Spec {
                state: state_at(Ecef(Vec3 {
                    x: common::A,
                    y: 0.001,
                    z: 0.0,
                })),
                ..base.clone()
            },
        ),
        (
            "velocity",
            Spec {
                state: KinematicState {
                    velocity: Vec3 {
                        x: 0.0,
                        y: 1.0,
                        z: 0.0,
                    },
                    ..base.state
                },
                ..base.clone()
            },
        ),
        (
            "orientation",
            Spec {
                state: KinematicState {
                    orientation: Quaternion {
                        w: 0.0,
                        x: 1.0,
                        y: 0.0,
                        z: 0.0,
                    },
                    ..base.state
                },
                ..base.clone()
            },
        ),
        (
            "ownership",
            Spec {
                ownership: Ownership::Remote {
                    source: RemoteSource::new("dis.a"),
                },
                ..base.clone()
            },
        ),
        (
            "motion model id",
            Spec {
                model_id: "test.stub.other",
                ..base.clone()
            },
        ),
        (
            "route absent",
            Spec {
                route: None,
                ..base.clone()
            },
        ),
        (
            "route waypoint count",
            Spec {
                route: Some(route(vec![at(100.0), at(200.0), at(300.0)], 5.0)),
                ..base.clone()
            },
        ),
        (
            "rng state (seed)",
            Spec {
                seed: 8,
                ..base.clone()
            },
        ),
    ]
}

/// ADR 0007: the remote source string is hashed, not just the ownership tag.
#[test]
fn state_hash_changes_with_remote_source() {
    let remote = |s: &str| Spec {
        ownership: Ownership::Remote {
            source: RemoteSource::new(s),
        },
        ..Spec::baseline()
    };

    assert_ne!(remote("dis.a").hash(), remote("dis.b").hash());
}

/// ADR 0007: the route's next index is hashed. Both entities stand still and have two
/// waypoints; only whether the first waypoint is within the arrival radius differs, so after one
/// tick one route has advanced and the other has not.
#[test]
fn state_hash_changes_with_route_next_index() {
    let build = |first_waypoint_y: f64| {
        let mut k = kernel(7);
        let m = Stub::new("test.stub.still", Behavior::Stationary);
        let init = EntityInit {
            route: Some(route(vec![at(first_waypoint_y), at(900.0)], 5.0)),
            ..owned(at(0.0), &m)
        };
        assert!(k.spawn(init).is_ok());
        assert!(k.tick().is_ok());
        k
    };

    let near = build(1.0);
    let far = build(100.0);

    assert_eq!(near.snapshot().entities[0].route_next, Some((1, 2)));
    assert_eq!(far.snapshot().entities[0].route_next, Some((0, 2)));
    assert_ne!(near.state_hash(), far.state_hash());
}

/// ADR 0007: wall timings and the clock mode are never hashed. The same scenario run on a plain
/// AFAP kernel and on a real-time kernel whose wall clock advances on every read hashes alike.
#[test]
fn state_hash_ignores_intents_and_wall_timings() {
    let run = |clock: ClockMode, wall: ManualWallClock| {
        let mut k = common::kernel_with(3, clock, wall);
        let m = Stub::new(
            "test.stub.drift",
            Behavior::Drift(Vec3 {
                x: 0.0,
                y: 3.0,
                z: 0.0,
            }),
        );
        let init = EntityInit {
            route: Some(route(vec![at(100.0), at(200.0)], 5.0)),
            ..owned(at(0.0), &m)
        };
        assert!(k.spawn(init).is_ok());
        for _ in 0..20 {
            assert!(k.tick().is_ok());
        }
        k.state_hash()
    };

    let plain = run(ClockMode::AsFastAsPossible, ManualWallClock::new());
    let timed = run(
        ClockMode::RealTime,
        ManualWallClock::with_auto_advance(Duration::from_millis(50)),
    );

    assert_eq!(plain, timed);
}

/// ADR 0007, ASM-037: the hash is a per-id sequence, not a bag. Swapping which id holds which
/// state changes it.
#[test]
fn hash_is_sensitive_to_which_id_holds_which_state() {
    let build = |first: f64, second: f64| {
        let mut k = kernel(7);
        let m = Stub::new("test.stub.still", Behavior::Stationary);
        assert!(k.spawn(owned(at(first), &m)).is_ok());
        assert!(k.spawn(owned(at(second), &m)).is_ok());
        k.state_hash()
    };

    assert_ne!(build(10.0, 20.0), build(20.0, 10.0));
}

/// ADR 0007, REQ-K-4: entities whose component sets differ (so an ECS stores them in different
/// archetypes) interleave across ids; two kernels built identically still hash alike, and the
/// snapshot order (which the hash follows) is ascending by id.
///
/// The plan also asks for kernels whose archetypes were created in a different order; with
/// ids assigned by spawn order and no despawn in M1, that cannot be arranged through the public
/// API (see the M1 running log). The cross-process test in `cgf-cli` covers the remainder.
#[test]
fn hash_walks_entities_in_id_order() {
    let build = || {
        let mut k = kernel(7);
        let m = Stub::new("test.stub.still", Behavior::Stationary);
        for i in 0..6_u32 {
            let mut init = owned(at(f64::from(i) * 10.0), &m);
            if i % 2 == 0 {
                init.route = Some(route(vec![at(1000.0)], 5.0));
            }
            assert!(k.spawn(init).is_ok());
        }
        k
    };

    let (a, b) = (build(), build());

    assert_eq!(a.state_hash(), b.state_hash());
    let ids: Vec<u64> = a.snapshot().entities.iter().map(|e| e.id.get()).collect();
    assert_eq!(ids, [1, 2, 3, 4, 5, 6]);
}

#[expect(clippy::expect_used, reason = "test helper")]
/// Hashes one entity by hand in the ADR 0007 field order. `route` is `(waypoints, next)`.
fn hash_entity(
    h: &mut StateHasher,
    id: u64,
    seed: u64,
    model: &str,
    state: &KinematicState,
    route: Option<(u64, u64)>,
) {
    h.write_u64(id);
    h.write_u8(0); // Owned
    h.write_str(model);
    h.write_vec3(state.position.0);
    h.write_vec3(state.velocity);
    h.write_quaternion(state.orientation);
    match route {
        None => h.write_u8(0),
        Some((count, next)) => {
            h.write_u8(1);
            h.write_u64(count);
            h.write_u64(next);
        }
    }
    let entity_id = EntityId::new(id).expect("non-zero id");
    for word in SimRng::new(seed)
        .stream(StreamKey::entity(entity_id))
        .state()
    {
        h.write_u64(word);
    }
}

/// ADR 0007: the kernel hash equals a hand-built encoding in the documented order: header
/// (tick, tick rate, entity count), then per entity in ascending id order, whatever the ECS
/// archetypes are (route on odd ids only here, so storage order differs from id order), each
/// with its own RNG stream. This pins walk order, the count, the absent-route tag and the
/// per-entity stream keys.
#[test]
fn state_hash_equals_the_reference_encoding() {
    let seed = 11;
    let mut k = kernel(seed);
    let m = Stub::new("test.stub.still", Behavior::Stationary);
    for i in 0..3_u32 {
        let mut init = owned(at(f64::from(i) * 10.0), &m);
        if i % 2 == 0 {
            init.route = Some(route(vec![at(1000.0), at(2000.0)], 5.0));
        }
        assert!(k.spawn(init).is_ok());
    }

    let mut h = StateHasher::new();
    h.write_u64(0); // tick
    h.write_u32(30); // tick rate
    h.write_u64(3); // entity count
    for (id, route) in [(1, Some((2, 0))), (2, None), (3, Some((2, 0)))] {
        let state = state_at(at(f64::from(u32::try_from(id - 1).expect("small")) * 10.0));
        hash_entity(&mut h, id, seed, "test.stub.still", &state, route);
    }

    assert_eq!(k.state_hash(), h.finish());
}
