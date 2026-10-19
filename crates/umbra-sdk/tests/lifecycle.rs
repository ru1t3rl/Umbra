//! WI-7: the plugin lifecycle through the SDK (ADR 0021, RR-M1, R-M1): the entry validates and stores
//! the host API, returns one descriptor from every thread, `init` replaces the stored host and runs the
//! plugin's `init` (errors and panics reported), `shutdown` runs the plugin's `shutdown` (panics logged)
//! and forgets the host, and a new entry starts a new lifecycle. Uses `export_plugin!` with `init` and
//! `shutdown`.
//!
//! One test function: the SDK's stored host API is process-global.
#![expect(unsafe_code, reason = "plays the host side of the C ABI")]
#![expect(
    clippy::cast_possible_truncation,
    clippy::expect_used,
    reason = "test harness casts small ABI sizes; the SDK always fills init and shutdown"
)]

use std::cell::RefCell;
use std::mem::size_of;
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};

use umbra_sdk::abi::{
    UMBRA_ABI_VERSION, UMBRA_ERR_INTERNAL, UMBRA_ERR_INVALID_ARGUMENT, UMBRA_ERR_PANIC,
    UMBRA_LOG_ERROR, UMBRA_LOG_INFO, UMBRA_OK, UmbraCode, UmbraErrorSink, UmbraHostApi,
    UmbraLogLevel, UmbraPluginDescriptor, UmbraStr, UmbraToken,
};
use umbra_sdk::{
    Behavior, BehaviorFactory, BehaviorInstance, EntityView, Error, Intents, Level, Params, Status,
    TickContext, export_plugin, log,
};

struct F;
struct B;
struct I;
impl BehaviorFactory for F {
    const ID: &'static str = "test.behavior.lifecycle";
    const PARAM_SCHEMA: &'static str = r#"{"type":"object"}"#;
    type Behavior = B;
    fn configure(_: &Params<'_>) -> Result<B, Error> {
        Ok(B)
    }
}
impl Behavior for B {
    type Instance = I;
    fn instantiate(&self, _: u32) -> Result<I, Error> {
        Ok(I)
    }
}
impl BehaviorInstance for I {
    fn tick(&mut self, _: &mut TickContext<'_>, _: &EntityView<'_>, _: &mut Intents<'_>) -> Status {
        Status::Running
    }
    fn halt(&mut self) {}
}

/// 0 = succeed, 1 = return an error, 2 = panic.
static MODE: AtomicU8 = AtomicU8::new(0);
static INITS: AtomicUsize = AtomicUsize::new(0);
static SHUTDOWNS: AtomicUsize = AtomicUsize::new(0);

fn plugin_init() -> Result<(), Error> {
    INITS.fetch_add(1, Ordering::SeqCst);
    match MODE.load(Ordering::SeqCst) {
        0 => Ok(()),
        1 => Err(Error::new("init refused")),
        _ => panic!("boom init"),
    }
}

fn plugin_shutdown() {
    SHUTDOWNS.fetch_add(1, Ordering::SeqCst);
    log(Level::Info, "shutdown");
    assert!(MODE.load(Ordering::SeqCst) != 2, "boom shutdown");
}

export_plugin! {
    id: "test.plugin.lifecycle",
    name: "lifecycle",
    version: "1.2.3",
    behaviors: [F],
    init: plugin_init,
    shutdown: plugin_shutdown,
}

thread_local! {
    static LOGS: RefCell<Vec<(UmbraToken, UmbraLogLevel, String)>> = const { RefCell::new(Vec::new()) };
    static ERR: RefCell<Option<String>> = const { RefCell::new(None) };
}

fn text(s: UmbraStr) -> String {
    if s.len == 0 {
        return String::new();
    }
    // SAFETY: the SDK passes valid UTF-8 for the duration of the callback.
    String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(s.ptr, s.len) }).into_owned()
}

unsafe extern "C" fn fake_log(ctx: UmbraToken, level: UmbraLogLevel, msg: UmbraStr) -> UmbraCode {
    LOGS.with(|l| l.borrow_mut().push((ctx, level, text(msg))));
    UMBRA_OK
}
unsafe extern "C" fn err_set(_: UmbraToken, msg: UmbraStr) {
    ERR.with(|e| *e.borrow_mut() = Some(text(msg)));
}

fn host(ctx: UmbraToken) -> &'static UmbraHostApi {
    Box::leak(Box::new(UmbraHostApi {
        abi_version: UMBRA_ABI_VERSION,
        struct_size: size_of::<UmbraHostApi>() as u32,
        host_ctx: ctx,
        log: Some(fake_log),
        rng_next_u64: None,
        rng_next_f64: None,
    }))
}

fn sink() -> UmbraErrorSink {
    UmbraErrorSink {
        abi_version: UMBRA_ABI_VERSION,
        struct_size: size_of::<UmbraErrorSink>() as u32,
        ctx: 9,
        set: Some(err_set),
    }
}

fn logs() -> Vec<(UmbraToken, UmbraLogLevel, String)> {
    LOGS.with(|l| std::mem::take(&mut *l.borrow_mut()))
}

