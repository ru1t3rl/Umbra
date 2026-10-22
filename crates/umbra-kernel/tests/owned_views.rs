//! AT-11 (publish side): `Kernel::owned_views`. Requirements: REQ-K-5, ASM-253.
//!
//! Written before WI-4 (M6 step 4), run against the implementation since.

#![forbid(unsafe_code)]

mod common;

use std::sync::Arc;

use umbra_core::dead_reckoning::{DeadReckoning, RemoteState};
use umbra_core::{Force, Vec3};
use umbra_kernel::{AgentInit, OwnedView, RemoteInit};

use common::{Behavior as Motion, Stub, at, kernel, owned, state_at};

fn remote_at(y_m: f64, model: &Arc<Stub>) -> RemoteInit {
    RemoteInit {
        source: umbra_core::RemoteSource::new("net.test"),
        motion: model.clone(),
        force: None,
        remote: RemoteState {
            state: state_at(at(y_m)),
            acceleration: Vec3::ZERO,
            angular_velocity: Vec3::ZERO,
            model: DeadReckoning::Static,
        },
    }
}

#[test]
fn given_owned_and_remote_entities_when_owned_views_then_only_owned_in_id_order() {
    let model = Stub::new("test.model.still", Motion::Stationary);
    let mut k = kernel(1);
    let a = k.spawn(owned(at(0.0), &model)).expect("spawn");
    k.spawn_remote(remote_at(5.0, &model))
        .expect("spawn_remote");
    let red = AgentInit {
        force: Force::new("red").ok(),
        ..AgentInit::default()
    };
    let c = k
        .spawn_with(owned(at(20.0), &model), red)
        .expect("spawn_with");
    k.spawn_remote(remote_at(30.0, &model))
        .expect("spawn_remote");
    k.tick().expect("tick");

    let mut views = Vec::new();
    k.owned_views(&mut views);

    assert_eq!(
        views,
        [
            OwnedView {
                id: a,
                state: state_at(at(0.0)),
                force: None
            },
            OwnedView {
                id: c,
                state: state_at(at(20.0)),
                force: Force::new("red").ok()
            },
        ]
    );
}

#[test]
fn given_a_filled_buffer_when_owned_views_then_it_is_cleared_first() {
    let model = Stub::new("test.model.still", Motion::Stationary);
    let mut k = kernel(1);
    let a = k.spawn(owned(at(0.0), &model)).expect("spawn");
    let mut views = vec![OwnedView {
        id: a,
        state: state_at(at(999.0)),
        force: None,
    }];

    k.owned_views(&mut views);
    k.owned_views(&mut views);

    assert_eq!(views.len(), 1);
    assert_eq!(views[0].state, state_at(at(0.0)));
}

#[test]
fn given_removed_owned_entity_when_owned_views_then_it_is_gone() {
    let model = Stub::new("test.model.still", Motion::Stationary);
    let mut k = kernel(1);
    let a = k.spawn(owned(at(0.0), &model)).expect("spawn");
    let b = k.spawn(owned(at(1.0), &model)).expect("spawn");
    k.remove(a).expect("remove");

    let mut views = Vec::new();
    k.owned_views(&mut views);

    assert_eq!(views.iter().map(|v| v.id).collect::<Vec<_>>(), [b]);
}
