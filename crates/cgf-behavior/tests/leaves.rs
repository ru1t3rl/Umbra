//! AT-11: every result path of the built-in conditions and actions (ASM-077).
//! Requirements: REQ-B-15, REQ-B-16, REQ-T-3, ASM-077, ASM-078, ASM-079, ASM-091.
//!
//! Written in step 4, before WI-11; un-ignored by WI-11. Choices the plan leaves open (recorded in the plan's running
//! log): `enemy_in_range` and `target_in_range` compare `Contact::range` with `range_m`;
//! `move_to` emits `SetSpeed(speed_mps)` and `MoveTo { contact position }` while Running and is
//! Success when the contact is within `arrival_radius_m`; `wait` is Running for `n` ticks counted
//! from its first tick (`n` = `ticks_for(duration_s)`) and Success on the next one; `timer_elapsed`
//! compares the absolute tick `now` with `n`.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

use std::sync::Arc;

use cgf_behavior::register_builtins;
use cgf_core::{
    Access, ActionInstance, BbType, BbValue, BehaviorContext, Blackboard, BlackboardParam,
    BuildError, ComponentId, ComponentKind, Condition, Contact, Ecef, EntityId, EntityView, Force,
    InstantiateError, Intent, IntentBuffer, KinematicState, Meters, MetersPerSecond, Quaternion,
    Registry, RngStream, RouteView, SimRng, SimTime, Status, StreamKey, TickRate, Vec3, WeaponId,
};
use serde_json::json;

const ENEMY: &str = "builtin.cond.enemy_in_range";
const TARGET: &str = "builtin.cond.target_in_range";
const TIMER: &str = "builtin.cond.timer_elapsed";
const CHANCE: &str = "builtin.cond.chance";
const FOLLOW: &str = "builtin.action.follow_route";
const MOVE_TO: &str = "builtin.action.move_to";
const WAIT: &str = "builtin.action.wait";
const FIRE: &str = "builtin.action.fire";
const CONDITIONS: [&str; 4] = [ENEMY, TARGET, TIMER, CHANCE];
const ACTIONS: [&str; 4] = [FOLLOW, MOVE_TO, WAIT, FIRE];

fn registry() -> Registry {
    let mut registry = Registry::new();
    register_builtins(&mut registry).expect("builtins register");
    registry
}

fn cid(s: &str) -> ComponentId {
    ComponentId::new(s).expect("valid id")
}

fn eid(n: u64) -> EntityId {
    EntityId::new(n).expect("non-zero")
}

fn rate() -> TickRate {
    TickRate::new(30).expect("30 Hz")
}

fn condition(id: &str, params: &serde_json::Value) -> Arc<dyn Condition> {
    registry()
        .condition(&cid(id))
        .expect("registered")
        .build(params)
        .expect("builds")
}

fn action(id: &str, params: &serde_json::Value) -> Box<dyn ActionInstance> {
    registry()
        .action(&cid(id))
        .expect("registered")
        .build(params)
        .expect("builds")
        .instantiate(rate())
        .expect("instantiates")
}

fn state() -> KinematicState {
    KinematicState {
        position: Ecef(Vec3::new(6_378_137.0, 0.0, 0.0)),
        velocity: Vec3::ZERO,
        orientation: Quaternion::IDENTITY,
    }
}

fn force(s: &str) -> Force {
    Force::new(s).expect("valid force")
}

/// A contact with entity `n` at `range` meters, with an optional force.
fn contact(n: u64, range: f64, side: Option<&str>) -> Contact {
    Contact::new(
        eid(n),
        Ecef(Vec3::new(6_378_137.0, range, 0.0)),
        Vec3::ZERO,
        side.map(force),
        Meters(range),
    )
}

fn stream() -> RngStream {
    SimRng::new(5).stream(StreamKey::entity(eid(1)))
}

/// Runs `check` once at `tick` for an observer with `side`, `contacts` and a blackboard.
fn check(
    cond: &dyn Condition,
    tick: u64,
    side: Option<&Force>,
    contacts: &[Contact],
    bb: &mut Blackboard,
) -> bool {
    let mut rng = stream();
    check_with(cond, tick, side, contacts, bb, &mut rng)
}

fn check_with(
    cond: &dyn Condition,
    tick: u64,
    side: Option<&Force>,
    contacts: &[Contact],
    bb: &mut Blackboard,
    rng: &mut RngStream,
) -> bool {
    let st = state();
    let mut ctx = BehaviorContext::new(SimTime::at_tick(rate(), tick), rng);
    let view = EntityView::new(eid(1), &st)
        .with_force(side)
        .with_contacts(contacts);
    cond.check(&mut ctx, &view, bb)
}

fn bb_with_target() -> Blackboard {
    let mut bb = Blackboard::default();
    bb.declare("target", BbType::Entity);
    bb
}

