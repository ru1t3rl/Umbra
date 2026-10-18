//! AT-16 (M3 acceptance 1) and AT-20 (`attack_subgraph_receives_target_through_binding`): the
//! patrol-and-engage example runs: blue patrols, notices red, engages, fires and resumes.
//! Requirements: M3 acceptance 1, REQ-B-6..B-12, REQ-K-5, REQ-T-6, ADR 0013, ADR 0014, ADR 0015.
//!
//! Written in step 4 of M3; all tests run since WI-16, which ran the example (blue-1 at seed 7:
//! first contact tick 328, approach 634, first `Fire` 1130, last `Fire` 2347, patrol resumed
//! 3488 when red passes the 900 m engage range; the sensor fov is 360 degrees, so contact is
//! lost by range only). The file bounds the ticks generously and checks events, not exact
//! numbers. Layout (see the example's `scenario.toml`; the ranges are in the templates):
//! blue-1 starts at (0, 0) heading east with a two-leg route (50 m, then 800 m); red-1 starts at
//! (1500 m east, 200 m north) heading west at 20 m/s; the visual sensor reaches 1200 m, the
//! engage range is 900 m and the fire range 450 m. Patrol speed is 10 m/s, the approach speed
//! (`move_to`) is 8 m/s, so the two are told apart by their `SetSpeed` intent.

#![forbid(unsafe_code)]

mod behavior_support;
mod common;

use core::time::Duration;
use std::sync::LazyLock;

use behavior_support::{
    FULL, entity, example_dir, fire_targets, first_tick, id_of, instantiate_with, load_example,
    standard,
};
use cgf_core::{EntityId, Intent, MetersPerSecond, WeaponId};
use cgf_engine::scenario::validate_path;
use cgf_engine::{EntitySnapshot, Snapshot};
use common::{Trace, run_to};

struct Run {
    trace: Trace,
    blue: EntityId,
    red: EntityId,
    builtin: EntityId,
}

/// The whole example (140 s), run once for all tests of this file.
static FULL_RUN: LazyLock<Run> = LazyLock::new(full_run);

fn full_run() -> Run {
    let run = instantiate_with(&load_example(), Duration::from_micros(500), None, None);
    let (blue, red, builtin) = (
        id_of(&run, "blue-1"),
        id_of(&run, "red-1"),
        id_of(&run, "blue-2"),
    );
    let trace = run_to(run.engine, FULL, false);
    Run {
        trace,
        blue,
        red,
        builtin,
    }
}

fn sees(e: &EntitySnapshot, who: EntityId) -> bool {
    e.contacts.iter().any(|c| c.entity == who)
}

/// Whether the intents are the route expansion `[SetSpeed(10), MoveTo]` of the patrol.
fn patrolling(e: &EntitySnapshot) -> bool {
    matches!(e.intents.first(), Some(Intent::SetSpeed(MetersPerSecond(v))) if (*v - 10.0).abs() < 1e-9)
        && fire_targets(e).is_empty()
}

/// Whether the intents are the approach of `move_to`: `SetSpeed(8)` and a `MoveTo`.
fn approaching(e: &EntitySnapshot) -> bool {
    e.intents
        .iter()
        .any(|i| matches!(i, Intent::SetSpeed(MetersPerSecond(v)) if (*v - 8.0).abs() < 1e-9))
        && e.intents.iter().any(|i| matches!(i, Intent::MoveTo { .. }))
}

/// REQ-T-6, M3 acceptance 1: the example directory validates with no diagnostics (the scenario,
/// four templates and three graphs: eight files).
#[test]
fn example_validates_with_no_diagnostics() {
    let report = validate_path(&standard(), &example_dir()).expect("registry");

    assert_eq!(report.diagnostics, []);
    assert_eq!(report.files_checked.len(), 8);
}

/// M3 acceptance 1: blue follows its route (the first waypoint is passed, `route_next` advances)
/// and fires nothing until red is seen.
#[test]
fn blue_follows_route_until_red_is_in_range() {
    let run = &*FULL_RUN;

    let contact = first_tick(&run.trace, run.blue, |e| sees(e, run.red)).expect("blue sees red");
    let first_fire = first_tick(&run.trace, run.blue, |e| !fire_targets(e).is_empty());

    assert!(
        contact > 150,
        "red starts out of the sensor's reach: first contact at tick {contact}"
    );
    let before = &run.trace.snapshots[..contact - 1];
    assert!(
        before
            .iter()
            .any(|s| entity(s, run.blue).route_next == Some((1, 2))),
        "the first waypoint was reached before the first contact"
    );
    assert!(
        before
            .iter()
            .all(|s| fire_targets(entity(s, run.blue)).is_empty())
    );
    assert!(
        first_fire.is_none_or(|t| t > contact),
        "Fire only after the first contact"
    );
    let patrol = entity(&run.trace.snapshots[contact / 2], run.blue);
    assert!(
        patrolling(patrol),
        "patrolling before contact: {:?}",
        patrol.intents
    );
}

