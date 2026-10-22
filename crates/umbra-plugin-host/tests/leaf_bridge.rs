//! AT-16 (M7 WI-2, leaf rows): the host bridges of the condition and action kinds and the blackboard
//! service (ABI 0.2, ADR 0050; REQ-P-4, REQ-P-5, REQ-P-7, REQ-B-3, REQ-B-11, REQ-B-15, ASM-616..618,
//! ASM-643) through the SDK-built fixture plugin `test.plugin.kinds`: a plugin condition reads and
//! writes exactly the keys its metadata declares, a refused service call (undeclared key, wrong type,
//! write to a read-only key) faults the capability, a condition then answers false and an action
//! Failure with the intents of that call discarded, `halt` resets an action instance, instances do not
//! share state, and the metadata maps to the catalog with `Provenance::Plugin`.
//!
//! "Unset condition result faults" is a raw-ABI row (the safe SDK always writes a result) and is
//! written with the raw fixtures in WI-9.
//!
//! Step 4 of M7 (WI-2): ignored until WI-9 (and WI-10 for the SDK side, see `motion_bridge.rs`).

#[macro_use]
mod support;

use serde_json::json;
use support::kinds::{Ctx, board, check, env, instance, tick};
use umbra_core::{
    Access, BbType, BbValue, Blackboard, BlackboardParam, ComponentId, Intent, MetersPerSecond,
    Provenance, Status, TickRate,
};

#[test]
#[ignore = "M7 WI-9"]
fn given_a_condition_that_is_always_true_when_checked_then_true() {
    let env = env();
    let condition = env.condition("test.cond.always", "{}").expect("build");

    let answer = check(
        condition.as_ref(),
        &mut Ctx::new(1),
        &mut Blackboard::default(),
        1,
    );

    assert!(answer);
    assert_eq!(env.host.faults(), Vec::new());
}