/// Ticks `act` once; returns the status and the intents.
fn tick(
    act: &mut dyn ActionInstance,
    at: u64,
    contacts: &[Contact],
    route: Option<RouteView<'_>>,
    bb: &mut Blackboard,
) -> (Status, Vec<Intent>) {
    let st = state();
    let mut rng = stream();
    let mut ctx = BehaviorContext::new(SimTime::at_tick(rate(), at), &mut rng);
    let mut view = EntityView::new(eid(1), &st).with_contacts(contacts);
    if let Some(r) = route {
        view = view.with_route(r);
    }
    let mut intents = Vec::new();
    let status = act.tick(&mut ctx, &view, bb, &mut IntentBuffer::new(&mut intents));
    (status, intents)
}

// --- registration ------------------------------------------------------------------------------

/// ASM-077: exactly four conditions and four actions are built in, all with `Provenance::Builtin`.
#[test]
fn register_builtins_registers_the_four_conditions_and_four_actions() {
    let registry = registry();

    let conditions: Vec<&str> = registry
        .ids(ComponentKind::Condition)
        .iter()
        .map(|i| i.as_str())
        .collect();
    let actions: Vec<&str> = registry
        .ids(ComponentKind::Action)
        .iter()
        .map(|i| i.as_str())
        .collect();

    let mut want_c = CONDITIONS.to_vec();
    want_c.sort_unstable();
    let mut want_a = ACTIONS.to_vec();
    want_a.sort_unstable();
    assert_eq!(conditions, want_c);
    assert_eq!(actions, want_a);
    for id in CONDITIONS.iter().chain(ACTIONS.iter()) {
        let info = registry.info(&cid(id)).expect("info");
        assert_eq!(info.source, cgf_core::Provenance::Builtin, "{id}");
        assert!(
            !info.description.is_empty(),
            "{id}: catalog descriptions are documentation"
        );
    }
}

/// REQ-B-17: the leaves that take blackboard key names say so in their `ComponentInfo`.
#[test]
fn blackboard_params_are_declared_for_store_as_and_target_key() {
    let registry = registry();
    let declared = |id: &str| -> Vec<BlackboardParam> {
        registry
            .info(&cid(id))
            .expect("info")
            .blackboard_params
            .clone()
    };

    assert_eq!(
        declared(ENEMY),
        [BlackboardParam::new(
            "store_as",
            BbType::Entity,
            Access::Write
        )]
    );
    for id in [TARGET, MOVE_TO, FIRE] {
        assert_eq!(
            declared(id),
            [BlackboardParam::new(
                "target_key",
                BbType::Entity,
                Access::Read
            )],
            "{id}"
        );
    }
    for id in [TIMER, CHANCE, FOLLOW, WAIT] {
        assert!(declared(id).is_empty(), "{id}");
    }
}

/// REQ-T-3: every schema has `examples`, and every example builds.
#[test]
fn every_schema_has_examples_that_build() {
    let registry = registry();
    for id in CONDITIONS {
        let schema = registry
            .param_schema(ComponentKind::Condition, &cid(id))
            .expect("schema");
        let examples = schema.as_json()["examples"].as_array().expect("examples");
        assert!(!examples.is_empty(), "{id}");
        for example in examples {
            let built = registry
                .condition(&cid(id))
                .expect("factory")
                .build(example);
            assert!(built.is_ok(), "{id} {example}: {:?}", built.err());
        }
    }
    for id in ACTIONS {
        let schema = registry
            .param_schema(ComponentKind::Action, &cid(id))
            .expect("schema");
        let examples = schema.as_json()["examples"].as_array().expect("examples");
        assert!(!examples.is_empty(), "{id}");
        for example in examples {
            let built = registry.action(&cid(id)).expect("factory").build(example);
            assert!(built.is_ok(), "{id} {example}: {:?}", built.err());
        }
    }
}

/// Factory rule: bad parameters are a `BuildError` located by a JSON pointer, never a panic.
#[test]
fn each_factory_rejects_bad_params_with_a_json_pointer() {
    let registry = registry();
    let cases: [(&str, serde_json::Value, &str); 13] = [
        (ENEMY, json!({}), "/range_m"),
        (ENEMY, json!({ "range_m": -1.0 }), "/range_m"),
        (ENEMY, json!({ "range_m": "far" }), "/range_m"),
        (
            ENEMY,
            json!({ "range_m": 10.0, "store_as": "Not A Key" }),
            "/store_as",
        ),
        (TARGET, json!({ "range_m": 10.0 }), "/target_key"),
        (
            TARGET,
            json!({ "target_key": "t", "range_m": 0.0 }),
            "/range_m",
        ),
        (TIMER, json!({ "duration_s": 0.0 }), "/duration_s"),
        (CHANCE, json!({ "probability": 1.5 }), "/probability"),
        (CHANCE, json!({ "probability": -0.1 }), "/probability"),
        (
            MOVE_TO,
            json!({ "target_key": "t", "speed_mps": 0.0, "arrival_radius_m": 1.0 }),
            "/speed_mps",
        ),
        (
            MOVE_TO,
            json!({ "target_key": "t", "speed_mps": 5.0, "arrival_radius_m": -1.0 }),
            "/arrival_radius_m",
        ),
        (WAIT, json!({ "duration_s": -1.0 }), "/duration_s"),
        (
            FIRE,
            json!({ "target_key": "t", "weapon": "Not Valid" }),
            "/weapon",
        ),
    ];
    for (id, params, pointer) in cases {
        let error: Option<BuildError> = if CONDITIONS.contains(&id) {
            registry
                .condition(&cid(id))
                .expect("factory")
                .build(&params)
                .err()
        } else {
            registry
                .action(&cid(id))
                .expect("factory")
                .build(&params)
                .err()
        };
        assert_eq!(
            error.and_then(|e| e.param).as_deref(),
            Some(pointer),
            "{id} {params}"
        );
    }
}

