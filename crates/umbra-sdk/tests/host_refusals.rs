//! WI-7: host refusals and logging through the stored host API. Requirements: REQ-K-4 (RNG only
//! through the host, a refused draw faults the call), REQ-P-5 (logging), REQ-P-7 (swallowed panics are
//! logged), ADR 0020 (a refused intent ends the call with the host's code).
//!
//! One test function: the SDK's stored host API is process-global, and this file installs hosts that
//! refuse (through `init`, which always replaces it; later entry calls of a lifecycle keep it), so its
//! steps must not run in parallel with each other.
#![expect(unsafe_code, reason = "plays the host side of the C ABI")]
#![expect(
    clippy::borrow_as_ptr,
    clippy::cast_possible_truncation,
    clippy::expect_used,
    reason = "test harness casts small ABI sizes and plays the C side"
)]

mod support;

use std::mem::size_of;

use support::{Rig, configure, draws, host_api, instantiate, logs, pushed, reset, vtable};
use umbra_sdk::abi::{
    UMBRA_ABI_VERSION, UMBRA_ERR_INVALID_ARGUMENT, UMBRA_ERR_STATE, UMBRA_LOG_ERROR,
    UMBRA_LOG_INFO, UMBRA_LOG_TRACE, UMBRA_LOG_WARN, UMBRA_OK, UMBRA_STATUS_UNSET, UmbraCode,
    UmbraHostApi, UmbraIntent, UmbraIntentSink, UmbraToken,
};
use umbra_sdk::{Level, log};

unsafe extern "C" fn refuse_u64(_: UmbraToken, _: UmbraToken, _: *mut u64) -> UmbraCode {
    UMBRA_ERR_STATE
}
unsafe extern "C" fn refuse_push(_: UmbraToken, _: usize, _: *const UmbraIntent) -> UmbraCode {
    UMBRA_ERR_INVALID_ARGUMENT
}

/// Runs the SDK's `init` with `host` (`init` always replaces the stored host API).
fn init(host: &'static UmbraHostApi) -> UmbraCode {
    let err = support::err_sink();
    let init = support::descriptor()
        .init
        .expect("the SDK always fills init");
    // SAFETY: the SDK's init with a live host API and sink.
    unsafe { init(host, &err) }
}

#[test]
fn refusals_and_logging_go_through_the_stored_host_api() {
    // 1. Logging after the entry reaches the host with the mapped level.
    reset();
    let _ = support::descriptor();
    log(Level::Info, "hello");
    log(Level::Warn, "careful");
    log(Level::Trace, "detail");
    assert_eq!(
        logs(),
        [
            (UMBRA_LOG_INFO, "hello".to_owned()),
            (UMBRA_LOG_WARN, "careful".to_owned()),
            (UMBRA_LOG_TRACE, "detail".to_owned()),
        ]
    );

    // 2. A refused RNG draw ends the call with the host's code; the item gets no status.
    reset();
    let refusing = Box::leak(Box::new(UmbraHostApi {
        rng_next_u64: Some(refuse_u64),
        ..host_api()
    }));
    assert_eq!(init(refusing), UMBRA_OK);
    let mut rig = Rig::new(&[(1, 11)], "{}");
    let (code, msg) = rig.tick(0);
    assert_eq!(code, UMBRA_ERR_STATE);
    assert!(msg.is_some_and(|m| m.contains("rng_next_u64")));
    assert_eq!(rig.statuses, [UMBRA_STATUS_UNSET]);

    // 3. A host without RNG callbacks is `UMBRA_ERR_STATE` too.
    reset();
    let no_rng = Box::leak(Box::new(UmbraHostApi {
        rng_next_u64: None,
        rng_next_f64: None,
        ..host_api()
    }));
    assert_eq!(init(no_rng), UMBRA_OK);
    assert_eq!(rig.tick(1).0, UMBRA_ERR_STATE);

    // 4. A refused intent ends the call with the sink's code, even though the plugin ignores the
    //    error of `push`.
    reset();
    assert_eq!(init(Box::leak(Box::new(host_api()))), UMBRA_OK);
    let sink = UmbraIntentSink {
        abi_version: UMBRA_ABI_VERSION,
        struct_size: size_of::<UmbraIntentSink>() as u32,
        ctx: support::SINK_CTX,
        push: Some(refuse_push),
    };
    let mut b = rig.batch(2);
    b.intents = &sink;
    let (code, msg) = support::call_tick(&b);
    assert_eq!(code, UMBRA_ERR_INVALID_ARGUMENT);
    assert!(msg.is_some_and(|m| m.contains("intent push")));
    assert_eq!(pushed(), Vec::new());
    assert!(
        !draws().is_empty(),
        "the draw before the refused push happened"
    );

    // 5. A panic in `destroy_*` is swallowed and logged at error level.
    reset();
    let (_, b, _) = configure("{\"panic\":\"destroy_instance\"}");
    let (_, i, _) = instantiate(b);
    // SAFETY: handles from configure/instantiate, destroyed once each.
    unsafe {
        vtable().destroy_instance.expect("destroy_instance")(i);
        vtable().destroy_behavior.expect("destroy_behavior")(b);
    }
    let errors: Vec<_> = logs()
        .into_iter()
        .filter(|(level, _)| *level == UMBRA_LOG_ERROR)
        .collect();
    assert!(
        errors
            .iter()
            .any(|(_, m)| m.contains("destroy_instance") && m.contains("boom destroy_instance")),
        "{errors:?}"
    );

    // 6. A panic in `halt` (no error sink) is logged at error level as well.
    reset();
    let rig = Rig::new(&[(1, 11)], "{\"panic\":\"halt\"}");
    // SAFETY: a live instance from the rig.
    let code = unsafe { vtable().halt.expect("halt")(rig.items[0].instance) };
    assert_eq!(code, umbra_sdk::abi::UMBRA_ERR_PANIC);
    assert!(
        logs()
            .iter()
            .any(|(l, m)| *l == UMBRA_LOG_ERROR && m.contains("halt") && m.contains("boom halt"))
    );
    assert_eq!(UMBRA_OK, 0);
}
