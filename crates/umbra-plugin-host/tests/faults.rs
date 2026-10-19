//! AT-4 (faults, calls and values): what a misbehaving plugin does to the host, per engine. Requirements:
//! REQ-P-7, REQ-B-4, REQ-B-5, REQ-K-4, ADR 0019, ADR 0020 (whole-batch discard), ADR 0021 (fault policy,
//! per-host state), ASM-118. Raw-ABI plugin from `support/raw.rs`; the engine-scope build helper is a
//! plan gap (see `support::build_behavior`). Step 4 of M4: the bridge lands in WI-9.
#![expect(clippy::expect_used, reason = "helpers build known-valid fixtures")]
#![expect(unsafe_code, reason = "raw_env! expands forged entry functions")]

#[macro_use]
mod support;

use std::cell::RefCell;
use std::sync::Arc;

use support::Recorder;
use support::bridge::{Ent, rate, tick_batched, tick_singly};
use support::raw;
use umbra_core::{Behavior, BehaviorBatch, PluginFaultPolicy, Status};
use umbra_plugin_host::{HostMode, HostOptions, LogLevel, PluginHost};

fn batch_of(b: &Arc<dyn Behavior>) -> Arc<dyn BehaviorBatch> {
    b.batch().expect("batch")
}

/// Ticks one entity of `mode` after a healthy neighbour; yields the env and both entities. A macro, so
/// that each test gets its own forged image (one call site per test, `support/forge.rs`).
macro_rules! faulting {
    ($mode:expr) => {{
        raw::clear();
        let env = raw_env!();
        let good = env.behavior("ok");
        let bad = env.behavior($mode);
        let batch = batch_of(&good);
        let mut ents = vec![Ent::new(1, &good, 3), Ent::new(2, &bad, 3)];
        tick_batched(&batch, &mut ents, 0);
        (env, ents)
    }};
}

macro_rules! tick_fault {
    ($name:ident, $mode:expr, $needle:expr) => {
        #[test]
        #[ignore = "WI-9"]
        fn $name() {
            let (env, ents) = faulting!($mode);

            let faults = env.host.faults();
            assert_eq!(faults.len(), 1, "one fault: {faults:?}");
            assert_eq!(faults[0].call, "tick_batch");
            assert_eq!(faults[0].plugin.as_str(), "test.plugin.raw");
            assert_eq!(faults[0].capability.as_str(), raw::CAPABILITY);
            assert!(
                faults[0].message.contains($needle),
                "{:?} in {:?}",
                $needle,
                faults[0].message
            );
            for e in &ents {
                assert_eq!(
                    e.status,
                    Some(Status::Failure),
                    "whole batch discarded: {:?}",
                    e.id
                );
                assert!(
                    e.buf.is_empty(),
                    "no intents survive a faulted batch: {:?}",
                    e.id
                );
            }
        }
    };
}

tick_fault!(
    tick_error_code_is_a_fault_with_the_plugins_message,
    "error_code",
    "raw tick failed"
);
tick_fault!(status_left_unset_is_a_fault, "unset", "unset");
tick_fault!(status_out_of_range_is_a_fault, "bad_status", "7");
tick_fault!(invalid_intent_kind_is_a_fault, "bad_kind", "kind");
tick_fault!(sixty_fifth_intent_for_one_item_is_a_fault, "flood", "64");
tick_fault!(intent_item_index_at_count_is_a_fault, "bad_index", "index");
tick_fault!(
    intent_struct_size_too_small_is_a_fault,
    "short_intent",
    "struct_size"
);
tick_fault!(fire_with_a_zero_target_is_a_fault, "zero_fire", "target");
tick_fault!(intent_with_a_bad_id_is_a_fault, "bad_id", "id");
tick_fault!(
    custom_intent_with_bad_json_is_a_fault,
    "bad_custom",
    "payload"
);

/// A status left `UNSET` and a status out of range are different faults (R-B2): the messages differ.
#[test]
#[ignore = "WI-9"]
fn unset_and_out_of_range_status_are_reported_differently() {
    let (a, _) = faulting!("unset");
    let (b, _) = faulting!("bad_status");

    assert_ne!(a.host.faults()[0].message, b.host.faults()[0].message);
}

/// ADR 0020: RNG draws made before the fault stay consumed (the streams advanced), the intents do not.
#[test]
#[ignore = "WI-9"]
fn whole_batch_discard_keeps_rng_draws_consumed() {
    let (_env, ents) = faulting!("error_code");

    let mut reference =
        umbra_core::SimRng::new(3).stream(umbra_core::StreamKey::entity(ents[0].id));
    reference.next_u64();
    assert_eq!(
        ents[0].rng.state(),
        reference.state(),
        "the healthy item drew once before the fault"
    );
    assert_eq!(ents[0].buf, Vec::new());
}