// --- conditions --------------------------------------------------------------------------------

/// ASM-077: `enemy_in_range` succeeds for a hostile contact in range and stores the nearest one;
/// friendly, neutral and out-of-range contacts do not count.
#[test]
fn enemy_in_range_stores_the_nearest_hostile_contact_in_range() {
    let cond = condition(ENEMY, &json!({ "range_m": 1000.0, "store_as": "target" }));
    let contacts = [
        contact(2, 800.0, Some("red")),
        contact(3, 500.0, Some("red")),
        contact(5, 100.0, Some("blue")),
        contact(6, 50.0, None),
        contact(7, 3000.0, Some("red")),
    ];
    let mut bb = bb_with_target();

    let seen = check(cond.as_ref(), 0, Some(&force("blue")), &contacts, &mut bb);

    assert!(seen);
    assert_eq!(bb.get("target"), Some(&BbValue::Entity(eid(3))));
}

/// ASM-077: equal ranges are broken by the smaller entity id.
#[test]
fn enemy_in_range_breaks_range_ties_by_entity_id() {
    let cond = condition(ENEMY, &json!({ "range_m": 1000.0, "store_as": "target" }));
    let contacts = [
        contact(4, 500.0, Some("red")),
        contact(9, 500.0, Some("red")),
    ];
    let mut bb = bb_with_target();

    assert!(check(
        cond.as_ref(),
        0,
        Some(&force("blue")),
        &contacts,
        &mut bb
    ));

    assert_eq!(bb.get("target"), Some(&BbValue::Entity(eid(4))));
}

/// ASM-077, ASM-079: no contacts, only out-of-range, friendly or neutral contacts, an observer
/// without a force, and a contact without a force are all Failure and store nothing.
#[test]
fn enemy_in_range_fails_without_a_hostile_contact_in_range() {
    let cond = condition(ENEMY, &json!({ "range_m": 1000.0, "store_as": "target" }));
    let blue = force("blue");
    let cases: [(&str, Option<&Force>, Vec<Contact>); 5] = [
        ("no contacts", Some(&blue), vec![]),
        (
            "out of range",
            Some(&blue),
            vec![contact(2, 5000.0, Some("red"))],
        ),
        (
            "friendly",
            Some(&blue),
            vec![contact(2, 100.0, Some("blue"))],
        ),
        (
            "neutral contact",
            Some(&blue),
            vec![contact(2, 100.0, None)],
        ),
        (
            "neutral observer",
            None,
            vec![contact(2, 100.0, Some("red"))],
        ),
    ];
    for (name, side, contacts) in cases {
        let mut bb = bb_with_target();

        let seen = check(cond.as_ref(), 0, side, &contacts, &mut bb);

        assert!(!seen, "{name}");
        assert_eq!(bb.get("target"), None, "{name}");
    }
}

/// ASM-077: `store_as` is optional.
#[test]
fn enemy_in_range_without_store_as_just_answers() {
    let cond = condition(ENEMY, &json!({ "range_m": 1000.0 }));
    let contacts = [contact(2, 100.0, Some("red"))];
    let mut bb = Blackboard::default();

    assert!(check(
        cond.as_ref(),
        0,
        Some(&force("blue")),
        &contacts,
        &mut bb
    ));
}

/// ASM-077: `target_in_range` is true iff the entity in the blackboard key is a current contact
/// within `range_m`.
#[test]
fn target_in_range_result_paths() {
    let cond = condition(
        TARGET,
        &json!({ "target_key": "target", "range_m": 1000.0 }),
    );
    let near = [contact(3, 500.0, Some("red"))];
    let far = [contact(3, 1500.0, Some("red"))];
    let other = [contact(4, 100.0, Some("red"))];
    let set = |bb: &mut Blackboard| bb.set("target", BbValue::Entity(eid(3))).expect("declared");

    let mut bb = bb_with_target();
    set(&mut bb);
    let in_range = check(cond.as_ref(), 0, None, &near, &mut bb);
    let out_of_range = check(cond.as_ref(), 0, None, &far, &mut bb);
    let not_a_contact = check(cond.as_ref(), 0, None, &other, &mut bb);
    let no_contacts = check(cond.as_ref(), 0, None, &[], &mut bb);
    let mut unset = bb_with_target();
    let key_unset = check(cond.as_ref(), 0, None, &near, &mut unset);

    assert!(in_range);
    assert!(!out_of_range);
    assert!(!not_a_contact);
    assert!(!no_contacts);
    assert!(!key_unset);
}

