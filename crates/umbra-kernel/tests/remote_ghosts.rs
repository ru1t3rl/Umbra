//! AT-11: remote tracks in the kernel. Requirements: REQ-K-2, REQ-K-4, REQ-K-5, D-16, ADR 0032,
//! ASM-230, ASM-242, ASM-253, ASM-267.
//!
//! Written before WI-4 (M6 step 4), run against the implementation since. The time rule the
//! tests pin (ASM-267): a basis given between ticks is taken at the kernel's current time `T`; the
//! tick that runs at `T + k` ticks leaves the entity at `extrapolate(basis, k / hz)`.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod agents;
mod common;

use std::sync::Arc;

use umbra_core::dead_reckoning::{DeadReckoning, RemoteState, extrapolate};
use umbra_core::{
    Ecef, EntityId, Force, Intent, KinematicState, Ownership, RemoteSource, Seconds, SimEvent,
    Status, Vec3, WeaponId,
};
use umbra_kernel::{AgentInit, Kernel, KernelError, RemoteInit};

use agents::{RangeSensor, Scripted};
use common::{Behavior as Motion, Stub, at, kernel, owned, state_at};

const HZ: f64 = 30.0;

fn id(raw: u64) -> EntityId {
    EntityId::new(raw).expect("non-zero")
}

fn source() -> RemoteSource {
    RemoteSource::new("net.test")
}

fn basis(y_m: f64, velocity: Vec3, model: DeadReckoning) -> RemoteState {
    RemoteState {
        state: KinematicState {
            velocity,
            ..state_at(at(y_m))
        },
        acceleration: Vec3::new(0.0, 0.5, 0.0),
        angular_velocity: Vec3::new(0.0, 0.0, 0.1),
        model,
    }
}

fn init(model: &Arc<Stub>, remote: RemoteState) -> RemoteInit {
    RemoteInit {
        source: source(),
        motion: model.clone(),
        force: Force::new("red").ok(),
        remote,
    }
}