#[test]
#[ignore = "M7 WI-9"]
fn given_the_flag_condition_when_the_declared_key_is_true_then_true_else_false() {
    let env = env();
    let condition = env
        .condition("test.cond.flag", r#"{"key":"armed"}"#)
        .expect("build");
    let mut ctx = Ctx::new(1);

    let answers: Vec<bool> = [Some(true), Some(false), None]
        .into_iter()
        .map(|v| check(condition.as_ref(), &mut ctx, &mut board(&[("armed", v)]), 1))
        .collect();

    assert_eq!(answers, [true, false, false]);
    assert_eq!(env.host.faults(), Vec::new(), "an unset key is not a fault");
}

#[test]
#[ignore = "M7 WI-9"]
fn given_the_mark_condition_when_checked_then_it_writes_its_declared_key() {
    let env = env();
    let condition = env
        .condition("test.cond.mark", r#"{"key":"seen"}"#)
        .expect("build");
    let mut bb = board(&[("seen", Some(false))]);

    let answer = check(condition.as_ref(), &mut Ctx::new(1), &mut bb, 1);

    assert!(answer);
    assert_eq!(bb.get("seen"), Some(&BbValue::Bool(true)));
}

/// REQ-B-11: two configured conditions of one capability read their own keys.
#[test]
#[ignore = "M7 WI-9"]
fn given_two_flag_conditions_with_different_keys_when_checked_then_each_reads_its_own_key() {
    let env = env();
    let a = env
        .condition("test.cond.flag", r#"{"key":"a"}"#)
        .expect("build");
    let b = env
        .condition("test.cond.flag", r#"{"key":"b"}"#)
        .expect("build");
    let mut bb = board(&[("a", Some(true)), ("b", Some(false))]);
    let mut ctx = Ctx::new(1);

    let answers = (
        check(a.as_ref(), &mut ctx, &mut bb, 1),
        check(b.as_ref(), &mut ctx, &mut bb, 1),
    );

    assert_eq!(answers, (true, false));
}

fn speeds(intents: &[Intent]) -> Vec<f64> {
    intents
        .iter()
        .filter_map(|i| match i {
            Intent::SetSpeed(MetersPerSecond(v)) => Some(*v),
            _ => None,
        })
        .collect()
}

#[test]
#[ignore = "M7 WI-9"]
fn given_the_count_action_when_ticked_then_running_for_ticks_ticks_then_success() {
    let env = env();
    let mut action = instance(&env, "test.action.count", r#"{"ticks":2}"#);
    let (mut ctx, mut bb) = (Ctx::new(1), Blackboard::default());

    let rounds: Vec<(Status, Vec<f64>)> = (1..=3)
        .map(|t| {
            let (status, intents) = tick(action.as_mut(), &mut ctx, &mut bb, t);
            (status, speeds(&intents))
        })
        .collect();

    assert_eq!(
        rounds,
        [
            (Status::Running, vec![1.0]),
            (Status::Running, vec![2.0]),
            (Status::Success, vec![])
        ]
    );
}

#[test]
#[ignore = "M7 WI-9"]
fn given_a_running_count_action_when_halted_then_it_starts_over() {
    let env = env();
    let mut action = instance(&env, "test.action.count", r#"{"ticks":3}"#);
    let (mut ctx, mut bb) = (Ctx::new(1), Blackboard::default());
    let _ = tick(action.as_mut(), &mut ctx, &mut bb, 1);
    let _ = tick(action.as_mut(), &mut ctx, &mut bb, 2);

    action.halt();
    let (status, intents) = tick(action.as_mut(), &mut ctx, &mut bb, 3);

    assert_eq!((status, speeds(&intents)), (Status::Running, vec![1.0]));
}

/// REQ-B-11: instances of one configured action do not share state.
#[test]
#[ignore = "M7 WI-9"]
fn given_two_instances_of_the_count_action_when_ticked_then_they_progress_independently() {
    let env = env();
    let action = env
        .action("test.action.count", r#"{"ticks":5}"#)
        .expect("build");
    let rate = TickRate::new(30).expect("30 Hz");
    let (mut first, mut second) = (
        action.instantiate(rate).expect("instance"),
        action.instantiate(rate).expect("instance"),
    );
    let (mut ctx, mut bb) = (Ctx::new(1), Blackboard::default());

    let _ = tick(first.as_mut(), &mut ctx, &mut bb, 1);
    let _ = tick(first.as_mut(), &mut ctx, &mut bb, 2);
    let (_, fresh) = tick(second.as_mut(), &mut ctx, &mut bb, 2);

    assert_eq!(
        speeds(&fresh),
        [1.0],
        "the second instance starts at its first tick"
    );
}

/// ADR 0050 (H2): events leave plugins as `CUSTOM` intents.
#[test]
#[ignore = "M7 WI-9"]
fn given_the_emit_action_when_ticked_then_it_emits_one_custom_intent_and_succeeds() {
    let env = env();
    let mut action = instance(&env, "test.action.emit", "{}");

    let (status, intents) = tick(
        action.as_mut(),
        &mut Ctx::new(1),
        &mut Blackboard::default(),
        1,
    );

    assert_eq!(status, Status::Success);
    assert_eq!(
        intents,
        [Intent::Custom {
            kind: ComponentId::new("test.event.ping").expect("id"),
            payload: json!({}),
        }]
    );
}

#[test]
#[ignore = "M7 WI-9"]
fn given_the_mark_action_when_ticked_then_it_writes_its_declared_key() {
    let env = env();
    let mut action = instance(&env, "test.action.mark", r#"{"key":"done"}"#);
    let mut bb = board(&[("done", Some(false))]);

    let (status, _) = tick(action.as_mut(), &mut Ctx::new(1), &mut bb, 1);

    assert_eq!(status, Status::Success);
    assert_eq!(bb.get("done"), Some(&BbValue::Bool(true)));
}

/// ASM-643: the metadata maps to `ComponentInfo` with `Provenance::Plugin`.
#[test]
#[ignore = "M7 WI-9"]
fn given_the_flag_condition_when_looked_up_then_the_catalog_lists_its_blackboard_param() {
    let env = env();
    let id = ComponentId::new("test.cond.flag").expect("id");

    let info = env.registry.info(&id).expect("registered");

    assert_eq!(info.source, Provenance::Plugin);
    assert_eq!(
        info.blackboard_params,
        [BlackboardParam::new("key", BbType::Bool, Access::Read)]
    );
}

#[test]
#[ignore = "M7 WI-9"]
fn given_the_mark_action_when_looked_up_then_the_catalog_lists_a_write_param() {
    let env = env();
    let id = ComponentId::new("test.action.mark").expect("id");

    let info = env.registry.info(&id).expect("registered");

    assert_eq!(info.source, Provenance::Plugin);
    assert_eq!(
        info.blackboard_params,
        [BlackboardParam::new("key", BbType::Bool, Access::Write)]
    );
}