/// ASM-077, ASM-078: `timer_elapsed` is true once the simulation time is at least `duration_s`
/// (whole ticks: 0.1 s is 3 ticks at 30 Hz).
#[test]
fn timer_elapsed_compares_simulation_time_in_whole_ticks() {
    let cond = condition(TIMER, &json!({ "duration_s": 0.1 }));
    let mut bb = Blackboard::default();

    let results: Vec<bool> = (0..6)
        .map(|t| check(cond.as_ref(), t, None, &[], &mut bb))
        .collect();

    assert_eq!(results, [false, false, false, true, true, true]);
}

/// REQ-B-16, ASM-077: `chance` draws exactly once from the entity stream per evaluation, and a
/// probability of 0 or 1 is exact.
#[test]
fn chance_draws_exactly_once_per_evaluation_and_zero_and_one_are_exact() {
    let mut expected = stream();
    let _ = expected.next_u64();
    for (probability, want) in [(0.0, false), (1.0, true)] {
        let cond = condition(CHANCE, &json!({ "probability": probability }));
        let mut rng = stream();
        let mut bb = Blackboard::default();

        let got = check_with(cond.as_ref(), 0, None, &[], &mut bb, &mut rng);

        assert_eq!(got, want, "probability {probability}");
        assert_eq!(
            rng, expected,
            "exactly one draw at probability {probability}"
        );
    }
}

/// REQ-K-4: the same stream gives the same answers (the seed matters, nothing else does).
#[test]
fn chance_answers_follow_the_stream() {
    let cond = condition(CHANCE, &json!({ "probability": 0.5 }));
    let run = |seed: u64| -> Vec<bool> {
        let mut rng = SimRng::new(seed).stream(StreamKey::entity(eid(1)));
        let mut bb = Blackboard::default();
        (0..64)
            .map(|t| check_with(cond.as_ref(), t, None, &[], &mut bb, &mut rng))
            .collect()
    };

    assert_eq!(run(7), run(7));
    assert_ne!(run(7), run(8));
}

// --- actions -----------------------------------------------------------------------------------

/// ASM-077: `follow_route` emits `FollowRoute` and is Running until the initial route is
/// complete, then Success; without a route it fails and emits nothing.
#[test]
fn follow_route_result_paths() {
    let mut act = action(FOLLOW, &json!({}));
    let waypoints = [Ecef(Vec3::new(6_378_137.0, 100.0, 0.0))];
    let mut bb = Blackboard::default();

    let none = tick(act.as_mut(), 0, &[], None, &mut bb);
    let going = tick(
        act.as_mut(),
        1,
        &[],
        Some(RouteView::new(&waypoints, 0, false)),
        &mut bb,
    );
    let done = tick(
        act.as_mut(),
        2,
        &[],
        Some(RouteView::new(&waypoints, 0, true)),
        &mut bb,
    );

    assert_eq!(none, (Status::Failure, vec![]));
    assert_eq!(going, (Status::Running, vec![Intent::FollowRoute]));
    assert_eq!(done.0, Status::Success);
}

/// ASM-077: `move_to` fails if the target is not a current contact (or the key is unset), is
/// Running with `SetSpeed` and `MoveTo` toward the contact, and Success within the radius.
#[test]
fn move_to_result_paths() {
    let params = json!({ "target_key": "target", "speed_mps": 8.0, "arrival_radius_m": 50.0 });
    let mut act = action(MOVE_TO, &params);
    let far = contact(3, 500.0, Some("red"));
    let close = contact(3, 30.0, Some("red"));
    let mut bb = bb_with_target();

    let unset = tick(act.as_mut(), 0, std::slice::from_ref(&far), None, &mut bb);
    bb.set("target", BbValue::Entity(eid(3))).expect("declared");
    let lost = tick(act.as_mut(), 1, &[], None, &mut bb);
    let moving = tick(act.as_mut(), 2, std::slice::from_ref(&far), None, &mut bb);
    let arrived = tick(act.as_mut(), 3, std::slice::from_ref(&close), None, &mut bb);

    assert_eq!(unset, (Status::Failure, vec![]));
    assert_eq!(lost, (Status::Failure, vec![]));
    assert_eq!(moving.0, Status::Running);
    assert!(
        moving.1.contains(&Intent::MoveTo {
            destination: far.position
        }),
        "{:?}",
        moving.1
    );
    assert!(
        moving.1.contains(&Intent::SetSpeed(MetersPerSecond(8.0))),
        "{:?}",
        moving.1
    );
    assert_eq!(arrived.0, Status::Success);
}

