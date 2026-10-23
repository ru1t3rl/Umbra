//! AT-16 (M7 WI-2, leaf rows): the host bridges of the condition and action kinds and the blackboard
//! service (ABI 0.2, ADR 0050; REQ-P-4, REQ-P-5, REQ-P-7, REQ-B-3, REQ-B-11, REQ-B-15, ASM-616..618,
//! ASM-643) through the SDK-built fixture plugin `test.plugin.kinds`: a plugin condition reads and
//! writes exactly the keys its metadata declares, a refused service call (undeclared key, wrong type,
//! write to a read-only key) faults the capability, a condition then answers false and an action
//! Failure with the intents of that call discarded, `halt` resets an action instance, instances do not
//! share state, and the metadata maps to the catalog with `Provenance::Plugin`.
//!
//! Raw-ABI misbehavior the safe SDK cannot produce (a condition result or action status left unset or
//! out of range, a blackboard token kept from a finished call, of another capability's call or of
//! another service) and the service's behavior at the ABI (what `get` reports, write undo, access, the
//! entity's RNG stream) are in the module `raw_rows` (WI-9, raw fixture in `support/kinds.rs`); the
//! fault rows of the SDK fixture are in `plugin_failures.rs` (gate 13) and the engine-level fault
//! policies in `umbra-engine/tests/plugin_kinds_faults.rs`.
//!
//! Step 4 of M7 (WI-2), un-ignored with the bridges in WI-9.

#![expect(
    unsafe_code,
    reason = "the raw-ABI rows (WI-9) publish a forged plugin with `forged!`"
)]

#[macro_use]
mod support;

use serde_json::json;
use support::kinds::{Ctx, board, check, env, instance, tick};
use umbra_core::{
    Access, BbType, BbValue, Blackboard, BlackboardParam, ComponentId, Intent, MetersPerSecond,
    Provenance, Status, TickRate,
};

#[test]
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

