//! A hand-written, raw-ABI network adapter plugin with selectable misbehavior (M6 WI-11, AT-16, AT-21):
//! every fault row of the network bridge is reachable, including those a plugin built with the safe SDK
//! cannot produce (an unknown kind, a non-UTF-8 string, a stale sink token).
//!
//! Capability `test.net.raw` in a descriptor declaring ABI 0.1. `configure` gets the ADR 0030 wrapper
//! `{"name", "params", "templates"}`; the plugin reads `params`: `mode` (below), `order` (the address of
//! the test's `forged!` call-order log, which also receives `init` and `shutdown`), `host` (the address
//! of the `UmbraHostApi`, for the I/O thread), `base` and `count` (its key space, default 1000 and 100),
//! `tag` (optional: prefixes every event of this adapter with `"<tag> "`, so several adapters sharing
//! one image's log stay apart; a tagged `poll` also records `"<tag> sink ctx=<token>"`).
//! Every entry pushes one event naming the call and what it saw into that log, so a test sees the
//! complete call order of one image lifecycle. [`parts_for`] makes a plugin with any ID and several
//! network capabilities over the same vtable, and [`NetImages`] loads several such images into one
//! host (multi-plugin rows, ASM-410).
//!
//! Modes: `echo` (default: `publish` queues copies of the messages, `poll` hands them back, at most the
//! `max` it was given), `configure_error`, `configure_panic`, `configure_null`, `key_error`,
//! `start_error`, `publish_panic`, `poll_panic` (pushes one valid message, then fails), `poll_error`,
//! `bad_kind`, `bad_utf8`, `flood` (`max + 1` pushes), `stale` (the second `poll` pushes through the sink
//! of the first), `unused_fields` (an `ENTITY_REMOVED` with garbage in every field it does not use),
//! `counters_error`, `thread` (`start` spawns an I/O thread that calls the host `log`, then waits for
//! `stop`, which joins it), `lend` (echo, and each `poll` also stores its sink in the process-wide
//! [`LENT`] slot) and `borrow` (each `poll` pushes one message through the lent sink of another
//! adapter instead of its own); one test uses the `lend`/`borrow` pair.
//!
//! The callbacks never panic (a panic cannot cross `extern "C"`): a missing value falls back to a
//! default, and the tests assert the events.
#![warn(clippy::undocumented_unsafe_blocks)]
#![expect(unsafe_code, reason = "the harness plays the plugin side of the C ABI")]
#![expect(
    clippy::cast_possible_truncation,
    clippy::expect_used,
    clippy::many_single_char_names,
    reason = "test harness: small ABI sizes, known-valid values, C-style callback names"
)]

use std::collections::VecDeque;
use std::ffi::c_void;
use std::mem::size_of;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;

use umbra_plugin_api::{
    UMBRA_ABI_VERSION_MAJOR, UMBRA_CAPABILITY_NETWORK_ADAPTER, UMBRA_ERR_INTERNAL, UMBRA_ERR_PANIC,
    UMBRA_ERR_PARAMS, UMBRA_ERR_STATE, UMBRA_NET_ENTITY_REMOVED, UMBRA_NET_ENTITY_STATE, UMBRA_OK,
    UmbraCode, UmbraErrorSink, UmbraHostApi, UmbraNetAdapter, UmbraNetBatch, UmbraNetMessage,
    UmbraNetSink, UmbraNetworkVTable, UmbraStr, UmbraToken, UmbraVec3,
};

use crate::support::forge::{Parts, leak_str, log_through, parts as behavior_parts};

pub const CAPABILITY: &str = "test.net.raw";
const SCHEMA: &str = r#"{"type":"object","description":"raw test network adapter"}"#;

/// The call-order log of one `forged!` site.
pub type Order = Mutex<Vec<&'static str>>;

/// `major << 16 | minor`.
pub const fn abi(minor: u32) -> u32 {
    (UMBRA_ABI_VERSION_MAJOR << 16) | minor
}

pub type Push = unsafe extern "C" fn(UmbraToken, *const UmbraNetMessage) -> UmbraCode;

/// A message copied out of a `publish` batch, with owned strings.
struct Owned {
    raw: UmbraNetMessage,
    strings: [String; 4],
}

struct Adapter {
    mode: String,
    tag: String,
    order: usize,
    host: usize,
    base: u64,
    count: u64,
    queue: VecDeque<Owned>,
    stash: Option<(UmbraToken, Push)>,
    polls: u64,
    thread: Option<JoinHandle<()>>,
    quit: Arc<AtomicBool>,
}