/// ASM-077, ASM-078: `wait` emits `Hold` and is Running for the ticks of `duration_s` counted from
/// its own first tick, then Success; a halt restarts the count.
#[test]
fn wait_counts_whole_ticks_from_its_first_tick_and_restarts_after_halt() {
    let mut act = action(WAIT, &json!({ "duration_s": 0.1 }));
    let mut bb = Blackboard::default();

    let first: Vec<(Status, Vec<Intent>)> = (10..14)
        .map(|t| tick(act.as_mut(), t, &[], None, &mut bb))
        .collect();
    act.halt();
    let again = tick(act.as_mut(), 20, &[], None, &mut bb);

    let running = (Status::Running, vec![Intent::Hold]);
    assert_eq!(first[0], running);
    assert_eq!(first[1], running);
    assert_eq!(first[2], running);
    assert_eq!(first[3].0, Status::Success);
    assert_eq!(again, running, "a halted wait starts over");
}

/// ASM-078 / ADR 0013 amendment: a `wait` whose duration does not fit in ticks fails `instantiate`
/// with `InstantiateError::Duration`.
#[test]
fn wait_duration_that_does_not_fit_fails_instantiate() {
    let factory = registry()
        .action(&cid(WAIT))
        .expect("registered")
        .build(&json!({ "duration_s": 1e30 }))
        .expect("the factory accepts a finite positive duration");

    let error = factory.instantiate(rate()).err();

    assert!(
        matches!(error, Some(InstantiateError::Duration { hz: 30, .. })),
        "{error:?}"
    );
}

/// ASM-077: `fire` emits `Fire { target, weapon }` and succeeds; it fails, without an intent, if
/// the target is not a current contact or the key is unset.
#[test]
fn fire_result_paths() {
    let params = json!({ "target_key": "target", "weapon": "example.weapon.main_gun" });
    let mut act = action(FIRE, &params);
    let target = contact(3, 500.0, Some("red"));
    let mut bb = bb_with_target();

    let unset = tick(
        act.as_mut(),
        0,
        std::slice::from_ref(&target),
        None,
        &mut bb,
    );
    bb.set("target", BbValue::Entity(eid(3))).expect("declared");
    let lost = tick(act.as_mut(), 1, &[], None, &mut bb);
    let fired = tick(
        act.as_mut(),
        2,
        std::slice::from_ref(&target),
        None,
        &mut bb,
    );

    assert_eq!(unset, (Status::Failure, vec![]));
    assert_eq!(lost, (Status::Failure, vec![]));
    assert_eq!(
        fired,
        (
            Status::Success,
            vec![Intent::Fire {
                target: eid(3),
                weapon: WeaponId::new("example.weapon.main_gun").expect("valid weapon id"),
            }]
        )
    );
}

// --- added in WI-11: remaining AT-11 rows ------------------------------------------------------

/// A `store_as` key that is not declared (or has another type) cannot be written, so the
/// condition fails instead of claiming a target it could not store (ASM-100 a).
#[test]
fn enemy_in_range_fails_when_store_as_cannot_be_written() {
    let cond = condition(ENEMY, &json!({ "range_m": 1000.0, "store_as": "target" }));
    let contacts = [contact(2, 100.0, Some("red"))];
    let mut undeclared = Blackboard::default();
    let mut wrong_type = Blackboard::default();
    wrong_type.declare("target", BbType::Bool);

    let a = check(
        cond.as_ref(),
        0,
        Some(&force("blue")),
        &contacts,
        &mut undeclared,
    );
    let b = check(
        cond.as_ref(),
        0,
        Some(&force("blue")),
        &contacts,
        &mut wrong_type,
    );

    assert!(!a && !b);
    assert_eq!(wrong_type.get("target"), None);
}

/// ASM-077: the range test is inclusive and uses `Contact::range`.
#[test]
fn range_tests_are_inclusive() {
    let enemy = condition(ENEMY, &json!({ "range_m": 500.0 }));
    let target = condition(TARGET, &json!({ "target_key": "target", "range_m": 500.0 }));
    let contacts = [contact(3, 500.0, Some("red"))];
    let mut bb = bb_with_target();
    bb.set("target", BbValue::Entity(eid(3))).expect("declared");

    assert!(check(
        enemy.as_ref(),
        0,
        Some(&force("blue")),
        &contacts,
        &mut bb
    ));
    assert!(check(target.as_ref(), 0, None, &contacts, &mut bb));
}

/// `target_in_range` does not require hostility: it only asks about the stored contact.
#[test]
fn target_in_range_ignores_forces() {
    let cond = condition(TARGET, &json!({ "target_key": "target", "range_m": 500.0 }));
    let mut bb = bb_with_target();
    bb.set("target", BbValue::Entity(eid(3))).expect("declared");

    let friendly = check(
        cond.as_ref(),
        0,
        Some(&force("blue")),
        &[contact(3, 100.0, Some("blue"))],
        &mut bb,
    );

    assert!(friendly);
}