/// WI-9: the bridged condition is `Send + Sync` like every core condition (a graph shares
/// `Arc<dyn Condition>`): checked from two threads at once (the calls serialise on the image's call
/// lock, each with its own call record and blackboard token), equal to a check on this thread, and
/// dropped last on the other thread, where `destroy` runs without an error.
#[test]
fn given_a_plugin_condition_shared_with_another_thread_then_it_checks_there_and_drops_last_there() {
    let env = env();
    let condition = env
        .condition("test.cond.flag", r#"{"key":"armed"}"#)
        .expect("build");
    let other = std::sync::Arc::clone(&condition);
    let (release, released) = std::sync::mpsc::channel::<()>();
    let worker = std::thread::spawn(move || {
        let mut ctx = Ctx::new(2);
        let got: Vec<bool> = (0..50)
            .map(|_| {
                check(
                    other.as_ref(),
                    &mut ctx,
                    &mut board(&[("armed", Some(true))]),
                    1,
                )
            })
            .collect();
        released
            .recv()
            .expect("the main thread dropped its condition");
        drop(other);
        got
    });

    let mut ctx = Ctx::new(1);
    let here: Vec<bool> = (0..50)
        .map(|_| {
            check(
                condition.as_ref(),
                &mut ctx,
                &mut board(&[("armed", Some(true))]),
                1,
            )
        })
        .collect();
    drop(condition);
    release.send(()).expect("the worker waits");
    let there = worker.join().expect("the worker did not panic");

    assert_eq!(here.len() + there.len(), 100);
    assert!(here.iter().chain(&there).all(|answer| *answer));
    assert_eq!(env.host.faults(), Vec::new());
    assert_no_error_logs(&env);
}

/// WI-9: a configured action is `Send + Sync` and its instances `Send`: an instance made on another
/// thread ticks there, and both the instance and the last handle of the action drop there, where
/// `destroy_instance` and `destroy_action` run without an error.
#[test]
fn given_a_plugin_action_shared_with_another_thread_then_its_instances_tick_there_and_drop_last_there()
 {
    let env = env();
    let action = env
        .action("test.action.count", r#"{"ticks":2}"#)
        .expect("build");
    let rate = TickRate::new(30).expect("30 Hz");
    let mut here = action.instantiate(rate).expect("instance");
    let other = std::sync::Arc::clone(&action);
    let (release, released) = std::sync::mpsc::channel::<()>();
    let worker = std::thread::spawn(move || {
        let mut instance = other.instantiate(rate).expect("instance");
        let (mut ctx, mut bb) = (Ctx::new(2), Blackboard::default());
        let got = tick(instance.as_mut(), &mut ctx, &mut bb, 1);
        released.recv().expect("the main thread dropped its action");
        drop(instance);
        drop(other);
        got
    });

    let (mut ctx, mut bb) = (Ctx::new(1), Blackboard::default());
    let (status, intents) = tick(here.as_mut(), &mut ctx, &mut bb, 1);
    drop(here);
    drop(action);
    release.send(()).expect("the worker waits");
    let (there, there_intents) = worker.join().expect("the worker did not panic");

    assert_eq!((status, speeds(&intents)), (Status::Running, vec![1.0]));
    assert_eq!(
        (there, speeds(&there_intents)),
        (Status::Running, vec![1.0])
    );
    assert_eq!(env.host.faults(), Vec::new());
    assert_no_error_logs(&env);
}

fn assert_no_error_logs(env: &support::kinds::Env) {
    let errors: Vec<_> = env
        .log
        .all()
        .into_iter()
        .filter(|(_, level, _)| *level == umbra_plugin_host::LogLevel::Error)
        .collect();
    assert_eq!(errors, Vec::new(), "destroy ran on the worker");
}

// ---------------------------------------------------------------- raw-ABI leaf rows (WI-9)
//
// ADR 0050, ADR 0019 RR-M2, ASM-618, ASM-723: a blackboard token is valid only in the call that
// carries it; the host judges the result byte and the status the plugin wrote. The raw fixture
// returns `UMBRA_OK` whatever the host answered (except where a row says otherwise), so a fault below
// comes from the host's own verdict or call record, not from the plugin's code.

mod raw_rows {
    use super::support::kinds::{
        Counting, Ctx, NO_REPORT, RAW_ACTION, RAW_CONDITION, REENTER, RawEnv, accesses, board,
        check, destroyed_with, leaf_board, raw_leaf_parts, state_at, step, stream, tick,
    };
    use std::cell::Cell;
    use umbra_core::{
        BbType, BbValue, Blackboard, Intent, MetersPerSecond, PluginFaultPolicy, Status,
    };
    use umbra_plugin_api::{UMBRA_ERR_INVALID_ARGUMENT, UMBRA_ERR_STATE, UMBRA_OK};
    use umbra_plugin_host::HostMode;

    /// The one fault: of `capability` in `call`, its message containing every needle.
    fn assert_fault(env: &RawEnv, capability: &str, call: &str, needles: &[&str]) {
        let faults = env.faults();
        assert_eq!(faults.len(), 1, "{faults:?}");
        assert_eq!((faults[0].0.as_str(), faults[0].1), (capability, call));
        for needle in needles {
            assert!(
                faults[0].2.contains(needle),
                "{needle:?} in {}",
                faults[0].2
            );
        }
    }

    fn check_on(env: &RawEnv, mode: &str, bb: &mut Blackboard) -> bool {
        check(env.condition(mode).as_ref(), &mut Ctx::new(1), bb, 1)
    }

    /// ADR 0050: `0xFF` (`UMBRA_CONDITION_UNSET`) left in place faults the condition; it answers false.
    #[test]
    fn an_unset_condition_result_faults_the_condition_and_answers_false() {
        let env = RawEnv::load(forged!(raw_leaf_parts()), PluginFaultPolicy::Abort);

        let answer = check_on(&env, "unset", &mut leaf_board(true, false));

        assert!(!answer);
        assert_fault(&env, RAW_CONDITION, "check", &["unset"]);
        assert!(env.host.take_abort().is_some(), "abort pending");
    }

    /// ADR 0050: a result byte other than 0 or 1 faults the condition.
    #[test]
    fn a_condition_result_other_than_0_or_1_faults_the_condition_and_answers_false() {
        let env = RawEnv::load(forged!(raw_leaf_parts()), PluginFaultPolicy::Abort);

        let answer = check_on(&env, "two", &mut leaf_board(true, false));

        assert!(!answer);
        assert_fault(&env, RAW_CONDITION, "check", &["2", "not 0 or 1"]);
    }

    /// The control row of the two above and of the access rows: `get` reports the granted key's value
    /// through the plugin's own sink, exactly once, and a clean call answers what the plugin wrote.
    #[test]
    fn a_granted_read_reports_the_keys_value_through_the_plugins_sink() {
        let env = RawEnv::load(forged!(raw_leaf_parts()), PluginFaultPolicy::Abort);

        let answers = [
            check_on(&env, "flag", &mut leaf_board(true, false)),
            check_on(&env, "flag", &mut leaf_board(false, false)),
            check_on(&env, "flag", &mut board(&[("armed", None)])),
        ];

        assert_eq!(answers, [true, false, false]);
        assert_eq!(
            accesses(),
            [
                (UMBRA_OK, "bool:true".to_owned()),
                (UMBRA_OK, "bool:false".to_owned()),
                (UMBRA_OK, "unset".to_owned())
            ]
        );
        assert_eq!(env.faults(), Vec::new());
    }

    /// ASM-723: a granted key holding a value of another type than the grant (which graph validation
    /// rules out) reads as unset, not as a fault.
    #[test]
    fn a_granted_key_holding_another_type_reads_as_unset() {
        let env = RawEnv::load(forged!(raw_leaf_parts()), PluginFaultPolicy::Abort);
        let mut bb = Blackboard::default();
        bb.declare("armed", BbType::Number);
        bb.set("armed", BbValue::Number(1.0)).expect("declared");

        let answer = check_on(&env, "flag", &mut bb);

        assert!(!answer);
        assert_eq!(accesses(), [(UMBRA_OK, "unset".to_owned())]);
        assert_eq!(env.faults(), Vec::new());
    }

    /// ASM-723: access is as declared: `get` of a key granted only for writing is refused.
    #[test]
    fn a_read_of_a_key_declared_write_only_is_refused_and_faults_the_condition() {
        let env = RawEnv::load(forged!(raw_leaf_parts()), PluginFaultPolicy::Abort);

        let answer = check_on(&env, "read_out", &mut leaf_board(true, true));

        assert!(!answer);
        assert_eq!(
            accesses(),
            [(UMBRA_ERR_INVALID_ARGUMENT, NO_REPORT.to_owned())]
        );
        assert_fault(
            &env,
            RAW_CONDITION,
            "check",
            &["done", "access write, not read"],
        );
    }

    /// A null value sink is refused before anything is read.
    #[test]
    fn a_null_value_sink_is_refused_and_faults_the_condition() {
        let env = RawEnv::load(forged!(raw_leaf_parts()), PluginFaultPolicy::Abort);

        let answer = check_on(&env, "null_sink", &mut leaf_board(true, false));

        assert!(!answer);
        assert_eq!(
            accesses(),
            [(UMBRA_ERR_INVALID_ARGUMENT, NO_REPORT.to_owned())]
        );
        assert_fault(&env, RAW_CONDITION, "check", &["value sink", "null"]);
    }

    fn assert_stale_token_fault(env: &RawEnv, capability: &str, call: &str) {
        assert_fault(env, capability, call, &["blackboard", "stale or foreign"]);
    }

    #[test]
    fn given_a_blackboard_token_kept_from_a_finished_check_when_used_in_a_later_check_then_refused_and_the_condition_faults()
     {
        let env = RawEnv::load(forged!(raw_leaf_parts()), PluginFaultPolicy::Abort);
        let keep = env.condition("keep");
        let stale = env.condition("stale");
        let mut bb = leaf_board(true, false);

        let kept = check(keep.as_ref(), &mut Ctx::new(1), &mut bb, 1);
        let refused = check(stale.as_ref(), &mut Ctx::new(1), &mut bb, 2);

        assert!(kept, "the keeping check is clean");
        assert!(!refused);
        assert_eq!(
            accesses(),
            [
                (UMBRA_OK, "bool:true".to_owned()),
                (UMBRA_ERR_STATE, NO_REPORT.to_owned())
            ],
            "the kept token answers in its own call only"
        );
        assert_stale_token_fault(&env, RAW_CONDITION, "check");
    }

    #[test]
    fn given_a_blackboard_token_kept_from_a_finished_check_when_used_outside_any_call_then_err_state()
     {
        let env = RawEnv::load(forged!(raw_leaf_parts()), PluginFaultPolicy::Abort);
        let _ = check_on(&env, "keep", &mut leaf_board(true, false));
        let (token, get, _) = super::support::kinds::KEPT_BB
            .with(Cell::get)
            .expect("the check kept its blackboard service");

        let after = super::support::kinds::bb_get(get, token, "armed", false);

        assert_eq!(after, (UMBRA_ERR_STATE, NO_REPORT.to_owned()));
        assert_eq!(
            env.faults(),
            Vec::new(),
            "no call to blame: the check itself was clean"
        );
    }

    /// Another capability's call: an action's blackboard token used in a check is refused, and only
    /// the condition is faulted.
    #[test]
    fn given_an_actions_blackboard_token_when_used_in_a_check_then_refused_and_only_the_condition_faults()
     {
        let env = RawEnv::load(
            forged!(raw_leaf_parts()),
            PluginFaultPolicy::DisableCapability,
        );
        let mut keeper = env.action("keep");
        let mut bb = leaf_board(true, false);

        let (status, _) = tick(keeper.as_mut(), &mut Ctx::new(1), &mut bb, 1);
        let refused = check_on(&env, "stale", &mut bb);

        assert_eq!(status, Status::Success);
        assert!(!refused);
        assert_eq!(accesses()[0], (UMBRA_OK, "bool:true".to_owned()));
        assert_eq!(accesses()[2], (UMBRA_ERR_STATE, NO_REPORT.to_owned()));
        assert_stale_token_fault(&env, RAW_CONDITION, "check");
    }

    /// Another service of the same call: the call's error sink token is not a blackboard token.
    #[test]
    fn given_the_error_sink_token_used_as_a_blackboard_token_then_refused_and_the_condition_faults()
    {
        let env = RawEnv::load(forged!(raw_leaf_parts()), PluginFaultPolicy::Abort);

        let refused = check_on(&env, "err_token", &mut leaf_board(true, false));

        assert!(!refused);
        assert_eq!(accesses(), [(UMBRA_ERR_STATE, NO_REPORT.to_owned())]);
        assert_stale_token_fault(&env, RAW_CONDITION, "check");
    }

    /// Another service of another capability's call: a motion step's terrain token used as a
    /// blackboard token.
    #[test]
    fn given_a_motion_steps_terrain_token_used_as_a_blackboard_token_then_refused_and_the_condition_faults()
     {
        let env = RawEnv::load(forged!(raw_leaf_parts()), PluginFaultPolicy::Abort);
        let keeper = env.motion("keep");
        let terrain = Counting::default();
        let _ = step(keeper.as_ref(), &state_at(4.0, 50.0, 0.0), &[], &terrain);

        let refused = check_on(&env, "terrain_token", &mut leaf_board(true, false));

        assert!(!refused);
        assert_eq!(accesses(), [(UMBRA_ERR_STATE, NO_REPORT.to_owned())]);
        assert_stale_token_fault(&env, RAW_CONDITION, "check");
    }

    /// A blackboard token kept from a check, used to write in a later action tick: refused, the key
    /// unchanged, the tick Failure with its intents discarded, only the action faulted.
    #[test]
    fn given_a_checks_blackboard_token_when_used_to_write_in_a_tick_then_refused_and_the_action_faults()
     {
        let env = RawEnv::load(
            forged!(raw_leaf_parts()),
            PluginFaultPolicy::DisableCapability,
        );
        let mut bb = leaf_board(true, false);
        let _ = check_on(&env, "keep", &mut bb);
        let mut stale = env.action("stale_set");

        let (status, intents) = tick(stale.as_mut(), &mut Ctx::new(1), &mut bb, 2);

        assert_eq!((status, intents), (Status::Failure, Vec::new()));
        assert_eq!(bb.get("done"), Some(&BbValue::Bool(false)));
        assert_eq!(accesses()[1], (UMBRA_ERR_STATE, NO_REPORT.to_owned()));
        assert_stale_token_fault(&env, RAW_ACTION, "tick");
    }

    /// ADR 0050: a status left unset faults the action; the tick is Failure and the intent the sink
    /// accepted is discarded.
    #[test]
    fn an_unset_action_status_fails_the_tick_discards_its_intents_and_faults_the_action() {
        let env = RawEnv::load(forged!(raw_leaf_parts()), PluginFaultPolicy::Abort);
        let mut action = env.action("unset");

        let (status, intents) = tick(
            action.as_mut(),
            &mut Ctx::new(1),
            &mut leaf_board(true, false),
            1,
        );

        assert_eq!((status, intents), (Status::Failure, Vec::new()));
        assert_eq!(
            accesses(),
            [(UMBRA_OK, "push".to_owned())],
            "the sink accepted it"
        );
        assert_fault(&env, RAW_ACTION, "tick", &["unset"]);
    }

    #[test]
    fn an_out_of_range_action_status_fails_the_tick_and_faults_the_action() {
        let env = RawEnv::load(forged!(raw_leaf_parts()), PluginFaultPolicy::Abort);
        let mut action = env.action("nine");

        let (status, intents) = tick(
            action.as_mut(),
            &mut Ctx::new(1),
            &mut leaf_board(true, false),
            1,
        );

        assert_eq!((status, intents), (Status::Failure, Vec::new()));
        assert_fault(&env, RAW_ACTION, "tick", &["9", "out of range"]);
    }

    /// The control row: a clean tick hands on its intent and keeps its write.
    #[test]
    fn a_clean_tick_hands_on_its_intents_and_keeps_its_write() {
        let env = RawEnv::load(forged!(raw_leaf_parts()), PluginFaultPolicy::Abort);
        let mut action = env.action("write");
        let mut bb = leaf_board(true, false);

        let (status, intents) = tick(action.as_mut(), &mut Ctx::new(1), &mut bb, 1);

        assert_eq!(status, Status::Success);
        assert_eq!(intents, [Intent::SetSpeed(MetersPerSecond(3.0))]);
        assert_eq!(bb.get("done"), Some(&BbValue::Bool(true)));
        assert_eq!(env.faults(), Vec::new());
    }

    /// ASM-723: a faulting check undoes its accepted write; a clean one keeps it.
    #[test]
    fn a_faulting_check_undoes_its_blackboard_write_and_a_clean_one_keeps_it() {
        let env = RawEnv::load(
            forged!(raw_leaf_parts()),
            PluginFaultPolicy::DisableCapability,
        );
        let mut clean = leaf_board(true, false);
        let mut faulting = leaf_board(true, false);

        let kept = check_on(&env, "write", &mut clean);
        let undone = check_on(&env, "write_unset", &mut faulting);

        assert!(kept && !undone);
        assert_eq!(
            accesses(),
            vec![(UMBRA_OK, NO_REPORT.to_owned()); 2],
            "both writes accepted"
        );
        assert_eq!(clean.get("done"), Some(&BbValue::Bool(true)));
        assert_eq!(faulting.get("done"), Some(&BbValue::Bool(false)), "undone");
        assert_fault(&env, RAW_CONDITION, "check", &["unset"]);
    }

    /// ASM-723: a tick that fails after an accepted write (here an unset key) leaves the key unset.
    #[test]
    fn a_failing_tick_undoes_its_blackboard_write() {
        let env = RawEnv::load(forged!(raw_leaf_parts()), PluginFaultPolicy::Abort);
        let mut action = env.action("write_fail");
        let mut bb = board(&[("armed", Some(true)), ("done", None)]);

        let (status, intents) = tick(action.as_mut(), &mut Ctx::new(1), &mut bb, 1);

        assert_eq!((status, intents), (Status::Failure, Vec::new()));
        assert_eq!(
            accesses()[0],
            (UMBRA_OK, NO_REPORT.to_owned()),
            "the write was accepted"
        );
        assert_eq!(bb.get("done"), None, "undone to unset");
        assert_eq!(env.faults().len(), 1);
    }

    /// REQ-K-4: a check and a tick draw from the entity's own stream through the call's RNG token.
    #[test]
    fn a_check_and_a_tick_draw_from_the_entitys_stream() {
        let env = RawEnv::load(forged!(raw_leaf_parts()), PluginFaultPolicy::Abort);
        let mut action = env.action("rng");
        let mut ctx = Ctx::new(5);
        let mut reference = stream(5);
        let want = [reference.next_u64(), reference.next_u64()];

        let answer = check(
            env.condition("rng").as_ref(),
            &mut ctx,
            &mut leaf_board(true, false),
            1,
        );
        let (status, _) = tick(action.as_mut(), &mut ctx, &mut leaf_board(true, false), 1);

        assert!(answer);
        assert_eq!(status, Status::Success);
        assert_eq!(
            accesses(),
            [
                (UMBRA_OK, format!("u64:{}", want[0])),
                (UMBRA_OK, format!("u64:{}", want[1])),
                (UMBRA_OK, "push".to_owned())
            ]
        );
        assert_eq!(
            ctx.rng.next_u64(),
            reference.next_u64(),
            "the stream advanced by two"
        );
    }

    /// A validation host never calls a plugin condition or action: false and Failure, no access.
    #[test]
    fn in_a_validation_host_plugin_leaves_are_never_called() {
        let env = RawEnv::load_in(
            forged!(raw_leaf_parts()),
            PluginFaultPolicy::Abort,
            HostMode::Validation,
        );
        let mut action = env.action("write");
        let mut bb = leaf_board(true, false);

        let answer = check_on(&env, "flag", &mut bb);
        let (status, intents) = tick(action.as_mut(), &mut Ctx::new(1), &mut bb, 1);

        assert!(!answer);
        assert_eq!((status, intents), (Status::Failure, Vec::new()));
        assert_eq!(accesses(), Vec::new(), "no plugin call");
        assert_eq!(bb.get("done"), Some(&BbValue::Bool(false)));
        assert_eq!(env.faults(), Vec::new());
    }

    // ------------------------------------------------------------ review round 1 rows (WI-9)

    /// ASM-723 (review m1): two writes of one key in a faulting call are undone last first, so the key
    /// gets back the value it had before the call (neither written value).
    #[test]
    fn a_faulting_check_undoes_two_writes_of_one_key_last_first() {
        let env = RawEnv::load(forged!(raw_leaf_parts()), PluginFaultPolicy::Abort);
        let mut bb = leaf_board(true, false);

        let answer = check_on(&env, "write_twice_fail", &mut bb);

        assert!(!answer);
        assert_eq!(
            accesses(),
            vec![(UMBRA_OK, NO_REPORT.to_owned()); 2],
            "set true, then unset: both accepted"
        );
        assert_eq!(
            bb.get("done"),
            Some(&BbValue::Bool(false)),
            "the original value"
        );
        assert_eq!(env.faults().len(), 1);
    }

    /// ASM-723 (review m2): the undo log is call-scoped: a clean call's write is never undone by a later
    /// faulting call, whose own blackboard starts at `done = true`, so a stale entry would show.
    #[test]
    fn a_clean_calls_write_is_never_undone_by_a_later_faulting_call() {
        let env = RawEnv::load(forged!(raw_leaf_parts()), PluginFaultPolicy::Abort);
        let mut clean = leaf_board(true, false);
        let mut faulting = leaf_board(true, true);

        let kept = check_on(&env, "write", &mut clean);
        let undone = check_on(&env, "write_unset", &mut faulting);

        assert!(kept && !undone);
        assert_eq!(clean.get("done"), Some(&BbValue::Bool(true)));
        assert_eq!(
            faulting.get("done"),
            Some(&BbValue::Bool(true)),
            "only its own write was undone, to its own old value"
        );
        assert_fault(&env, RAW_CONDITION, "check", &["unset"]);
    }

    /// ASM-723 (a) (review m3): a `null` key param builds and names no key, so a read of that key is
    /// refused as not granted.
    #[test]
    fn a_null_key_param_builds_and_grants_no_key() {
        let env = RawEnv::load(forged!(raw_leaf_parts()), PluginFaultPolicy::Abort);
        let condition = env
            .condition_with(r#"{"mode":"get_armed","key":null,"out":"done"}"#)
            .expect("a null param builds");

        let answer = check(
            condition.as_ref(),
            &mut Ctx::new(1),
            &mut leaf_board(true, false),
            1,
        );

        assert!(!answer);
        assert_eq!(
            accesses(),
            [(UMBRA_ERR_INVALID_ARGUMENT, NO_REPORT.to_owned())]
        );
        assert_fault(
            &env,
            RAW_CONDITION,
            "check",
            &[
                "\"armed\"",
                "not one of the leaf's declared blackboard keys",
            ],
        );
    }

    /// ASM-723 (a) (review m3): an absent key param builds and names no key.
    #[test]
    fn an_absent_key_param_builds_and_grants_no_key() {
        let env = RawEnv::load(forged!(raw_leaf_parts()), PluginFaultPolicy::Abort);
        let condition = env
            .condition_with(r#"{"mode":"get_armed","out":"done"}"#)
            .expect("an absent param builds");

        let answer = check(
            condition.as_ref(),
            &mut Ctx::new(1),
            &mut leaf_board(true, false),
            1,
        );

        assert!(!answer);
        assert_eq!(
            accesses(),
            [(UMBRA_ERR_INVALID_ARGUMENT, NO_REPORT.to_owned())]
        );
        assert_fault(
            &env,
            RAW_CONDITION,
            "check",
            &["not one of the leaf's declared"],
        );
    }

    /// ASM-723 (a) (review m3): a numeric key param is a build error naming the param.
    #[test]
    fn a_numeric_key_param_is_a_build_error_naming_the_param() {
        let env = RawEnv::load(forged!(raw_leaf_parts()), PluginFaultPolicy::Abort);

        let err = env
            .condition_with(r#"{"mode":"get_armed","key":7,"out":"done"}"#)
            .err()
            .expect("a numeric key param is refused");

        let message = err.to_string();
        assert!(message.contains(RAW_CONDITION), "{message}");
        assert!(
            message.contains("\"key\"") && message.contains("string"),
            "{message}"
        );
        assert_eq!(env.faults(), Vec::new(), "a build error, not a fault");
    }

    /// ASM-725 (d) (review m4): `destroy_action` runs after the last `destroy_instance`, also when the
    /// action is dropped before its instance; the instance still ticks in between.
    fn assert_destroy_action_after_the_last_instance(mode: &str, expected: Status) {
        let env = RawEnv::load(
            forged!(raw_leaf_parts()),
            PluginFaultPolicy::DisableCapability,
        );
        let (action, mut instance) = env.action_parts(mode);

        drop(action);
        let (status, _) = tick(
            instance.as_mut(),
            &mut Ctx::new(1),
            &mut leaf_board(true, false),
            1,
        );
        let before = destroyed_with();
        drop(instance);

        assert_eq!(status, expected, "{mode}");
        assert_eq!(
            before, None,
            "{mode}: destroy_action waits for the instance"
        );
        assert_eq!(
            destroyed_with(),
            Some(0),
            "{mode}: no live instance when it ran"
        );
    }

    #[test]
    fn destroy_action_runs_after_the_last_instance_when_the_action_drops_first() {
        assert_destroy_action_after_the_last_instance("write", Status::Success);
    }

    #[test]
    fn destroy_action_runs_after_the_last_instance_when_the_action_drops_first_after_a_fault() {
        assert_destroy_action_after_the_last_instance("unset", Status::Failure);
    }

    /// Review n1: a capability faulted during a call (here by a re-entrant check of the same
    /// capability from inside the plugin's value sink, refused as re-entry) discards that call's clean
    /// result: the outer check read `armed = true` but answers false, and the fault is recorded once.
    #[test]
    fn a_check_whose_capability_is_faulted_during_the_call_answers_false() {
        let env = RawEnv::load(
            forged!(raw_leaf_parts()),
            PluginFaultPolicy::DisableCapability,
        );
        let outer = env.condition("flag");
        let inner = env.condition("flag");
        // The hook owns a clone: the condition itself is dropped after the outer call (re-review
        // n-a), so its `destroy` is not refused as re-entry.
        let reentrant = std::sync::Arc::clone(&inner);
        REENTER.with(|h| {
            *h.borrow_mut() = Some(Box::new(move || {
                let _ = check(
                    reentrant.as_ref(),
                    &mut Ctx::new(2),
                    &mut leaf_board(true, false),
                    1,
                );
            }));
        });

        let answer = check(
            outer.as_ref(),
            &mut Ctx::new(1),
            &mut leaf_board(true, false),
            1,
        );

        assert_eq!(
            accesses(),
            [(UMBRA_OK, "bool:true".to_owned())],
            "the read itself succeeded"
        );
        assert!(!answer, "the faulted capability's result is discarded");
        assert_fault(&env, RAW_CONDITION, "check", &["re-entrant"]);
        drop(inner);
        drop(outer);
        let skipped: Vec<_> = env
            .log
            .all()
            .into_iter()
            .filter(|(_, _, message)| message.contains("destroy skipped"))
            .collect();
        assert_eq!(skipped, Vec::new(), "both conditions destroyed");
    }
}