fn record(order: usize, event: String) {
    if order == 0 {
        return;
    }
    // SAFETY: `order` is the address of a `forged!` static (`'static`), passed by the test.
    let log = unsafe { &*(order as *const Order) };
    log.lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(Box::leak(event.into_boxed_str()));
}

/// The sink a `lend` adapter's last `poll` was given (`borrow` pushes through it).
pub static LENT: Mutex<Option<(UmbraToken, Push)>> = Mutex::new(None);

/// `event` prefixed with `"<tag> "` when `tag` is not empty.
fn tagged(tag: &str, event: String) -> String {
    if tag.is_empty() {
        event
    } else {
        format!("{tag} {event}")
    }
}

impl Adapter {
    fn event(&self, event: String) {
        record(self.order, tagged(&self.tag, event));
    }
}

fn str_of(t: &str) -> UmbraStr {
    UmbraStr {
        ptr: t.as_ptr(),
        len: t.len(),
    }
}

fn text(s: UmbraStr) -> String {
    if s.len == 0 || s.ptr.is_null() {
        return String::new();
    }
    // SAFETY: the host passes valid bytes for the call.
    String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(s.ptr, s.len) }).into_owned()
}

unsafe fn say(err: *const UmbraErrorSink, message: &str) {
    if err.is_null() {
        return;
    }
    // SAFETY: the host passes a valid sink for the call.
    let e = unsafe { &*err };
    if let Some(set) = e.set {
        // SAFETY: valid ctx and string for the call.
        unsafe { set(e.ctx, str_of(message)) };
    }
}

/// # Safety
/// `a` is a handle made by `configure`.
unsafe fn adapter<'a>(a: *mut UmbraNetAdapter) -> &'a mut Adapter {
    // SAFETY: by the caller; calls are serialised by the host.
    unsafe { &mut *a.cast::<Adapter>() }
}

fn zero() -> UmbraVec3 {
    UmbraVec3 {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    }
}

/// A valid `ENTITY_REMOVED` message of the plugin's minor (1).
fn removed(key: u64) -> UmbraNetMessage {
    UmbraNetMessage {
        abi_version: abi(1),
        struct_size: size_of::<UmbraNetMessage>() as u32,
        kind: UMBRA_NET_ENTITY_REMOVED,
        dead_reckoning: 0,
        key,
        other_key: 0,
        position: zero(),
        velocity: zero(),
        acceleration: zero(),
        angular_velocity: zero(),
        orientation: zero(),
        entity_type: str_of(""),
        force: str_of(""),
        name: str_of(""),
        weapon: str_of(""),
        control: 0,
        reserved_0: 0,
    }
}

static NOT_UTF8: [u8; 3] = [0xFF, 0xFE, 0xFD];

unsafe extern "C" fn configure(
    params: UmbraStr,
    out: *mut *mut UmbraNetAdapter,
    err: *const UmbraErrorSink,
) -> UmbraCode {
    let input = text(params);
    let json: serde_json::Value = serde_json::from_str(&input).unwrap_or_default();
    let p = &json["params"];
    let num = |k: &str, d: u64| p[k].as_u64().unwrap_or(d);
    let order = usize::try_from(num("order", 0)).unwrap_or(0);
    let tag = p["tag"].as_str().unwrap_or("").to_owned();
    record(order, tagged(&tag, format!("configure {input}")));
    let mode = p["mode"].as_str().unwrap_or("echo").to_owned();
    // SAFETY: valid out pointer and sink (host contract).
    unsafe {
        match mode.as_str() {
            "configure_error" => {
                say(err, "params.port: 70000 is out of range");
                return UMBRA_ERR_PARAMS;
            }
            "configure_panic" => {
                say(err, "panicked: configure exploded");
                return UMBRA_ERR_PANIC;
            }
            "configure_null" => {
                *out = std::ptr::null_mut();
                return UMBRA_OK;
            }
            _ => {}
        }
        let a = Adapter {
            mode,
            tag,
            order,
            host: usize::try_from(num("host", 0)).unwrap_or(0),
            base: num("base", 1000),
            count: num("count", 100),
            queue: VecDeque::new(),
            stash: None,
            polls: 0,
            thread: None,
            quit: Arc::new(AtomicBool::new(false)),
        };
        *out = Box::into_raw(Box::new(a)).cast();
    }
    UMBRA_OK
}