/// ASM-078 at another rate, and a duration beyond `u64` ticks never elapses.
#[test]
fn timer_elapsed_at_60_hz_and_with_an_unreachable_duration() {
    let st = state();
    let run = |cond: &dyn Condition, tick: u64| {
        let mut rng = stream();
        let r60 = TickRate::new(60).expect("60 Hz");
        let mut ctx = BehaviorContext::new(SimTime::at_tick(r60, tick), &mut rng);
        cond.check(
            &mut ctx,
            &EntityView::new(eid(1), &st),
            &mut Blackboard::default(),
        )
    };
    let cond = condition(TIMER, &json!({ "duration_s": 0.1 }));
    let never = condition(TIMER, &json!({ "duration_s": 1e30 }));

    let results: Vec<bool> = (4..9).map(|t| run(cond.as_ref(), t)).collect();

    assert_eq!(
        results,
        [false, false, true, true, true],
        "0.1 s is 6 ticks"
    );
    assert!(!run(never.as_ref(), u64::MAX));
}

/// REQ-B-16: probabilities strictly between 0 and 1 follow the draw (`draw < p`), one draw each.
#[test]
fn chance_succeeds_iff_the_draw_is_below_the_probability() {
    let cond = condition(CHANCE, &json!({ "probability": 0.3 }));
    let mut mirror = stream();
    let mut rng = stream();
    let mut bb = Blackboard::default();

    for tick in 0..200 {
        let got = check_with(cond.as_ref(), tick, None, &[], &mut bb, &mut rng);
        assert_eq!(got, mirror.next_f64() < 0.3, "tick {tick}");
    }
    assert_eq!(rng, mirror);
}

/// Probability 0 and 1 are exact over many draws.
#[test]
fn chance_zero_never_and_one_always_over_many_draws() {
    let never = condition(CHANCE, &json!({ "probability": 0.0 }));
    let always = condition(CHANCE, &json!({ "probability": 1.0 }));
    let mut rng = stream();
    let mut bb = Blackboard::default();

    for tick in 0..1000 {
        assert!(!check_with(
            never.as_ref(),
            tick,
            None,
            &[],
            &mut bb,
            &mut rng
        ));
        assert!(check_with(
            always.as_ref(),
            tick,
            None,
            &[],
            &mut bb,
            &mut rng
        ));
    }
}

/// ASM-077: an empty route counts as no route; `follow_route` takes `null` or `{}` params.
#[test]
fn follow_route_with_an_empty_route_fails_and_accepts_null_params() {
    let mut act = action(FOLLOW, &serde_json::Value::Null);
    let mut bb = Blackboard::default();

    let empty = tick(
        act.as_mut(),
        0,
        &[],
        Some(RouteView::new(&[], 0, false)),
        &mut bb,
    );

    assert_eq!(empty, (Status::Failure, vec![]));
}

/// A finished route still emits `FollowRoute` (the kernel expands it to `Hold`, ASM-100 b).
#[test]
fn follow_route_emits_the_intent_on_the_completion_tick() {
    let mut act = action(FOLLOW, &json!({}));
    let waypoints = [Ecef(Vec3::new(6_378_137.0, 100.0, 0.0))];
    let mut bb = Blackboard::default();

    let done = tick(
        act.as_mut(),
        0,
        &[],
        Some(RouteView::new(&waypoints, 0, true)),
        &mut bb,
    );

    assert_eq!(done, (Status::Success, vec![Intent::FollowRoute]));
}

/// `move_to` holds no state: a
/// target lost then found again is simply approached again.
#[test]
fn move_to_is_stateless_between_ticks() {
    let params = json!({ "target_key": "target", "speed_mps": 8.0, "arrival_radius_m": 0.5 });
    let mut act = action(MOVE_TO, &params);
    let mut bb = bb_with_target();
    bb.set("target", BbValue::Entity(eid(3))).expect("declared");
    let here = contact(3, 0.25, Some("red"));
    let near = contact(3, 1.0, Some("red"));

    let a = tick(act.as_mut(), 0, std::slice::from_ref(&near), None, &mut bb);
    let lost = tick(act.as_mut(), 1, &[], None, &mut bb);
    act.halt();
    act.halt();
    let b = tick(act.as_mut(), 2, std::slice::from_ref(&near), None, &mut bb);
    let arrived = tick(act.as_mut(), 3, std::slice::from_ref(&here), None, &mut bb);

    assert_eq!(a.0, Status::Running);
    assert_eq!(lost.0, Status::Failure);
    assert_eq!(a, b);
    assert_eq!(arrived, (Status::Success, vec![]));
}

/// A `wait` is Running exactly while it holds, restarts after it finished, and `halt` is
/// idempotent and harmless when it is not Running (ASM-096 c).
#[test]
fn wait_restarts_after_success_and_halt_is_idempotent() {
    let mut act = action(WAIT, &json!({ "duration_s": 0.0333 }));
    let mut bb = Blackboard::default();
    act.halt();
    act.halt();

    let statuses: Vec<Status> = (0..6)
        .map(|t| tick(act.as_mut(), t, &[], None, &mut bb).0)
        .collect();

    assert_eq!(
        statuses,
        [
            Status::Running,
            Status::Success,
            Status::Running,
            Status::Success,
            Status::Running,
            Status::Success
        ],
        "0.0333 s is one tick at 30 Hz"
    );
}