fn still() -> Arc<Stub> {
    Stub::new("test.model.still", Motion::Stationary)
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

fn assert_state_close(got: &KinematicState, want: &KinematicState, what: &str) {
    let pairs = [
        (got.position.0.x, want.position.0.x),
        (got.position.0.y, want.position.0.y),
        (got.position.0.z, want.position.0.z),
        (got.velocity.x, want.velocity.x),
        (got.velocity.y, want.velocity.y),
        (got.velocity.z, want.velocity.z),
        (got.orientation.w, want.orientation.w),
        (got.orientation.x, want.orientation.x),
        (got.orientation.y, want.orientation.y),
        (got.orientation.z, want.orientation.z),
    ];
    assert!(
        pairs.iter().all(|&(g, w)| close(g, w)),
        "{what}: {got:?} vs {want:?}"
    );
}

fn ghost_state(k: &Kernel, index: usize) -> KinematicState {
    k.snapshot().entities[index].state
}

#[test]
fn given_spawned_remote_then_it_is_remote_without_behavior_sensors_or_intents() {
    let model = still();
    let mut k = kernel(1);
    let ghost = k
        .spawn_remote(init(&model, basis(10.0, Vec3::ZERO, DeadReckoning::Static)))
        .expect("spawn_remote");

    k.tick().expect("tick");
    let snap = k.snapshot();
    let e = &snap.entities[0];

    assert_eq!(e.id, ghost);
    assert_eq!(e.ownership, Ownership::Remote { source: source() });
    assert_eq!(e.force, Force::new("red").ok());
    assert_eq!(e.behavior, None);
    assert_eq!(e.status, None);
    assert_eq!(e.intents, Vec::<Intent>::new());
    assert_eq!(e.contacts, Vec::<umbra_core::Contact>::new());
}

#[test]
fn given_remote_spawned_when_not_yet_ticked_then_its_state_is_the_basis() {
    let model = still();
    let mut k = kernel(1);
    let remote = basis(10.0, Vec3::new(0.0, 5.0, 0.0), DeadReckoning::Velocity);
    k.spawn_remote(init(&model, remote)).expect("spawn_remote");

    assert_eq!(ghost_state(&k, 0), remote.state);
}

#[test]
fn given_remote_in_each_model_when_ticked_then_state_is_the_extrapolated_basis_at_that_tick() {
    let models = [
        DeadReckoning::Static,
        DeadReckoning::Velocity,
        DeadReckoning::Acceleration,
        DeadReckoning::VelocityRotation,
        DeadReckoning::AccelerationRotation,
    ];
    for dr in models {
        let model = still();
        let mut k = kernel(1);
        let remote = basis(100.0, Vec3::new(0.0, 12.0, 3.0), dr);
        k.spawn_remote(init(&model, remote)).expect("spawn_remote");

        for ticks_after_basis in 0..40_u32 {
            k.tick().expect("tick");

            let want = extrapolate(&remote, Seconds(f64::from(ticks_after_basis) / HZ));
            assert_state_close(
                &ghost_state(&k, 0),
                &want,
                &format!("{dr:?}, {ticks_after_basis} ticks after the basis"),
            );
        }
    }
}

#[test]
fn given_ticks_run_when_update_remote_then_the_next_tick_shows_the_new_basis_exactly() {
    let model = still();
    let mut k = kernel(1);
    let id = k
        .spawn_remote(init(
            &model,
            basis(0.0, Vec3::new(0.0, 5.0, 0.0), DeadReckoning::Velocity),
        ))
        .expect("spawn_remote");
    for _ in 0..7 {
        k.tick().expect("tick");
    }
    let fresh = basis(5_000.0, Vec3::new(0.0, -20.0, 0.0), DeadReckoning::Velocity);

    k.update_remote(id, fresh).expect("update_remote");
    k.tick().expect("tick");
    let at_basis = ghost_state(&k, 0);
    k.tick().expect("tick");
    let one_later = ghost_state(&k, 0);

    assert_eq!(at_basis, fresh.state);
    assert_state_close(
        &one_later,
        &extrapolate(&fresh, Seconds(1.0 / HZ)),
        "one tick after the update",
    );
}

#[test]
fn given_update_remote_when_not_yet_ticked_then_the_state_is_the_new_basis() {
    let model = still();
    let mut k = kernel(1);
    let id = k
        .spawn_remote(init(&model, basis(0.0, Vec3::ZERO, DeadReckoning::Static)))
        .expect("spawn_remote");
    let fresh = basis(77.0, Vec3::new(0.0, 4.0, 0.0), DeadReckoning::Velocity);

    k.update_remote(id, fresh).expect("update_remote");

    assert_eq!(ghost_state(&k, 0), fresh.state);
}

#[test]
fn given_owned_entity_when_update_remote_then_not_remote_and_kernel_unchanged() {
    let model = still();
    let mut k = kernel(1);
    let owned_id = k.spawn(owned(at(0.0), &model)).expect("spawn");
    k.tick().expect("tick");
    let (hash, snapshot) = (k.state_hash(), k.snapshot());

    let result = k.update_remote(owned_id, basis(9.0, Vec3::ZERO, DeadReckoning::Static));

    assert_eq!(result, Err(KernelError::NotRemote(owned_id)));
    assert_eq!(k.state_hash(), hash);
    assert_eq!(k.snapshot(), snapshot);
}

#[test]
fn given_unknown_or_removed_id_when_update_remote_then_unknown_entity() {
    let model = still();
    let mut k = kernel(1);
    let ghost = k
        .spawn_remote(init(&model, basis(0.0, Vec3::ZERO, DeadReckoning::Static)))
        .expect("spawn_remote");
    let fresh = basis(1.0, Vec3::ZERO, DeadReckoning::Static);

    assert_eq!(
        k.update_remote(id(99), fresh),
        Err(KernelError::UnknownEntity(id(99)))
    );
    k.remove(ghost).expect("remove");
    assert_eq!(
        k.update_remote(ghost, fresh),
        Err(KernelError::UnknownEntity(ghost))
    );
}

#[test]
fn given_remote_with_a_moving_motion_model_when_ticked_then_the_model_is_never_stepped() {
    let drifting = Stub::new(
        "test.model.drift",
        Motion::Drift(Vec3::new(0.0, 900.0, 0.0)),
    );
    let mut k = kernel(1);
    k.spawn_remote(init(
        &drifting,
        basis(10.0, Vec3::ZERO, DeadReckoning::Static),
    ))
    .expect("spawn_remote");

    for _ in 0..5 {
        k.tick().expect("tick");
    }

    assert!(drifting.calls().is_empty(), "motion model stepped");
    assert_eq!(ghost_state(&k, 0).position, at(10.0));
}

#[test]
fn given_owned_shooter_next_to_a_ghost_when_ticked_then_no_event_names_the_ghost_as_source() {
    let model = still();
    let mut k = kernel(1);
    let shooter_behavior = Scripted::arc("test.behavior.loud", || {
        Box::new(|_, _, out| {
            out.push(Intent::Fire {
                target: id(2),
                weapon: WeaponId::new("example.weapon.main_gun").expect("valid id"),
            });
            Status::Running
        })
    });
    let shooter = k
        .spawn_with(
            owned(at(0.0), &model),
            AgentInit {
                behavior: Some(shooter_behavior),
                ..AgentInit::default()
            },
        )
        .expect("spawn_with");
    let ghost = k
        .spawn_remote(init(&model, basis(10.0, Vec3::ZERO, DeadReckoning::Static)))
        .expect("spawn_remote");
    assert_eq!(ghost, id(2));

    let events: Vec<SimEvent> = (0..3)
        .flat_map(|_| k.tick().expect("tick").events)
        .collect();

    assert_eq!(events.len(), 3, "one Fire per tick: {events:?}");
    assert!(
        events
            .iter()
            .all(|e| matches!(e, SimEvent::Fire { shooter: s, target, .. } if *s == shooter && *target == ghost)),
        "{events:?}"
    );
}

#[test]
fn given_owned_sensor_when_ghost_approaches_then_it_is_perceived_at_its_dead_reckoned_position() {
    let model = still();
    let mut k = kernel(1);
    k.spawn_with(
        owned(at(0.0), &model),
        AgentInit {
            sensors: vec![Arc::new(RangeSensor::new("test.sensor.range", 105.0))],
            ..AgentInit::default()
        },
    )
    .expect("spawn_with");
    // 300 m/s towards the observer from 500 m: 10 m per tick, so 100 m away 40 ticks after the basis.
    let ghost = k
        .spawn_remote(init(
            &model,
            basis(500.0, Vec3::new(0.0, -300.0, 0.0), DeadReckoning::Velocity),
        ))
        .expect("spawn_remote");

    let mut contacts_per_tick = Vec::new();
    for _ in 0..41 {
        k.tick().expect("tick");
        contacts_per_tick.push(k.snapshot().entities[0].contacts.clone());
    }

    assert!(
        contacts_per_tick[39].is_empty(),
        "110 m away is out of range"
    );
    let seen = &contacts_per_tick[40];
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].entity, ghost);
    assert!(
        (seen[0].position.0.y - 100.0).abs() < 1e-6,
        "contact at {:?}",
        seen[0].position
    );
}

