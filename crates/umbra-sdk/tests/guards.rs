//! AT-3 (guard rows): panics never cross the boundary and malformed input is refused before any
//! trait call. Requirements: REQ-P-7, REQ-SDK-1, R-M1, R-M2, ADR 0019. Step 4 of M4 (WI-7 stub).
#![expect(unsafe_code, reason = "plays the host side of the C ABI")]
#![expect(
    clippy::borrow_as_ptr,
    clippy::cast_possible_truncation,
    clippy::semicolon_if_nothing_returned,
    reason = "test harness casts small ABI sizes and plays the C side"
)]

mod support;

use std::mem::size_of;

use support::{Rig, configure, draws, err_sink, instantiate, pushed, reset, s, vtable};
use umbra_sdk::abi::{
    UMBRA_ABI_VERSION, UMBRA_ERR_INVALID_ARGUMENT, UMBRA_ERR_PANIC, UMBRA_ERR_PARAMS, UMBRA_OK,
    UMBRA_STATUS_UNSET, UmbraBehaviorBatch, UmbraBehaviorItem, UmbraStr, UmbraValueSink,
};

fn params(mode: &str) -> String {
    format!("{{\"panic\":\"{mode}\"}}")
}

/// REQ-P-7: a panic in each export is `UMBRA_ERR_PANIC`, with the panic message in the error sink where
/// the export has one.
#[test]
#[ignore = "WI-7"]
fn panic_in_each_export_is_err_panic_with_message() {
    reset();
    let (code, _, msg) = configure(&params("configure"));
    assert_eq!(code, UMBRA_ERR_PANIC, "configure");
    assert!(
        msg.is_some_and(|m| m.contains("boom configure")),
        "configure message"
    );

    let (code, b, _) = configure(&params("instantiate"));
    assert_eq!(code, UMBRA_OK);
    let (code, _, msg) = instantiate(b);
    assert_eq!(code, UMBRA_ERR_PANIC, "instantiate");
    assert!(
        msg.is_some_and(|m| m.contains("boom instantiate")),
        "instantiate message"
    );

    let mut rig = Rig::new(&[(1, 11)], &params("tick"));
    let (code, msg) = rig.tick(0);
    assert_eq!(code, UMBRA_ERR_PANIC, "tick_batch");
    assert!(msg.is_some_and(|m| m.contains("boom tick")), "tick message");

    let vt = vtable();
    let err = err_sink();
    for (mode, name) in [("halt", "halt"), ("halt_running", "halt_running")] {
        let rig = Rig::new(&[(1, 11)], &params(mode));
        let f = if name == "halt" {
            vt.halt
        } else {
            vt.halt_running
        };
        // SAFETY: a live instance from the rig.
        let code = unsafe { f.expect("entry")(rig.items[0].instance) };
        assert_eq!(code, UMBRA_ERR_PANIC, "{name}");
    }

    let rig = Rig::new(&[(1, 11)], &params("set_input"));
    // SAFETY: a live instance; valid strings and sink.
    let code = unsafe {
        vt.set_input.expect("set_input")(rig.items[0].instance, s("echo"), std::ptr::null(), &err)
    };
    assert_eq!(code, UMBRA_ERR_PANIC, "set_input");

    let rig = Rig::new(&[(1, 11)], &params("output"));
    let sink: UmbraValueSink = support::value_sink();
    // SAFETY: a live instance; valid port and sink.
    let code = unsafe { vt.output.expect("output")(rig.items[0].instance, s("turns"), &sink) };
    assert_eq!(code, UMBRA_ERR_PANIC, "output");
}

/// R-M1: a panic payload whose `Drop` panics again is dropped inside a nested guard; the call still
/// returns an error code and the next call works.
#[test]
#[ignore = "WI-7"]
fn panicking_payload_drop_does_not_escape() {
    reset();
    let mut rig = Rig::new(&[(1, 11)], &params("drop_payload"));

    let (code, _) = rig.tick(0);

    assert_eq!(code, UMBRA_ERR_PANIC);
    let (code, _, _) = configure("{}");
    assert_eq!(code, UMBRA_OK, "the plugin is still usable");
}