fn join(a: &mut Adapter) {
    a.quit.store(true, Ordering::SeqCst);
    if let Some(t) = a.thread.take() {
        t.thread().unpark();
        let joined = t.join().is_ok();
        a.event(format!("joined ok={joined}"));
    }
}

unsafe extern "C" fn destroy(a: *mut UmbraNetAdapter) {
    if a.is_null() {
        return;
    }
    // SAFETY: created by `configure`, destroyed once.
    let mut boxed = unsafe { Box::from_raw(a.cast::<Adapter>()) };
    join(&mut boxed);
    boxed.event("destroy".to_owned());
}

unsafe extern "C" fn key_space(
    a: *mut UmbraNetAdapter,
    base: *mut u64,
    count: *mut u64,
) -> UmbraCode {
    // SAFETY: a live handle and valid out pointers (host contract).
    unsafe {
        let a = adapter(a);
        a.event("key_space".to_owned());
        if a.mode == "key_error" {
            return UMBRA_ERR_INTERNAL;
        }
        *base = a.base;
        *count = a.count;
    }
    UMBRA_OK
}

unsafe extern "C" fn start(
    a: *mut UmbraNetAdapter,
    tick: u64,
    rate_hz: u32,
    err: *const UmbraErrorSink,
) -> UmbraCode {
    // SAFETY: a live handle (host contract).
    let a = unsafe { adapter(a) };
    a.event(format!("start tick={tick} rate={rate_hz}"));
    if a.mode == "start_error" {
        // SAFETY: valid sink for the call.
        unsafe { say(err, "bind failed: address in use") };
        return UMBRA_ERR_INTERNAL;
    }
    if a.mode == "thread" {
        let (order, host, quit) = (a.order, a.host, Arc::clone(&a.quit));
        let (logged, wait) = std::sync::mpsc::channel();
        a.thread = Some(std::thread::spawn(move || {
            let code = log_through(host as *const UmbraHostApi, "from the I/O thread");
            let what = if code == UMBRA_ERR_STATE {
                "STATE"
            } else {
                "other"
            };
            record(order, format!("thread log={what}"));
            let _ = logged.send(());
            while !quit.load(Ordering::SeqCst) {
                std::thread::park();
            }
            record(order, "thread done".to_owned());
        }));
        // The thread has logged before `start` returns, so the call order is fixed.
        let _ = wait.recv();
    }
    UMBRA_OK
}

unsafe extern "C" fn publish(
    a: *mut UmbraNetAdapter,
    batch: *const UmbraNetBatch,
    err: *const UmbraErrorSink,
) -> UmbraCode {
    // SAFETY: a live handle and a valid batch for the call (host contract).
    unsafe {
        let a = adapter(a);
        let b = &*batch;
        a.event(format!(
            "publish v={:#x} size={} tick={} rate={} count={} null={}",
            b.abi_version,
            b.struct_size,
            b.tick,
            b.rate_hz,
            b.count,
            b.messages.is_null()
        ));
        if a.mode == "publish_panic" {
            say(err, "panicked: publish exploded");
            return UMBRA_ERR_PANIC;
        }
        if let ("stale", Some((ctx, push))) = (a.mode.as_str(), a.stash) {
            let m = removed(7);
            let code = push(ctx, &raw const m);
            a.event(format!("stale push from publish={code}"));
        }
        if b.count > 0 {
            for m in std::slice::from_raw_parts(b.messages, b.count) {
                a.event(format!(
                    "message v={:#x} size={}",
                    m.abi_version, m.struct_size
                ));
                a.queue.push_back(Owned {
                    raw: UmbraNetMessage {
                        entity_type: str_of(""),
                        force: str_of(""),
                        name: str_of(""),
                        weapon: str_of(""),
                        ..*m
                    },
                    strings: [
                        text(m.entity_type),
                        text(m.force),
                        text(m.name),
                        text(m.weapon),
                    ],
                });
            }
        }
    }
    UMBRA_OK
}

/// Records a tagged adapter's sink token, then the `lend` and `borrow` modes of `poll`: `lend` stores this call's sink in [`LENT`] (and the poll
/// goes on as `echo`); `borrow` pushes one message through the lent sink and ends the poll (`true`).
///
/// # Safety
/// `push` and `ctx` are the current call's host sink.
unsafe fn lend_or_borrow(a: &Adapter, ctx: UmbraToken, push: Push) -> bool {
    if !a.tag.is_empty() {
        a.event(format!("sink ctx={ctx}"));
    }
    let mut lent = LENT.lock().unwrap_or_else(PoisonError::into_inner);
    match a.mode.as_str() {
        "borrow" => {
            if let Some((foreign, f)) = *lent {
                // SAFETY: a host sink callback, called during this adapter's `poll` (the host
                // refuses the foreign token without reading the message twice).
                let code = unsafe { f(foreign, &removed(5)) };
                a.event(format!("foreign push={code}"));
            }
            true
        }
        "lend" => {
            *lent = Some((ctx, push));
            false
        }
        _ => false,
    }
}