#[test]
fn given_world_with_a_ghost_when_ticked_then_owned_entities_are_unaffected() {
    let model = still();
    let run = |with_ghost: bool| {
        let mut k = kernel(7);
        let drift = Stub::new("test.model.drift", Motion::Drift(Vec3::new(1.0, 2.0, 0.0)));
        k.spawn(owned(at(0.0), &drift)).expect("spawn");
        if with_ghost {
            k.spawn_remote(init(
                &model,
                basis(40.0, Vec3::new(0.0, 3.0, 0.0), DeadReckoning::Velocity),
            ))
            .expect("spawn_remote");
        }
        for _ in 0..20 {
            k.tick().expect("tick");
        }
        k.snapshot().entities[0].clone()
    };

    assert_eq!(run(true), run(false));
}

#[test]
fn given_a_non_finite_basis_when_spawn_remote_then_invalid_state_and_the_kernel_is_unchanged() {
    let model = still();
    let mut k = kernel(1);
    k.spawn(owned(at(0.0), &model)).expect("spawn");
    k.tick().expect("tick");
    let (hash, snapshot) = (k.state_hash(), k.snapshot());
    let mut bad = basis(10.0, Vec3::ZERO, DeadReckoning::Static);
    bad.state.position = Ecef(Vec3::new(f64::NAN, 0.0, 0.0));

    let result = k.spawn_remote(init(&model, bad));

    assert_eq!(result, Err(KernelError::InvalidState));
    assert_eq!(k.state_hash(), hash);
    assert_eq!(k.snapshot(), snapshot);
    let next = k
        .spawn_remote(init(&model, basis(10.0, Vec3::ZERO, DeadReckoning::Static)))
        .expect("spawn_remote");
    assert_eq!(next, id(2), "the failed spawn consumed no id");
}