/// ADR 0021: `abort` is the default; the first fault is handed out once.
#[test]
#[ignore = "WI-9"]
fn abort_policy_hands_out_the_first_fault_once() {
    let (env, _) = faulting!("error_code");

    let first = env.host.take_abort();
    let second = env.host.take_abort();

    assert_eq!(first.map(|f| f.call), Some("tick_batch"));
    assert!(second.is_none());
}

/// ADR 0021: `disable_capability` skips the capability from then on: items fail without a plugin call,
/// `configure` and `instantiate` fail, and the capability is marked faulted.
#[test]
#[ignore = "WI-9"]
fn disable_capability_skips_the_capability_afterwards() {
    raw::clear();
    let env = raw_env!();
    env.host
        .set_fault_policy(PluginFaultPolicy::DisableCapability);
    let bad = env.behavior("error_code");
    let batch = batch_of(&bad);
    let mut ents = vec![Ent::new(1, &bad, 3)];
    tick_batched(&batch, &mut ents, 0);
    let calls_after_fault = raw::seen().len();

    tick_batched(&batch, &mut ents, 1);

    assert_eq!(
        raw::seen().len(),
        calls_after_fault,
        "the faulted capability is not called again"
    );
    assert_eq!(ents[0].status, Some(Status::Failure));
    assert_eq!(ents[0].buf, Vec::new());
    assert!(env.host.plugins()[0].capabilities[0].faulted);
    assert!(
        env.try_behavior("ok").is_err(),
        "configure fails on a faulted capability"
    );
    assert!(
        bad.instantiate(rate()).is_err(),
        "instantiate fails on a faulted capability"
    );
    assert!(
        env.host.take_abort().is_none(),
        "no abort under disable_capability"
    );
}

/// ADR 0021: `destroy_*` is still called for handles of a faulted capability.
#[test]
#[ignore = "WI-9"]
fn destroy_still_called_on_a_faulted_capability() {
    raw::clear();
    let env = raw_env!();
    let bad = env.behavior("error_code");
    let batch = batch_of(&bad);
    let mut ents = vec![Ent::new(1, &bad, 3)];
    tick_batched(&batch, &mut ents, 0);

    drop(ents);
    drop(batch);
    drop(bad);

    let events = raw::events();
    assert!(
        events.contains(&"destroy_instance".to_owned()),
        "{events:?}"
    );
    assert!(
        events.contains(&"destroy_behavior".to_owned()),
        "{events:?}"
    );
}

/// ADR 0021: a fault is logged once at error level with plugin, capability and call.
#[test]
#[ignore = "WI-9"]
fn a_fault_is_logged_once_at_error_level() {
    raw::clear();
    let recorder = Recorder::arc();
    let mut options = HostOptions::default();
    options.log = recorder.clone();
    let env = raw_env_with!(options, true);
    let bad = env.behavior("error_code");
    let batch = batch_of(&bad);
    let mut ents = vec![Ent::new(1, &bad, 3)];

    tick_batched(&batch, &mut ents, 0);

    let errors: Vec<_> = recorder
        .all()
        .into_iter()
        .filter(|e| e.1 == LogLevel::Error)
        .collect();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].0, "test.plugin.raw");
    assert!(
        errors[0].2.contains(raw::CAPABILITY) && errors[0].2.contains("tick_batch"),
        "{}",
        errors[0].2
    );
}

/// ASM-118, R-M3: two hosts over one image keep independent fault state and policy.
#[test]
#[ignore = "WI-9"]
fn two_hosts_over_one_image_keep_independent_fault_state() {
    raw::clear();
    let first = raw_env!();
    let mut second_host = PluginHost::new(HostOptions::default());
    second_host
        .load_static("second-host", first.f.entry)
        .expect("shared image");
    let mut second_registry = umbra_core::Registry::new();
    second_host
        .register(&mut second_registry)
        .expect("register");
    let addr = first.f.host.load(std::sync::atomic::Ordering::SeqCst);
    let params = raw::params("ok", addr);
    let theirs = support::build_behavior(&second_host, &second_registry, raw::CAPABILITY, &params)
        .expect("build");
    let bad = first.behavior("error_code");
    let mut ents = vec![Ent::new(1, &bad, 3)];
    tick_batched(&batch_of(&bad), &mut ents, 0);

    let mut other = vec![Ent::new(1, &theirs, 3)];
    tick_batched(&batch_of(&theirs), &mut other, 0);

    assert_eq!(first.host.faults().len(), 1);
    assert_eq!(second_host.faults(), Vec::new());
    assert!(!second_host.plugins()[0].capabilities[0].faulted);
    assert_eq!(
        other[0].status,
        Some(Status::Running),
        "the second engine is unaffected"
    );
}