unsafe extern "C" fn poll(
    a: *mut UmbraNetAdapter,
    tick: u64,
    max: usize,
    sink: *const UmbraNetSink,
    err: *const UmbraErrorSink,
) -> UmbraCode {
    // SAFETY: a live handle and a valid sink for the call (host contract).
    unsafe {
        let a = adapter(a);
        let host_sink = &*sink;
        a.polls += 1;
        a.event(format!(
            "poll tick={tick} max={max} sink v={:#x} size={}",
            host_sink.abi_version, host_sink.struct_size
        ));
        let Some(push) = host_sink.push else {
            return UMBRA_ERR_INTERNAL;
        };
        let ctx = host_sink.ctx;
        if lend_or_borrow(a, ctx, push) {
            return UMBRA_OK;
        }
        let send = |m: &UmbraNetMessage| push(ctx, m);
        match a.mode.as_str() {
            "poll_panic" => {
                let code = send(&removed(5));
                a.event(format!("pushed={code}"));
                say(err, "panicked: poll exploded");
                return UMBRA_ERR_PANIC;
            }
            "poll_error" => {
                say(err, "socket closed");
                return UMBRA_ERR_INTERNAL;
            }
            "bad_kind" => {
                let mut m = removed(5);
                m.kind = 99;
                let code = send(&m);
                a.event(format!("pushed={code}"));
            }
            "bad_utf8" => {
                let mut m = removed(5);
                m.kind = UMBRA_NET_ENTITY_STATE;
                m.dead_reckoning = 2;
                m.name = UmbraStr {
                    ptr: NOT_UTF8.as_ptr(),
                    len: NOT_UTF8.len(),
                };
                let code = send(&m);
                a.event(format!("pushed={code}"));
            }
            "flood" => {
                for k in 0..=max as u64 {
                    let code = send(&removed(k + 1));
                    if code != UMBRA_OK {
                        a.event(format!("push {k} refused={code}"));
                    }
                }
            }
            "stale" => {
                if let Some((ctx, old)) = a.stash {
                    let code = old(ctx, &removed(5));
                    a.event(format!("stale push={code}"));
                } else {
                    a.stash = Some((ctx, push));
                }
            }
            "unused_fields" => {
                let mut m = removed(5);
                m.dead_reckoning = 77;
                m.control = 77;
                m.other_key = 9;
                m.velocity.x = f64::NAN;
                m.name = UmbraStr {
                    ptr: NOT_UTF8.as_ptr(),
                    len: NOT_UTF8.len(),
                };
                m.weapon = m.name;
                let code = send(&m);
                a.event(format!("pushed={code}"));
            }
            _ => {
                let mut pushed = 0;
                while pushed < max {
                    let Some(o) = a.queue.pop_front() else {
                        break;
                    };
                    let [t, f, nm, w] = &o.strings;
                    let m = UmbraNetMessage {
                        entity_type: str_of(t),
                        force: str_of(f),
                        name: str_of(nm),
                        weapon: str_of(w),
                        ..o.raw
                    };
                    let code = send(&m);
                    if code != UMBRA_OK {
                        a.event(format!("echo refused={code}"));
                    }
                    pushed += 1;
                }
            }
        }
    }
    UMBRA_OK
}

unsafe extern "C" fn stop(a: *mut UmbraNetAdapter) -> UmbraCode {
    // SAFETY: a live handle (host contract).
    let a = unsafe { adapter(a) };
    a.event("stop".to_owned());
    join(a);
    UMBRA_OK
}

unsafe extern "C" fn counters(a: *mut UmbraNetAdapter, out: *mut u64, n: usize) -> UmbraCode {
    // SAFETY: a live handle and `n` writable counters (host contract).
    unsafe {
        let a = adapter(a);
        let values = std::slice::from_raw_parts_mut(out, n);
        let zeroed = values.iter().all(|&v| v == 0);
        a.event(format!("counters n={n} zeroed={zeroed}"));
        if a.mode == "counters_error" {
            return UMBRA_ERR_INTERNAL;
        }
        for (i, v) in values.iter_mut().enumerate() {
            *v = a.polls * 10 + i as u64 + 1;
        }
    }
    UMBRA_OK
}