/// R-M1: `destroy_*` cannot report; a panic in either is swallowed (an escape would abort the process).
#[test]
#[ignore = "WI-7"]
fn destroy_panics_are_swallowed() {
    let (_, b, _) = configure(&params("destroy_instance"));
    let (_, i, _) = instantiate(b);
    // SAFETY: handles from configure/instantiate, destroyed once each.
    unsafe { vtable().destroy_instance.expect("destroy_instance")(i) };
    unsafe { vtable().destroy_behavior.expect("destroy_behavior")(b) };

    let (_, b, _) = configure(&params("destroy_behavior"));
    // SAFETY: as above.
    unsafe { vtable().destroy_behavior.expect("destroy_behavior")(b) };
}

/// REQ-P-6: an `Error::params` returned by `configure` is `UMBRA_ERR_PARAMS` with its message, and no
/// handle; so is the error of `Params::value` on text that is not JSON (the SDK maps the error's code).
#[test]
#[ignore = "WI-7"]
fn bad_params_json_is_err_params() {
    let (code, handle, msg) = configure("{not json");

    assert_eq!(code, UMBRA_ERR_PARAMS);
    assert!(handle.is_null());
    assert!(msg.is_some_and(|m| !m.is_empty()));

    let (code, handle, msg) = configure(&params("no_such_mode"));

    assert_eq!(code, UMBRA_ERR_PARAMS, "the factory's own Error::params");
    assert!(handle.is_null());
    assert!(msg.is_some_and(|m| m.contains("no_such_mode")));
}

/// Runs a forged batch and checks that it was refused before any trait call: invalid-argument code,
/// no RNG draw, no intent, statuses untouched.
fn assert_rejected(name: &str, forge: impl FnOnce(&mut Rig, &mut UmbraBehaviorBatch)) {
    reset();
    let mut rig = Rig::new(&[(1, 11), (2, 22)], "{}");
    let mut b = rig.batch(0);
    forge(&mut rig, &mut b);
    let (code, msg) = support::call_tick(&b);
    assert_eq!(code, UMBRA_ERR_INVALID_ARGUMENT, "{name}");
    assert!(msg.is_some(), "{name}: message");
    assert!(
        draws().is_empty() && pushed().is_empty(),
        "{name}: a trait call happened"
    );
    assert!(
        rig.statuses.iter().all(|s| *s == UMBRA_STATUS_UNSET),
        "{name}: statuses written"
    );
}

/// R-M2: every malformed-batch row is rejected before the trait is called.
#[test]
#[ignore = "WI-7"]
fn malformed_batch_rejected_before_trait_call() {
    assert_rejected("null items", |_, b| b.items = std::ptr::null());
    assert_rejected("null statuses", |_, b| b.statuses = std::ptr::null_mut());
    assert_rejected("null intent sink", |_, b| b.intents = std::ptr::null());
    assert_rejected("short batch struct_size", |_, b| b.struct_size = 16);
    assert_rejected("batch struct_size not a multiple of 8", |_, b| {
        b.struct_size = size_of::<UmbraBehaviorBatch>() as u32 - 4;
    });
    assert_rejected("other major in the batch", |_, b| {
        b.abi_version = (1 << 16) | UMBRA_ABI_VERSION
    });
    assert_rejected("short item struct_size", |r, _| r.items[0].struct_size = 16);
    assert_rejected("item struct_size not a multiple of 8", |r, _| {
        r.items[1].struct_size = size_of::<UmbraBehaviorItem>() as u32 - 4;
    });
    assert_rejected("misaligned items array", |_, b| {
        // SAFETY: only the address is computed; a correct SDK rejects it without reading.
        b.items = unsafe { b.items.cast::<u8>().add(4) }.cast();
    });
    assert_rejected("invalid UTF-8 in a force", |r, _| {
        static BAD: [u8; 2] = [0xFF, 0xFE];
        r.views[1].force = UmbraStr {
            ptr: BAD.as_ptr(),
            len: 2,
        };
    });
    assert_rejected("short intent sink struct_size", |_, b| {
        let mut sink = support::intent_sink();
        sink.struct_size = 16;
        b.intents = Box::leak(Box::new(sink));
    });
    assert_rejected("other major in the intent sink", |_, b| {
        let mut sink = support::intent_sink();
        sink.abi_version = (1 << 16) | UMBRA_ABI_VERSION;
        b.intents = Box::leak(Box::new(sink));
    });
    assert_rejected("entity id 0", |r, _| r.views[0].id = 0);
    assert_rejected("null entity view", |r, _| {
        r.items[0].entity = std::ptr::null()
    });
}