/// At 60 Hz the same duration is more ticks (ASM-078).
#[test]
fn wait_converts_duration_at_the_instance_rate() {
    let factory = registry()
        .action(&cid(WAIT))
        .expect("registered")
        .build(&json!({ "duration_s": 0.1 }))
        .expect("builds");
    let mut act = factory
        .instantiate(TickRate::new(60).expect("60 Hz"))
        .expect("instantiates");
    let mut bb = Blackboard::default();

    let statuses: Vec<Status> = (0..8)
        .map(|t| tick(act.as_mut(), t, &[], None, &mut bb).0)
        .collect();

    assert_eq!(statuses[..6], [Status::Running; 6]);
    assert_eq!(statuses[6], Status::Success);
}

/// `fire` holds no state and keeps working after halts.
#[test]
fn fire_fires_every_tick_and_halt_is_a_no_op() {
    let params = json!({ "target_key": "target", "weapon": "example.weapon.main_gun" });
    let mut act = action(FIRE, &params);
    let target = contact(3, 500.0, Some("red"));
    let mut bb = bb_with_target();
    bb.set("target", BbValue::Entity(eid(3))).expect("declared");

    act.halt();
    let first = tick(
        act.as_mut(),
        0,
        std::slice::from_ref(&target),
        None,
        &mut bb,
    );
    let second = tick(
        act.as_mut(),
        1,
        std::slice::from_ref(&target),
        None,
        &mut bb,
    );

    assert_eq!(first, second);
    assert_eq!(first.0, Status::Success);
}

/// Any key that is not a parameter is rejected at its pointer, and params that are not a table
/// are rejected at the root. Wrong types and non-finite numbers are located, never a panic.
#[test]
fn unknown_keys_wrong_types_and_non_tables_are_located_errors() {
    let registry = registry();
    let cases: [(&str, serde_json::Value, &str); 9] = [
        (
            MOVE_TO,
            json!({ "target_key": "t", "speed_mps": 1.0, "arrival_radius_m": 0.0 }),
            "/arrival_radius_m",
        ),
        (FOLLOW, json!({ "speed": 1 }), "/speed"),
        (
            CHANCE,
            json!({ "probability": 0.5, "extra": true }),
            "/extra",
        ),
        (CHANCE, json!([0.5]), ""),
        (CHANCE, json!({ "probability": "half" }), "/probability"),
        (TIMER, json!({ "duration_s": null }), "/duration_s"),
        (
            TARGET,
            json!({ "target_key": 7, "range_m": 1.0 }),
            "/target_key",
        ),
        (
            ENEMY,
            json!({ "range_m": 1.0, "store_as": null }),
            "/store_as",
        ),
        (WAIT, json!({}), "/duration_s"),
    ];
    for (id, params, pointer) in cases {
        let error = if CONDITIONS.contains(&id) {
            registry
                .condition(&cid(id))
                .expect("f")
                .build(&params)
                .err()
        } else {
            registry.action(&cid(id)).expect("f").build(&params).err()
        };
        assert_eq!(
            error.and_then(|e| e.param).as_deref(),
            Some(pointer),
            "{id} {params}"
        );
    }
}

/// The weapon and key grammars: an over-long key is a located error, and a valid namespaced
/// weapon builds.
#[test]
fn key_and_weapon_grammar_are_enforced() {
    let registry = registry();
    let long = "k".repeat(300);

    let too_long = registry
        .condition(&cid(TARGET))
        .expect("f")
        .build(&json!({ "target_key": long, "range_m": 1.0 }))
        .err();
    let bad_weapon = registry
        .action(&cid(FIRE))
        .expect("f")
        .build(&json!({ "target_key": "t", "weapon": 5 }))
        .err();

    assert_eq!(
        too_long.and_then(|e| e.param).as_deref(),
        Some("/target_key")
    );
    assert_eq!(bad_weapon.and_then(|e| e.param).as_deref(), Some("/weapon"));
}

/// Every leaf's product reports the ID it was built for (Factory contract).
#[test]
fn products_carry_their_factory_id() {
    let registry = registry();
    for id in CONDITIONS {
        let params = match id {
            ENEMY => json!({ "range_m": 1.0 }),
            TARGET => json!({ "target_key": "t", "range_m": 1.0 }),
            TIMER => json!({ "duration_s": 1.0 }),
            _ => json!({ "probability": 0.5 }),
        };
        let built = registry.condition(&cid(id)).expect("f").build(&params);
        assert_eq!(built.expect("builds").id().as_str(), id);
    }
    for id in ACTIONS {
        let params = match id {
            FOLLOW => json!({}),
            MOVE_TO => json!({ "target_key": "t", "speed_mps": 1.0, "arrival_radius_m": 1.0 }),
            WAIT => json!({ "duration_s": 1.0 }),
            _ => json!({ "target_key": "t", "weapon": "a.b" }),
        };
        let built = registry.action(&cid(id)).expect("f").build(&params);
        assert_eq!(built.expect("builds").id().as_str(), id);
    }
}