/// A complete network vtable of minor 1, `counters` included.
pub fn vtable() -> UmbraNetworkVTable {
    UmbraNetworkVTable {
        abi_version: abi(1),
        struct_size: size_of::<UmbraNetworkVTable>() as u32,
        configure: Some(configure),
        destroy: Some(destroy),
        key_space: Some(key_space),
        start: Some(start),
        publish: Some(publish),
        poll: Some(poll),
        stop: Some(stop),
        counters: Some(counters),
    }
}

/// A plugin declaring ABI 0.1 whose one capability is the network adapter `test.net.raw`, with
/// `edit` applied to its vtable.
pub fn parts_with(edit: impl FnOnce(&mut UmbraNetworkVTable)) -> Parts {
    let mut vt = vtable();
    edit(&mut vt);
    network_parts(None, &[CAPABILITY], vt)
}

/// A plugin `plugin` declaring ABI 0.1 whose capabilities are the network adapters `capabilities`,
/// all over the complete vtable.
pub fn parts_for(plugin: &str, capabilities: &[&str]) -> Parts {
    network_parts(Some(plugin), capabilities, vtable())
}

fn network_parts(plugin: Option<&str>, capabilities: &[&str], vt: UmbraNetworkVTable) -> Parts {
    let mut p = behavior_parts();
    p.desc.abi_version = abi(1);
    if let Some(plugin) = plugin {
        p.desc.id = leak_str(plugin);
    }
    for (i, id) in capabilities.iter().enumerate() {
        if i == 0 {
            p.set_capability(0, id);
        } else {
            p.add_capability(id);
        }
    }
    let vt: &'static UmbraNetworkVTable = Box::leak(Box::new(vt));
    for c in &mut p.caps {
        c.kind = UMBRA_CAPABILITY_NETWORK_ADAPTER;
        c.abi_version = abi(1);
        c.param_schema = leak_str(SCHEMA);
        c.vtable = std::ptr::from_ref(vt).cast::<c_void>();
    }
    p
}

/// [`parts_with`] without an edit.
pub fn parts() -> Parts {
    parts_with(|_| {})
}

/// The raw network plugin loaded into a host and registered, with the host's log recorder.
pub struct NetEnv {
    pub host: umbra_plugin_host::PluginHost,
    pub registry: umbra_core::Registry,
    pub f: crate::support::forge::Forged,
    pub log: Arc<crate::support::Recorder>,
}

/// A [`NetEnv`] over the raw network plugin, published by the caller's own `forged!` (one call site
/// per test, see `forge.rs`). `net_env!()` is a run-mode host with the `abort` policy;
/// `net_env!(mode = .., policy = ..)` chooses both.
macro_rules! net_env {
    () => {
        net_env!(
            mode = umbra_plugin_host::HostMode::Run,
            policy = umbra_core::PluginFaultPolicy::Abort
        )
    };
    (mode = $mode:expr, policy = $policy:expr) => {{
        let f = forged!($crate::net_support::parts());
        $crate::net_support::NetEnv::load(f, $mode, $policy)
    }};
}

/// A fresh configuration of the raw network plugin in mode `$mode` (one `forged!` per expansion, so
/// `network_adapter_contract_tests!` gets one image per generated test). The host is dropped; the
/// configuration keeps the image.
macro_rules! net_config {
    ($mode:expr) => {{
        let env = net_env!();
        env.config($mode)
    }};
}

/// The adapter name the tests pass in the configure input.
pub const NAME: &str = "raw_net";

impl NetEnv {
    /// Loads the forged raw network plugin into a new host (log recorded) and registers it.
    pub fn load(
        f: crate::support::forge::Forged,
        mode: umbra_plugin_host::HostMode,
        policy: umbra_core::PluginFaultPolicy,
    ) -> Self {
        let log = crate::support::Recorder::arc();
        let mut options = umbra_plugin_host::HostOptions::default();
        options.log = log.clone();
        options.mode = mode;
        let mut host = umbra_plugin_host::PluginHost::new(options);
        host.load_static("net-origin", f.entry)
            .expect("load the raw network plugin");
        host.set_fault_policy(policy);
        let mut registry = umbra_core::Registry::new();
        host.register(&mut registry)
            .expect("register the raw network plugin");
        Self {
            host,
            registry,
            f,
            log,
        }
    }