/// R-M2: an error sink with a short `struct_size`, a size that is not a multiple of 8 or another major
/// is refused before the trait is called: invalid-argument code and no handle (there is no sink to
/// write a message to).
#[test]
#[ignore = "WI-7"]
fn malformed_error_sink_rejected_before_trait_call() {
    type Forge<T> = fn(&mut T);
    let forged: [(&str, Forge<umbra_sdk::abi::UmbraErrorSink>); 3] = [
        ("short struct_size", |e| e.struct_size = 16),
        ("struct_size not a multiple of 8", |e| {
            e.struct_size = size_of::<umbra_sdk::abi::UmbraErrorSink>() as u32 - 4;
        }),
        ("other major", |e| {
            e.abi_version = (1 << 16) | UMBRA_ABI_VERSION
        }),
    ];
    for (name, forge) in forged {
        reset();
        let mut err = err_sink();
        forge(&mut err);
        let mut out: *mut umbra_sdk::abi::UmbraBehavior = std::ptr::null_mut();
        let f = vtable().configure.expect("configure");

        // SAFETY: valid params and out; the sink is forged on purpose and must be refused unread.
        let code = unsafe { f(s("{}"), &mut out, &err) };

        assert_eq!(code, UMBRA_ERR_INVALID_ARGUMENT, "{name}");
        assert!(out.is_null(), "{name}: no handle");
    }
}

/// R-M2: the same for the value sink handed to `output`: refused before the trait is called, and
/// nothing is written to the sink.
#[test]
#[ignore = "WI-7"]
fn malformed_value_sink_rejected_before_trait_call() {
    type Forge<T> = fn(&mut T);
    let forged: [(&str, Forge<UmbraValueSink>); 3] = [
        ("short struct_size", |v| v.struct_size = 16),
        ("struct_size not a multiple of 8", |v| {
            v.struct_size = size_of::<UmbraValueSink>() as u32 - 4;
        }),
        ("other major", |v| {
            v.abi_version = (1 << 16) | UMBRA_ABI_VERSION
        }),
    ];
    for (name, forge) in forged {
        reset();
        let rig = Rig::new(&[(1, 11)], "{}");
        let mut sink: UmbraValueSink = support::value_sink();
        forge(&mut sink);

        // SAFETY: a live instance and port; the sink is forged on purpose and must be refused unread.
        let code =
            unsafe { vtable().output.expect("output")(rig.items[0].instance, s("turns"), &sink) };

        assert_eq!(code, UMBRA_ERR_INVALID_ARGUMENT, "{name}");
        assert!(support::values().is_empty(), "{name}: nothing written");
    }
}

/// REQ-P-6: a header with the plugin's own major is accepted (control for the rows above).
#[test]
#[ignore = "WI-7"]
fn well_formed_batch_is_accepted() {
    reset();
    let mut rig = Rig::new(&[(1, 11)], "{}");
    assert_eq!(rig.batch(0).abi_version, UMBRA_ABI_VERSION);

    let (code, _) = rig.tick(0);

    assert_eq!(code, UMBRA_OK);
}