fn moving_ghost(model: &Arc<Stub>, y_m: f64) -> RemoteInit {
    init(
        model,
        basis(y_m, Vec3::new(0.0, 10.0, 0.0), DeadReckoning::Velocity),
    )
}

#[test]
fn given_an_owned_entity_removed_next_to_a_moving_ghost_then_the_ghost_keeps_dead_reckoning() {
    let model = still();
    let mut k = kernel(1);
    let owned_id = k.spawn(owned(at(0.0), &model)).expect("spawn");
    let remote = basis(50.0, Vec3::new(0.0, 10.0, 0.0), DeadReckoning::Velocity);
    k.spawn_remote(init(&model, remote)).expect("spawn_remote");
    k.remove(owned_id).expect("remove");

    for n in 0..5_u32 {
        k.tick().expect("tick");
        let want = extrapolate(&remote, Seconds(f64::from(n) / HZ));
        assert_state_close(&ghost_state(&k, 0), &want, "ghost after the owned removal");
    }
}

#[test]
fn given_two_ghosts_when_one_is_removed_then_the_other_keeps_moving() {
    let model = still();
    let mut k = kernel(1);
    let first = k
        .spawn_remote(moving_ghost(&model, 0.0))
        .expect("spawn_remote");
    let remote = basis(50.0, Vec3::new(0.0, 10.0, 0.0), DeadReckoning::Velocity);
    k.spawn_remote(init(&model, remote)).expect("spawn_remote");
    k.remove(first).expect("remove");

    for n in 0..5_u32 {
        k.tick().expect("tick");
        let want = extrapolate(&remote, Seconds(f64::from(n) / HZ));
        assert_state_close(&ghost_state(&k, 0), &want, "remaining ghost");
    }
}

#[test]
fn given_non_finite_basis_parts_when_spawn_or_update_remote_then_invalid_state_and_unchanged() {
    let model = still();
    let mut k = kernel(1);
    let owned_id = k.spawn(owned(at(0.0), &model)).expect("spawn");
    let ghost = k
        .spawn_remote(moving_ghost(&model, 10.0))
        .expect("spawn_remote");
    for _ in 0..3 {
        k.tick().expect("tick");
    }
    let (hash, snapshot) = (k.state_hash(), k.snapshot());
    let good = basis(1.0, Vec3::ZERO, DeadReckoning::Velocity);
    let mut bad_velocity = good;
    bad_velocity.state.velocity = Vec3::new(0.0, f64::NAN, 0.0);
    let mut bad_acceleration = good;
    bad_acceleration.acceleration = Vec3::new(f64::INFINITY, 0.0, 0.0);
    let mut bad_rate = good;
    bad_rate.angular_velocity = Vec3::new(0.0, 0.0, f64::NAN);

    for bad in [bad_velocity, bad_acceleration, bad_rate] {
        assert_eq!(k.update_remote(ghost, bad), Err(KernelError::InvalidState));
        assert_eq!(
            k.spawn_remote(init(&model, bad)),
            Err(KernelError::InvalidState)
        );
        // ASM-283: the Owned check comes before the state check.
        assert_eq!(
            k.update_remote(owned_id, bad),
            Err(KernelError::NotRemote(owned_id))
        );
    }

    assert_eq!(k.state_hash(), hash);
    assert_eq!(k.snapshot(), snapshot);
}