/// R-M9: `configure` returning an error is a `BuildError` with the plugin's message, not a fault.
#[test]
#[ignore = "WI-9"]
fn configure_error_is_a_build_error() {
    raw::clear();
    let env = raw_env!();

    let err = env
        .try_behavior("configure_error")
        .err()
        .expect("build error");

    assert!(err.message.contains("raw configure error"), "{err}");
}

/// R-M9: `configure` returning OK with a null handle is a build error.
#[test]
#[ignore = "WI-9"]
fn configure_ok_with_a_null_handle_is_a_build_error() {
    raw::clear();
    let env = raw_env!();

    let err = env.try_behavior("null_handle").err().expect("build error");

    assert!(err.message.contains("null"), "{err}");
}

/// R-M9: `instantiate` returning an error is an `InstantiateError`.
#[test]
#[ignore = "WI-9"]
fn instantiate_error_is_an_instantiate_error() {
    raw::clear();
    let env = raw_env!();
    let b = env.behavior("instantiate_error");

    let err = b.instantiate(rate()).err().expect("instantiate error");

    assert!(err.to_string().contains("raw instantiate error"), "{err}");
}

/// R-M9: `instantiate` returning OK with a null handle is an `InstantiateError`, never a null instance.
#[test]
#[ignore = "WI-9"]
fn instantiate_ok_with_a_null_handle_is_an_instantiate_error() {
    raw::clear();
    let env = raw_env!();
    let b = env.behavior("instantiate_null");

    let err = b.instantiate(rate()).err().expect("instantiate error");

    assert!(err.to_string().contains("null"), "{err}");
}

/// R-M9: `halt` returning an error code is a fault of call `halt`.
#[test]
#[ignore = "WI-9"]
fn halt_returning_an_error_code_is_a_fault() {
    raw::clear();
    let env = raw_env!();
    let b = env.behavior("halt_error");
    let mut inst = b.instantiate(rate()).expect("instantiate");

    inst.halt();

    let faults = env.host.faults();
    assert_eq!(faults.len(), 1);
    assert_eq!(faults[0].call, "halt");
}

macro_rules! output_fault {
    ($name:ident, $mode:expr) => {
        #[test]
        #[ignore = "WI-9"]
        fn $name() {
            raw::clear();
            let env = raw_env!();
            let b = env.behavior($mode);
            let inst = b.instantiate(rate()).expect("instantiate");

            let value = inst.output("out_integer");

            assert_eq!(value, None, "a bad value is not passed on");
            let faults = env.host.faults();
            assert_eq!(faults.len(), 1, "{faults:?}");
            assert_eq!(faults[0].call, "output");
        }
    };
}

output_fault!(output_sink_called_twice_is_a_fault, "output_twice");
output_fault!(value_with_an_unknown_tag_is_a_fault, "bad_tag");
output_fault!(value_with_boolean_2_is_a_fault, "bad_bool");
output_fault!(value_with_a_non_finite_number_is_a_fault, "nan");
output_fault!(value_with_entity_0_is_a_fault, "zero_entity");
output_fault!(value_with_invalid_utf8_is_a_fault, "bad_utf8");

/// RR-B1: re-entering the same image on the same thread (from inside a plugin call) is a fault, never a
/// deadlock.
#[test]
#[ignore = "WI-9"]
fn same_thread_reentry_is_a_fault() {
    raw::clear();
    let env = raw_env!();
    let outer = env.behavior("reenter");
    let inner_behavior = env.behavior("ok");
    let inner = RefCell::new(Ent::new(9, &inner_behavior, 1));
    raw::HOOK.with(|h| {
        *h.borrow_mut() = Some(Box::new(move || {
            tick_singly(std::slice::from_mut(&mut inner.borrow_mut()), 0);
        }));
    });
    let mut ents = vec![Ent::new(1, &outer, 3)];

    tick_batched(&batch_of(&outer), &mut ents, 0);

    let faults = env.host.faults();
    assert!(!faults.is_empty(), "re-entry is a fault");
    assert!(
        faults
            .iter()
            .any(|f| f.message.to_lowercase().contains("re-entr")),
        "{faults:?}"
    );
    assert_eq!(ents[0].status, Some(Status::Failure));
}

/// Validation hosts (RR-m2): builds work, instances refuse to tick.
#[test]
#[ignore = "WI-9"]
fn validation_host_builds_but_its_instances_refuse_to_tick() {
    raw::clear();
    let mut options = HostOptions::default();
    options.mode = HostMode::Validation;
    let env = raw_env_with!(options, true);
    let b = env.behavior("ok");
    let mut ents = vec![Ent::new(1, &b, 3)];

    tick_batched(&batch_of(&b), &mut ents, 0);

    assert_eq!(ents[0].status, Some(Status::Failure));
    assert_eq!(ents[0].buf, Vec::new());
    assert!(
        raw::seen().is_empty(),
        "the plugin is never ticked in validation mode"
    );
}