/// Registering twice is a duplicate error and leaves the first registration intact.
#[test]
fn registering_builtins_twice_is_a_duplicate_error() {
    let mut registry = registry();

    let again = register_builtins(&mut registry);

    assert!(again.is_err());
    assert_eq!(registry.ids(ComponentKind::Condition).len(), 4);
    assert_eq!(registry.ids(ComponentKind::Action).len(), 4);
}

// --- review follow-up -----------------------------------------------------------------------------

fn set_target(bb: &mut Blackboard, id: u64) {
    bb.set("target", BbValue::Entity(eid(id)))
        .expect("declared");
}

/// The key names an entity that is not among the contacts, even though others are present.
#[test]
fn move_to_and_fire_fail_when_only_other_contacts_exist() {
    let others = [contact(2, 10.0, Some("red")), contact(4, 20.0, Some("red"))];
    let mut bb = bb_with_target();
    set_target(&mut bb, 3);
    let mut go = action(
        MOVE_TO,
        &json!({ "target_key": "target", "speed_mps": 8.0, "arrival_radius_m": 5.0 }),
    );
    let mut shoot = action(
        FIRE,
        &json!({ "target_key": "target", "weapon": "example.weapon.main_gun" }),
    );

    let a = tick(go.as_mut(), 0, &others, None, &mut bb);
    let b = tick(shoot.as_mut(), 0, &others, None, &mut bb);

    assert_eq!(a, (Status::Failure, vec![]));
    assert_eq!(b, (Status::Failure, vec![]));
}

/// A failed `enemy_in_range` leaves an earlier `store_as` value alone (ASM-100 a).
#[test]
fn failed_enemy_in_range_keeps_the_previous_store_as_value() {
    let cond = condition(ENEMY, &json!({ "range_m": 100.0, "store_as": "target" }));
    let mut bb = bb_with_target();
    set_target(&mut bb, 9);

    let seen = check(
        cond.as_ref(),
        0,
        Some(&force("blue")),
        &[contact(2, 500.0, Some("red"))],
        &mut bb,
    );

    assert!(!seen);
    assert_eq!(bb.get("target"), Some(&BbValue::Entity(eid(9))));
}

/// While approaching, `move_to` emits exactly `SetSpeed` then `MoveTo` (ASM-077).
#[test]
fn move_to_running_emits_exactly_set_speed_then_move_to() {
    let mut act = action(
        MOVE_TO,
        &json!({ "target_key": "target", "speed_mps": 8.0, "arrival_radius_m": 50.0 }),
    );
    let far = contact(3, 500.0, Some("red"));
    let mut bb = bb_with_target();
    set_target(&mut bb, 3);

    let moving = tick(act.as_mut(), 0, std::slice::from_ref(&far), None, &mut bb);

    assert_eq!(
        moving,
        (
            Status::Running,
            vec![
                Intent::SetSpeed(MetersPerSecond(8.0)),
                Intent::MoveTo {
                    destination: far.position
                }
            ]
        )
    );
}

/// A key that holds a value of another type is not a target: every user of it fails.
#[test]
fn a_target_key_holding_a_non_entity_value_is_not_a_target() {
    let near = [contact(3, 10.0, Some("red"))];
    let mut bb = Blackboard::default();
    bb.declare("target", BbType::Integer);
    bb.set("target", BbValue::Integer(3)).expect("declared");
    let cond = condition(TARGET, &json!({ "target_key": "target", "range_m": 100.0 }));
    let mut go = action(
        MOVE_TO,
        &json!({ "target_key": "target", "speed_mps": 8.0, "arrival_radius_m": 5.0 }),
    );
    let mut shoot = action(
        FIRE,
        &json!({ "target_key": "target", "weapon": "example.weapon.main_gun" }),
    );

    assert!(!check(cond.as_ref(), 0, None, &near, &mut bb));
    assert_eq!(
        tick(go.as_mut(), 0, &near, None, &mut bb),
        (Status::Failure, vec![])
    );
    assert_eq!(
        tick(shoot.as_mut(), 0, &near, None, &mut bb),
        (Status::Failure, vec![])
    );
}

/// REQ-B-17: the catalog description of every leaf is the description of its schema.
#[test]
fn catalog_description_equals_schema_description() {
    let registry = registry();
    for (kind, ids) in [
        (ComponentKind::Condition, CONDITIONS),
        (ComponentKind::Action, ACTIONS),
    ] {
        for id in ids {
            let schema = registry.param_schema(kind, &cid(id)).expect("schema");
            let want = schema.as_json()["description"]
                .as_str()
                .expect("description");
            let got = &registry.info(&cid(id)).expect("info").description;
            assert!(!want.is_empty(), "{id}");
            assert_eq!(got, want, "{id}");
        }
    }
}