/// Pins how Owned entities are hashed (ASM-230): the per-tick state hashes of a fixed three-entity
/// world without Remote entities, taken from the M5 tree (goldens unchanged since). The gate-4
/// goldens cover the walking skeleton.
#[test]
fn given_a_world_without_remote_entities_then_per_tick_hashes_are_the_m5_values() {
    let drift = Stub::new("test.model.drift", Motion::Drift(Vec3::new(0.0, 3.0, 0.0)));
    let still = still();
    let mut k = kernel(42);
    k.spawn(owned(at(0.0), &drift)).expect("spawn");
    k.spawn(owned(at(100.0), &still)).expect("spawn");
    k.spawn(owned(at(200.0), &drift)).expect("spawn");

    let hashes: Vec<String> = (0..5)
        .map(|_| {
            k.tick().expect("tick");
            k.state_hash().to_string()
        })
        .collect();

    assert_eq!(hashes, PINNED);
}

const PINNED: [&str; 5] = [
    "3a77ce5775e70315",
    "1e22bcc64b6546b8",
    "661a9fc53e4c1331",
    "a005bfab0886018a",
    "c213e5593b8b50ad",
];

#[test]
fn given_a_remote_timeout_when_a_ghost_is_not_refreshed_then_elapsed_is_capped_at_the_timeout() {
    // ADR 0032: the caller caps `elapsed` at the adapter's ghost timeout (ASM-457).
    let model = still();
    let mut k = kernel(1);
    k.set_remote_timeouts([(&source(), Seconds(0.5))])
        .expect("timeouts");
    let remote = basis(0.0, Vec3::new(0.0, 10.0, 0.0), DeadReckoning::Velocity);
    k.spawn_remote(init(&model, remote)).expect("spawn_remote");
    let other = RemoteInit {
        source: RemoteSource::new("net.other"),
        ..init(&model, remote)
    };
    k.spawn_remote(other).expect("spawn_remote");

    for _ in 0..60 {
        k.tick().expect("tick");
    }

    let capped = extrapolate(&remote, Seconds(0.5));
    assert_state_close(&ghost_state(&k, 0), &capped, "ghost with a timeout");
    let free = extrapolate(&remote, Seconds(59.0 / HZ));
    assert_state_close(
        &ghost_state(&k, 1),
        &free,
        "ghost of a source without timeout",
    );
}

#[test]
fn given_invalid_timeouts_when_set_then_error_and_the_previous_caps_stay() {
    let model = still();
    let mut k = kernel(1);
    k.set_remote_timeouts([(&source(), Seconds(0.5))])
        .expect("timeouts");

    for bad in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        let result = k.set_remote_timeouts([(&source(), Seconds(bad))]);
        assert_eq!(result, Err(KernelError::InvalidRemoteTimeout), "{bad}");
    }

    let remote = basis(0.0, Vec3::new(0.0, 10.0, 0.0), DeadReckoning::Velocity);
    k.spawn_remote(init(&model, remote)).expect("spawn_remote");
    for _ in 0..60 {
        k.tick().expect("tick");
    }
    assert_state_close(
        &ghost_state(&k, 0),
        &extrapolate(&remote, Seconds(0.5)),
        "the first cap still applies",
    );
}

#[test]
fn given_new_timeouts_when_set_then_stale_caps_of_other_sources_are_dropped() {
    let model = still();
    let mut k = kernel(1);
    k.set_remote_timeouts([(&source(), Seconds(0.5))])
        .expect("timeouts");
    let other = RemoteSource::new("net.other");
    k.set_remote_timeouts([(&other, Seconds(0.5))])
        .expect("timeouts");
    let remote = basis(0.0, Vec3::new(0.0, 10.0, 0.0), DeadReckoning::Velocity);
    k.spawn_remote(init(&model, remote)).expect("spawn_remote");

    for _ in 0..60 {
        k.tick().expect("tick");
    }

    assert_state_close(
        &ghost_state(&k, 0),
        &extrapolate(&remote, Seconds(59.0 / HZ)),
        "uncapped",
    );
}