    /// The adapter's params in `mode` with the extra JSON members `extra` (`""` or `,"k":v...`).
    pub fn params(&self, mode: &str, extra: &str) -> umbra_core::ParamValue {
        params_of(&self.f, mode, extra)
    }

    /// Builds a configuration through the registered factory inside the host's build scope.
    pub fn try_build(
        &self,
        mode: &str,
        extra: &str,
    ) -> Result<Arc<dyn umbra_core::net::NetworkAdapterConfig>, umbra_core::BuildError> {
        let input = umbra_core::net::configure_input(NAME, &self.params(mode, extra), []);
        let id = umbra_core::ComponentId::new(CAPABILITY).expect("valid id");
        let factory = self
            .registry
            .network_adapter(&id)
            .expect("the network adapter is registered");
        let _scope = self.host.build_scope();
        factory.build(&input)
    }

    /// [`NetEnv::try_build`] that must succeed.
    pub fn config(&self, mode: &str) -> Arc<dyn umbra_core::net::NetworkAdapterConfig> {
        self.try_build(mode, "")
            .expect("build the raw network adapter")
    }

    /// The call-order log so far (`init`, the plugin's calls, `shutdown`).
    pub fn events(&self) -> Vec<&'static str> {
        self.f
            .order
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// How many events start with `prefix`.
    pub fn count(&self, prefix: &str) -> usize {
        self.events()
            .iter()
            .filter(|e| e.starts_with(prefix))
            .count()
    }
}

/// The params of an adapter of image `f` in `mode` with the extra JSON members `extra` (`""` or
/// `,"k":v...`): the image's call-order log and host API addresses included.
pub fn params_of(
    f: &crate::support::forge::Forged,
    mode: &str,
    extra: &str,
) -> umbra_core::ParamValue {
    let order = std::ptr::from_ref(f.order) as usize;
    let host = f.host.load(Ordering::SeqCst);
    serde_json::from_str(&format!(
        r#"{{"mode":"{mode}","order":{order},"host":{host}{extra}}}"#
    ))
    .expect("params JSON")
}

/// Several forged network plugin images loaded into one run-mode host and registered (ASM-410).
pub struct NetImages {
    pub host: umbra_plugin_host::PluginHost,
    pub registry: umbra_core::Registry,
    /// The images in load order (`net-origin-<index>`).
    pub images: Vec<crate::support::forge::Forged>,
}

impl NetImages {
    /// Loads every image of `images` (distinct plugin IDs) into a new run-mode host with `policy`.
    pub fn load(
        images: Vec<crate::support::forge::Forged>,
        policy: umbra_core::PluginFaultPolicy,
    ) -> Self {
        let mut host =
            umbra_plugin_host::PluginHost::new(umbra_plugin_host::HostOptions::default());
        for (i, f) in images.iter().enumerate() {
            host.load_static(&format!("net-origin-{i}"), f.entry)
                .expect("load a network plugin image");
        }
        host.set_fault_policy(policy);
        let mut registry = umbra_core::Registry::new();
        host.register(&mut registry)
            .expect("register the network plugin images");
        Self {
            host,
            registry,
            images,
        }
    }

    /// Builds capability `capability` of image `image` under the scenario name `name`, in `mode`,
    /// tagged `tag`, with key space `base` (count 100), inside the host's build scope.
    pub fn build(
        &self,
        image: usize,
        capability: &str,
        name: &str,
        mode: &str,
        tag: &str,
        base: u64,
    ) -> Arc<dyn umbra_core::net::NetworkAdapterConfig> {
        let extra = format!(r#","tag":"{tag}","base":{base}"#);
        let params = params_of(&self.images[image], mode, &extra);
        let input = umbra_core::net::configure_input(name, &params, []);
        let id = umbra_core::ComponentId::new(capability).expect("valid id");
        let factory = self
            .registry
            .network_adapter(&id)
            .expect("the network adapter is registered");
        let _scope = self.host.build_scope();
        factory.build(&input).expect("build a network adapter")
    }

    /// The events of image `image` that start with `"<tag> "`, without the tag.
    pub fn events(&self, image: usize, tag: &str) -> Vec<&'static str> {
        events_tagged(self.images[image].order, tag)
    }
}

/// The events of the call-order log `order` that start with `"<tag> "`, without the tag.
pub fn events_tagged(order: &Order, tag: &str) -> Vec<&'static str> {
    let prefix = format!("{tag} ");
    order
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .filter_map(|e| e.strip_prefix(prefix.as_str()))
        .collect()
}
