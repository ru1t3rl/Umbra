//! REQ-B-5, ASM-093 (d): `RouteView::complete` marks the tick on which the stand-in holds.
//! New in M3 (WI-6 review follow-up).

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod agents;
mod common;

use std::sync::{Arc, Mutex};

use cgf_core::{Intent, Status, Vec3};
use cgf_kernel::{AgentInit, Kernel};

use agents::Scripted;
use common::{Behavior as Motion, Stub, at, kernel, owned, route};

/// Spawns an entity whose behavior emits `FollowRoute` and logs `complete` each tick.
fn spawn_following(k: &mut Kernel, init: cgf_kernel::EntityInit) -> Arc<Mutex<Vec<bool>>> {
    let log = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&log);
    let behavior = Scripted::arc("test.behavior.watch_route", move || {
        let sink = Arc::clone(&sink);
        Box::new(move |_, view, out| {
            let complete = view.route.is_some_and(|r| r.complete);
            sink.lock().expect("log lock").push(complete);
            out.push(Intent::FollowRoute);
            Status::Running
        })
    });
    k.spawn_with(
        init,
        AgentInit {
            behavior: Some(behavior),
            ..AgentInit::default()
        },
    )
    .expect("spawn_with");
    log
}

/// `complete` is true on exactly the ticks the stand-in emits `[Hold]`.
#[test]
fn complete_turns_true_on_exactly_the_tick_the_stand_in_holds() {
    let model = Stub::new("test.model.drift", Motion::Drift(Vec3::new(0.0, 10.0, 0.0)));
    let mut standin = kernel(3);
    let mut watched = kernel(3);
    let mut init = owned(at(0.0), &model);
    init.route = Some(route(vec![at(40.0)], 6.0));
    standin.spawn(init.clone()).expect("spawn");
    let log = spawn_following(&mut watched, init);

    let mut holds = Vec::new();
    for _ in 0..200 {
        standin.tick().expect("tick");
        watched.tick().expect("tick");
        holds.push(standin.snapshot().entities[0].intents == vec![Intent::Hold]);
    }

    let complete = log.lock().expect("log lock").clone();
    assert_eq!(complete, holds);
    assert!(holds.iter().any(|h| *h) && holds.iter().any(|h| !*h));
}

/// With `next == last - 1` and the entity inside both radii, `complete` is false and the
/// stand-in advances and steers to the last waypoint.
#[test]
fn complete_is_false_one_before_last_even_inside_both_radii() {
    let model = Stub::new("test.model.still", Motion::Stationary);
    let mut k = kernel(1);
    let mut init = owned(at(0.0), &model);
    init.route = Some(route(vec![at(0.0), at(2.0)], 6.0));
    let log = spawn_following(&mut k, init);

    k.tick().expect("tick");

    assert_eq!(log.lock().expect("log lock").clone(), vec![false]);
    let intents = &k.snapshot().entities[0].intents;
    assert_eq!(
        intents.last(),
        Some(&Intent::MoveTo {
            destination: at(2.0)
        })
    );
    assert!(!intents.contains(&Intent::Hold));
}
