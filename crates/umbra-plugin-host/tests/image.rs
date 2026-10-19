//! AT-4 (image table and lifecycle rows that need no real library), R-B5 and the re-review RR-M1..M3.
//! Requirements: REQ-P-8, ADR 0021 (one image per process, lifecycle states, `keep_loaded`), ASM-122.
//! Entry functions are forged (`load_static` keys the image by entry address). Step 4 of M4 (WI-8 stub).
#![expect(unsafe_code, reason = "calls the host API like a plugin would")]

#[macro_use]
mod support;

use std::sync::atomic::Ordering;
use std::thread;

use support::Recorder;
use support::forge::parts;
use umbra_plugin_api::{UMBRA_ERR_STATE, UMBRA_LOG_INFO, UmbraHostApi};
use umbra_plugin_host::{HostOptions, PluginError, PluginHost};

fn host() -> PluginHost {
    PluginHost::new(HostOptions::default())
}

fn api(f: &support::forge::Forged) -> UmbraHostApi {
    let p = f.host.load(Ordering::SeqCst) as *const UmbraHostApi;
    assert!(!p.is_null(), "the entry was called");
    // SAFETY: the host API is a process-lifetime struct (ADR 0021).
    unsafe { std::ptr::read(p) }
}

#[test]
#[ignore = "WI-8"]
fn same_entry_twice_in_one_host_is_duplicate_found_before_a_second_entry_call() {
    let f = forged!(parts());
    let mut h = host();
    h.load_static("first-origin", f.entry).expect("first load");

    let err = h
        .load_static("second-origin", f.entry)
        .expect_err("duplicate image");

    assert!(matches!(err, PluginError::Duplicate { .. }), "{err:?}");
    let msg = err.to_string();
    assert!(
        msg.contains("first-origin") && msg.contains("second-origin"),
        "names both: {msg}"
    );
    assert_eq!(
        f.entry_calls.load(Ordering::SeqCst),
        2,
        "no entry call for the duplicate"
    );
}

#[test]
#[ignore = "WI-8"]
fn same_plugin_id_from_two_images_is_duplicate() {
    let a = forged!(parts());
    let b = forged!(parts());
    let mut h = host();
    h.load_static("image-a", a.entry).expect("first");

    let err = h
        .load_static("image-b", b.entry)
        .expect_err("same plugin id");

    assert!(matches!(err, PluginError::Duplicate { .. }), "{err:?}");
    assert!(err.to_string().contains("test.plugin.forge"));
}

#[test]
#[ignore = "WI-8"]
fn two_hosts_share_one_image_with_one_entry_pair_one_init_one_shutdown() {
    let f = forged!(parts());
    let mut first = host();
    let mut second = host();
    first.load_static("host-1", f.entry).expect("first");
    second.load_static("host-2", f.entry).expect("second");

    assert_eq!(f.entry_calls.load(Ordering::SeqCst), 2);
    assert_eq!(f.init_calls.load(Ordering::SeqCst), 1);
    drop(first);
    assert_eq!(
        f.shutdown_calls.load(Ordering::SeqCst),
        0,
        "the image is still held by host 2"
    );
    drop(second);
    assert_eq!(f.shutdown_calls.load(Ordering::SeqCst), 1);
}

#[test]
#[ignore = "WI-8"]
fn two_hosts_load_the_same_entry_concurrently() {
    let f = forged!(parts());
    let entry = f.entry;

    let handles: Vec<_> = (0..2)
        .map(|n| {
            thread::spawn(move || {
                let mut h = host();
                h.load_static(&format!("thread-{n}"), entry)
                    .map(|i| i.id.as_str().to_owned())
            })
        })
        .collect();
    let ids: Vec<_> = handles
        .into_iter()
        .map(|t| t.join().expect("thread"))
        .collect();

    assert!(ids.iter().all(Result::is_ok), "{ids:?}");
    assert_eq!(f.entry_calls.load(Ordering::SeqCst), 2, "one lifecycle");
    assert_eq!(f.init_calls.load(Ordering::SeqCst), 1);
    assert_eq!(f.shutdown_calls.load(Ordering::SeqCst), 1);
}

#[test]
#[ignore = "WI-8"]
fn reload_after_release() {
    let f = forged!(parts());
    {
        let mut h = host();
        h.load_static("lifecycle-1", f.entry).expect("first");
    }
    let first = api(&f);
    assert_eq!(f.shutdown_calls.load(Ordering::SeqCst), 1);

    let mut h = host();
    h.load_static("lifecycle-2", f.entry).expect("reload");

    assert_eq!(
        f.entry_calls.load(Ordering::SeqCst),
        4,
        "entry twice per lifecycle"
    );
    assert_eq!(f.init_calls.load(Ordering::SeqCst), 2);
    let second = api(&f);
    assert_ne!(
        first.host_ctx, second.host_ctx,
        "a new host_ctx per lifecycle"
    );
    // SAFETY: the old API struct is process-lifetime; its token must now be dead.
    let code = unsafe {
        (first.log.expect("log"))(
            first.host_ctx,
            UMBRA_LOG_INFO,
            support::forge::leak_str("x"),
        )
    };
    assert_eq!(code, UMBRA_ERR_STATE, "the old host_ctx is refused");
}