/// M3 acceptance 1: blue engages red: it approaches (`move_to` toward red's contact position),
/// the FSM reaches `engage`, and `Fire { target: red, weapon }` is emitted.
#[test]
fn blue_engages_red() {
    let run = &*FULL_RUN;
    let approach = first_tick(&run.trace, run.blue, approaching).expect("blue approaches");
    let fire =
        first_tick(&run.trace, run.blue, |e| !fire_targets(e).is_empty()).expect("blue fires");

    assert!(
        approach < fire,
        "approach (tick {approach}) comes before the first shot (tick {fire})"
    );
    let snap: &Snapshot = &run.trace.snapshots[approach - 1];
    let blue = entity(snap, run.blue);
    let red_contact = blue
        .contacts
        .iter()
        .find(|c| c.entity == run.red)
        .expect("red is a contact");
    assert!(
        blue.intents.contains(&Intent::MoveTo {
            destination: red_contact.position
        }),
        "approaches the contact's position"
    );
    let shot = entity(&run.trace.snapshots[fire - 1], run.blue);
    assert!(shot.intents.iter().any(|i| matches!(
        i,
        Intent::Fire { target, weapon }
            if *target == run.red && Some(weapon) == WeaponId::new("example.weapon.main_gun").ok().as_ref()
    )), "{:?}", shot.intents);
    assert!(
        fire < 2400,
        "the first shot falls in the first 80 s (tick {fire})"
    );
}

/// M3 acceptance 1: when red has left, blue resumes its patrol (route expansion again) and stops
/// firing; red never fires and never has a behavior that engages.
#[test]
fn blue_resumes_patrol_when_red_leaves_range() {
    let run = &*FULL_RUN;
    let last_fire = run
        .trace
        .snapshots
        .iter()
        .rposition(|s| !fire_targets(entity(s, run.blue)).is_empty())
        .map(|i| i + 1)
        .expect("blue fired");

    let resumed = run.trace.snapshots[last_fire..]
        .iter()
        .position(|s| patrolling(entity(s, run.blue)));

    assert!(
        resumed.is_some(),
        "blue patrols again after its last shot (tick {last_fire})"
    );
    assert!(
        run.trace
            .snapshots
            .iter()
            .all(|s| fire_targets(entity(s, run.red)).is_empty()),
        "red only drives"
    );
    let final_snap = run.trace.snapshots.last().expect("a last tick");
    assert!(
        !sees(entity(final_snap, run.blue), run.red) || patrolling(entity(final_snap, run.blue))
    );
}

/// REQ-B-9, ADR 0015: the second blue tank uses the built-in graph through the spec 7.1 style
/// template: its behavior is `builtin.fsm.patrol_and_engage`, it has a status every tick, and it
/// engages red too (the built-in graph engages hostiles inside its sensor range).
#[test]
fn builtin_patrol_and_engage_entity_engages_too() {
    let run = &*FULL_RUN;

    let first = entity(&run.trace.snapshots[0], run.builtin);
    assert_eq!(
        first.behavior.as_ref().map(cgf_core::BehaviorId::as_str),
        Some("builtin.fsm.patrol_and_engage")
    );
    assert!(first.status.is_some());
    let fired = run
        .trace
        .snapshots
        .iter()
        .any(|s| fire_targets(entity(s, run.builtin)).contains(&run.red));
    assert!(fired, "blue-2 fires at red-1 at some point");
}

/// ADR 0014 (AT-20): the `attack` subgraph receives its `target` through the input binding: red's
/// ID is stored by `enemy_in_range` in the patrol graph's key `target`, copied into the child's
/// read-only key before each tick, and `move_to`/`fire` in the child act on it: the `Fire` names
/// red, and the approach goes where red is. Nothing else connects the two graphs.
#[test]
fn attack_subgraph_receives_target_through_binding() {
    let run = &*FULL_RUN;

    let shots: Vec<EntityId> = run
        .trace
        .snapshots
        .iter()
        .flat_map(|s| fire_targets(entity(s, run.blue)))
        .collect();

    assert_ne!(shots, Vec::<EntityId>::new());
    assert!(
        shots.iter().all(|t| *t == run.red),
        "every shot is at the stored target: {shots:?}"
    );
    let blue_ticks_with_status = run
        .trace
        .snapshots
        .iter()
        .filter(|s| entity(s, run.blue).status.is_some())
        .count();
    assert_eq!(blue_ticks_with_status, run.trace.snapshots.len());
}