fn take_err() -> Option<String> {
    ERR.with(|e| e.borrow_mut().take())
}

fn init(d: &UmbraPluginDescriptor, h: *const UmbraHostApi) -> UmbraCode {
    let err = sink();
    // SAFETY: the SDK's init with a host API and a valid sink (or a forged one it must refuse).
    unsafe { d.init.expect("the SDK always fills init")(h, &raw const err) }
}

/// The generated entry with `host` (null, forged or live), as an address.
fn entry(host: *const UmbraHostApi) -> usize {
    // SAFETY: `host` is null or a leaked, readable host API (the forged ones are refused after their
    // 8-byte header, which is readable too).
    unsafe { umbra_plugin_entry(host) as usize }
}

fn shutdown(d: &UmbraPluginDescriptor) {
    // SAFETY: the SDK's shutdown.
    unsafe { d.shutdown.expect("the SDK always fills shutdown")() };
}

#[test]
fn lifecycle_stores_replaces_and_forgets_the_host_api() {
    // Before any entry, logging is a no-op.
    log(Level::Info, "too early");
    assert_eq!(logs(), Vec::new());

    // A null or malformed host makes the entry return null and stores nothing.
    assert_eq!(entry(std::ptr::null()), 0);
    let other_major = Box::leak(Box::new(UmbraHostApi {
        abi_version: 1 << 16,
        ..*host(1)
    }));
    assert_eq!(entry(other_major), 0);
    let short = Box::leak(Box::new(UmbraHostApi {
        struct_size: 16,
        ..*host(1)
    }));
    assert_eq!(entry(short), 0);
    log(Level::Info, "still too early");
    assert_eq!(logs(), Vec::new());

    // Lifecycle 1: the entry from several threads returns one descriptor.
    let h1 = host(101);
    let first = entry(h1);
    let others: Vec<usize> = std::thread::scope(|s| {
        let handles: Vec<_> = (0..4).map(|_| s.spawn(|| entry(h1))).collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("thread"))
            .collect()
    });
    assert!(others.iter().all(|p| *p == first), "one descriptor");
    // SAFETY: the SDK's descriptor is a process-lifetime static.
    let d = unsafe { &*(first as *const UmbraPluginDescriptor) };
    assert_eq!(text(d.version), "1.2.3");

    assert_eq!(init(d, h1), UMBRA_OK);
    assert_eq!(INITS.load(Ordering::SeqCst), 1);
    log(Level::Info, "running");
    assert_eq!(logs(), [(101, UMBRA_LOG_INFO, "running".to_owned())]);

    // Shutdown runs the plugin's shutdown, which can still log; afterwards logging is a no-op.
    shutdown(d);
    assert_eq!(SHUTDOWNS.load(Ordering::SeqCst), 1);
    assert_eq!(logs(), [(101, UMBRA_LOG_INFO, "shutdown".to_owned())]);
    log(Level::Info, "after shutdown");
    assert_eq!(logs(), Vec::new());

    // Lifecycle 2: a new entry with a new host API replaces the stored one; same descriptor.
    let h2 = host(202);
    assert_eq!(entry(h2), first);
    log(Level::Info, "second");
    assert_eq!(logs(), [(202, UMBRA_LOG_INFO, "second".to_owned())]);

    // `init` with a malformed host or error sink is refused.
    assert_eq!(init(d, short), UMBRA_ERR_INVALID_ARGUMENT);
    assert!(take_err().is_some_and(|m| m.contains("host API")));
    let mut bad_sink = sink();
    bad_sink.struct_size = 12;
    // SAFETY: the SDK's init with a forged sink it must refuse unread.
    let code = unsafe { d.init.expect("init")(h2, &raw const bad_sink) };
    assert_eq!(code, UMBRA_ERR_INVALID_ARGUMENT);

    // The plugin's init failing or panicking is reported through the sink.
    MODE.store(1, Ordering::SeqCst);
    assert_eq!(init(d, h2), UMBRA_ERR_INTERNAL);
    assert_eq!(take_err().as_deref(), Some("init refused"));
    MODE.store(2, Ordering::SeqCst);
    assert_eq!(init(d, h2), UMBRA_ERR_PANIC);
    assert!(take_err().is_some_and(|m| m.contains("boom init")));

    // `init` stores the host it is given (a reload resets the SDK's global state).
    MODE.store(0, Ordering::SeqCst);
    let h3 = host(303);
    assert_eq!(init(d, h3), UMBRA_OK);
    log(Level::Info, "third");
    assert_eq!(logs(), [(303, UMBRA_LOG_INFO, "third".to_owned())]);

    // A panicking shutdown is swallowed and logged at error level.
    MODE.store(2, Ordering::SeqCst);
    shutdown(d);
    let got = logs();
    assert!(
        got.iter().any(|(_, l, m)| *l == UMBRA_LOG_ERROR
            && m.contains("shutdown")
            && m.contains("boom shutdown")),
        "{got:?}"
    );
    log(Level::Info, "after the second shutdown");
    assert_eq!(logs(), Vec::new());
}