#[test]
#[ignore = "WI-8"]
fn reload_with_keep_loaded() {
    let f = forged!(parts());
    let mut options = HostOptions::default();
    options.keep_loaded = true;
    {
        let mut h = PluginHost::new(options.clone());
        h.load_static("kept-1", f.entry).expect("first");
    }
    assert_eq!(
        f.shutdown_calls.load(Ordering::SeqCst),
        1,
        "shutdown still runs with keep_loaded"
    );

    let mut h = PluginHost::new(options);
    h.load_static("kept-2", f.entry).expect("reload");

    assert_eq!(f.entry_calls.load(Ordering::SeqCst), 4);
    assert_eq!(f.init_calls.load(Ordering::SeqCst), 2);
}

#[test]
#[ignore = "WI-8"]
fn keep_loaded_is_sticky_per_image() {
    let f = forged!(parts());
    let mut sticky = HostOptions::default();
    sticky.keep_loaded = true;
    let mut a = PluginHost::new(sticky);
    let mut b = host();
    a.load_static("sticky-a", f.entry).expect("a");
    b.load_static("sticky-b", f.entry)
        .expect("b shares the image");

    drop(a);
    drop(b);

    // The close itself is not observable through an entry function; what is: the lifecycle ended
    // normally (shutdown once) and a later default host can start a new one.
    assert_eq!(f.shutdown_calls.load(Ordering::SeqCst), 1);
    let mut c = host();
    c.load_static("sticky-c", f.entry).expect("new lifecycle");
    assert_eq!(f.init_calls.load(Ordering::SeqCst), 2);
}

#[test]
#[ignore = "WI-8"]
fn load_waits_while_image_is_closing() {
    let f = forged!(parts());
    let mut h = host();
    h.load_static("closing-1", f.entry).expect("first");
    f.gate.arm();
    let closer = thread::spawn(move || drop(h));
    f.gate.wait_in_shutdown();
    let entry = f.entry;
    let second_load = thread::spawn(move || {
        let mut h = host();
        h.load_static("closing-2", entry)
            .map(|_| ())
            .map_err(|e| e.to_string())
    });

    f.gate.open();
    closer.join().expect("closer");
    let outcome = second_load.join().expect("second load");

    assert_eq!(outcome, Ok(()));
    let order = f.order.lock().expect("order").clone();
    assert_eq!(
        order,
        ["init", "shutdown", "init"],
        "the second load saw a closed image and reloaded"
    );
    assert_eq!(f.entry_calls.load(Ordering::SeqCst), 4);
}

#[test]
#[ignore = "WI-8"]
fn log_during_init_and_shutdown_is_routed_and_attributed() {
    let f = forged!(parts());
    let recorder = Recorder::arc();
    let mut options = HostOptions::default();
    options.log = recorder.clone();
    {
        let mut h = PluginHost::new(options);
        h.load_static("log-origin", f.entry).expect("load");
        assert_eq!(
            f.init_log_code.load(Ordering::SeqCst),
            0,
            "init is a call: log works"
        );
    }

    assert_eq!(
        f.shutdown_log_code.load(Ordering::SeqCst),
        0,
        "shutdown is a call: log works"
    );
    let got = recorder.all();
    let ours: Vec<_> = got.iter().filter(|e| e.0 == "test.plugin.forge").collect();
    assert_eq!(ours.len(), 2, "{got:?}");
    assert_eq!(ours[0].2, "init says hi");
    assert_eq!(ours[1].2, "shutdown says bye");
}

#[test]
#[ignore = "WI-8"]
fn log_outside_a_call_and_from_another_thread_is_err_state() {
    let f = forged!(parts());
    let mut h = host();
    h.load_static("idle", f.entry).expect("load");
    let api = api(&f);
    let message = support::forge::leak_str("late");

    // SAFETY: valid API struct and string.
    let idle = unsafe { (api.log.expect("log"))(api.host_ctx, UMBRA_LOG_INFO, message) };
    let log = api.log;
    let ctx = api.host_ctx;
    let from_thread = thread::spawn(move || {
        // SAFETY: as above.
        unsafe { (log.expect("log"))(ctx, UMBRA_LOG_INFO, support::forge::leak_str("late")) }
    })
    .join()
    .expect("thread");

    assert_eq!(idle, UMBRA_ERR_STATE);
    assert_eq!(from_thread, UMBRA_ERR_STATE);
}

#[test]
#[ignore = "WI-8"]
fn log_after_shutdown_is_err_state() {
    let f = forged!(parts());
    {
        let mut h = host();
        h.load_static("closing", f.entry).expect("load");
    }
    let api = api(&f);

    // SAFETY: valid API struct and string.
    let code = unsafe {
        (api.log.expect("log"))(api.host_ctx, UMBRA_LOG_INFO, support::forge::leak_str("x"))
    };

    assert_eq!(code, UMBRA_ERR_STATE);
}
