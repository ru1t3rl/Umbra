//! The host side of the SDK's network adapter tests (M6 WI-12, ADR 0033): a fake host API and error
//! sink, a message sink that records what the SDK pushes (and can refuse), host-written messages and
//! batches, and one helper per vtable call. It exports no plugin, so a test binary that has its own
//! `export_plugin!` (or uses `support`) can include it.
#![warn(clippy::undocumented_unsafe_blocks)]
#![expect(dead_code, reason = "each test binary uses a subset of the harness")]
#![expect(unsafe_code, reason = "the harness plays the host side of the C ABI")]
#![expect(
    clippy::cast_possible_truncation,
    clippy::expect_used,
    reason = "test harness casts small ABI sizes and plays the C side"
)]

use std::cell::{Cell, RefCell};
use std::mem::size_of;

use umbra_sdk::abi::{
    UMBRA_ABI_VERSION, UMBRA_OK, UmbraCapability, UmbraCode, UmbraErrorSink, UmbraHostApi,
    UmbraLogLevel, UmbraNetAdapter, UmbraNetBatch, UmbraNetMessage, UmbraNetSink,
    UmbraNetworkVTable, UmbraPluginDescriptor, UmbraStr, UmbraToken, UmbraVec3,
};

pub const SINK_CTX: UmbraToken = 0x4E_E7;

/// What the SDK pushed to the message sink, copied during the push.
#[derive(Debug, Clone, PartialEq)]
pub struct Pushed {
    pub abi_version: u32,
    pub struct_size: u32,
    pub kind: u32,
    pub dead_reckoning: u32,
    pub key: u64,
    pub other_key: u64,
    pub position: [f64; 3],
    pub velocity: [f64; 3],
    pub orientation: [f64; 3],
    pub entity_type: String,
    pub force: String,
    pub name: String,
    pub weapon: String,
    pub control: u32,
}

thread_local! {
    static ERR: RefCell<Option<String>> = const { RefCell::new(None) };
    static LOGS: RefCell<Vec<(u32, String)>> = const { RefCell::new(Vec::new()) };
    static PUSHED: RefCell<Vec<Pushed>> = const { RefCell::new(Vec::new()) };
    /// From which push (0-based) the sink refuses, and with which code.
    static REFUSE: Cell<Option<(usize, UmbraCode)>> = const { Cell::new(None) };
    /// Pushes seen, refused ones included.
    static PUSH_CALLS: Cell<usize> = const { Cell::new(0) };
}

pub fn reset() {
    ERR.with(|e| e.borrow_mut().take());
    LOGS.with(|l| l.borrow_mut().clear());
    PUSHED.with(|p| p.borrow_mut().clear());
    REFUSE.with(|r| r.set(None));
    PUSH_CALLS.with(|c| c.set(0));
}

pub fn take_err() -> Option<String> {
    ERR.with(|e| e.borrow_mut().take())
}

pub fn logs() -> Vec<(u32, String)> {
    LOGS.with(|l| l.borrow().clone())
}

pub fn pushed() -> Vec<Pushed> {
    PUSHED.with(|p| p.borrow().clone())
}

pub fn push_calls() -> usize {
    PUSH_CALLS.with(Cell::get)
}

/// The sink refuses every push from the `from`-th (0-based) on with `code`.
pub fn refuse_from(from: usize, code: UmbraCode) {
    REFUSE.with(|r| r.set(Some((from, code))));
}

pub fn text(s: UmbraStr) -> String {
    if s.len == 0 {
        return String::new();
    }
    // SAFETY: the SDK passes valid UTF-8 for the duration of the callback.
    let bytes = unsafe { std::slice::from_raw_parts(s.ptr, s.len) };
    String::from_utf8_lossy(bytes).into_owned()
}

pub fn s(t: &str) -> UmbraStr {
    UmbraStr {
        ptr: t.as_ptr(),
        len: t.len(),
    }
}

unsafe extern "C" fn log(_ctx: UmbraToken, level: UmbraLogLevel, msg: UmbraStr) -> UmbraCode {
    LOGS.with(|l| l.borrow_mut().push((level, text(msg))));
    UMBRA_OK
}

pub fn host_api() -> UmbraHostApi {
    UmbraHostApi {
        abi_version: UMBRA_ABI_VERSION,
        struct_size: size_of::<UmbraHostApi>() as u32,
        host_ctx: 0xB0_57,
        log: Some(log),
        rng_next_u64: None,
        rng_next_f64: None,
    }
}

unsafe extern "C" fn err_set(_ctx: UmbraToken, message: UmbraStr) {
    ERR.with(|e| {
        let mut e = e.borrow_mut();
        if e.is_none() {
            *e = Some(text(message));
        }
    });
}

pub fn err_sink() -> UmbraErrorSink {
    UmbraErrorSink {
        abi_version: UMBRA_ABI_VERSION,
        struct_size: size_of::<UmbraErrorSink>() as u32,
        ctx: 0xE_22,
        set: Some(err_set),
    }
}

fn v3(v: UmbraVec3) -> [f64; 3] {
    [v.x, v.y, v.z]
}

unsafe extern "C" fn push(ctx: UmbraToken, m: *const UmbraNetMessage) -> UmbraCode {
    let n = PUSH_CALLS.with(|c| {
        let n = c.get();
        c.set(n + 1);
        n
    });
    assert_eq!(ctx, SINK_CTX, "the SDK passes the sink's own token back");
    if let Some((from, code)) = REFUSE.with(Cell::get)
        && n >= from
    {
        return code;
    }
    // SAFETY: the SDK passes a valid message for the duration of the call.
    let m = unsafe { &*m };
    PUSHED.with(|p| {
        p.borrow_mut().push(Pushed {
            abi_version: m.abi_version,
            struct_size: m.struct_size,
            kind: m.kind,
            dead_reckoning: m.dead_reckoning,
            key: m.key,
            other_key: m.other_key,
            position: v3(m.position),
            velocity: v3(m.velocity),
            orientation: v3(m.orientation),
            entity_type: text(m.entity_type),
            force: text(m.force),
            name: text(m.name),
            weapon: text(m.weapon),
            control: m.control,
        });
    });
    UMBRA_OK
}

pub fn net_sink() -> UmbraNetSink {
    UmbraNetSink {
        abi_version: UMBRA_ABI_VERSION,
        struct_size: size_of::<UmbraNetSink>() as u32,
        ctx: SINK_CTX,
        push: Some(push),
    }
}

// ---------------------------------------------------------------- host-written messages

pub fn vec3(x: f64, y: f64, z: f64) -> UmbraVec3 {
    UmbraVec3 { x, y, z }
}

/// An all-zero host-written message of `kind` (ABI 0.1).
pub fn blank(kind: u32) -> UmbraNetMessage {
    UmbraNetMessage {
        abi_version: UMBRA_ABI_VERSION,
        struct_size: size_of::<UmbraNetMessage>() as u32,
        kind,
        dead_reckoning: 0,
        key: 0,
        other_key: 0,
        position: vec3(0.0, 0.0, 0.0),
        velocity: vec3(0.0, 0.0, 0.0),
        acceleration: vec3(0.0, 0.0, 0.0),
        angular_velocity: vec3(0.0, 0.0, 0.0),
        orientation: vec3(0.0, 0.0, 0.0),
        entity_type: s(""),
        force: s(""),
        name: s(""),
        weapon: s(""),
        control: 0,
        reserved_0: 0,
    }
}

pub fn batch(messages: &[UmbraNetMessage], tick: u64) -> UmbraNetBatch {
    UmbraNetBatch {
        abi_version: UMBRA_ABI_VERSION,
        struct_size: size_of::<UmbraNetBatch>() as u32,
        tick,
        rate_hz: 30,
        reserved_0: 0,
        messages: if messages.is_empty() {
            std::ptr::null()
        } else {
            messages.as_ptr()
        },
        count: messages.len(),
    }
}

// ---------------------------------------------------------------- the descriptor

pub fn capabilities(d: &UmbraPluginDescriptor) -> &[UmbraCapability] {
    // SAFETY: the SDK's descriptor holds `capability_count` capabilities while it lives.
    unsafe { std::slice::from_raw_parts(d.capabilities, d.capability_count) }
}

/// The network vtable of `c` (a network adapter capability).
pub fn net_vtable(c: &UmbraCapability) -> &UmbraNetworkVTable {
    // SAFETY: the caller passes a network adapter capability, whose vtable is an
    // `UmbraNetworkVTable` living as long as the descriptor.
    unsafe { &*c.vtable.cast::<UmbraNetworkVTable>() }
}

// ---------------------------------------------------------------- vtable calls

pub type Handle = *mut UmbraNetAdapter;

pub fn configure_raw(vt: &UmbraNetworkVTable, json: &str) -> (UmbraCode, Handle, Option<String>) {
    let err = err_sink();
    let mut out: Handle = std::ptr::null_mut();
    // SAFETY: valid params, out and sink for the call.
    let code = unsafe { vt.configure.expect("configure")(s(json), &raw mut out, &raw const err) };
    (code, out, take_err())
}

/// `configure` with the ADR 0030 wrapper around `params` (JSON text).
pub fn configure(vt: &UmbraNetworkVTable, params: &str) -> (UmbraCode, Handle, Option<String>) {
    configure_raw(
        vt,
        &format!(r#"{{"name":"net","params":{params},"templates":{{}}}}"#),
    )
}

pub fn key_space(vt: &UmbraNetworkVTable, h: Handle) -> (UmbraCode, u64, u64) {
    let (mut base, mut count) = (0, 0);
    // SAFETY: `h` is the handle under test (live or deliberately foreign); valid out pointers.
    let code = unsafe { vt.key_space.expect("key_space")(h, &raw mut base, &raw mut count) };
    (code, base, count)
}

pub fn start(vt: &UmbraNetworkVTable, h: Handle, rate_hz: u32) -> (UmbraCode, Option<String>) {
    let err = err_sink();
    // SAFETY: as in `key_space`; a valid error sink.
    let code = unsafe { vt.start.expect("start")(h, 0, rate_hz, &raw const err) };
    (code, take_err())
}

pub fn publish_batch(
    vt: &UmbraNetworkVTable,
    h: Handle,
    b: *const UmbraNetBatch,
) -> (UmbraCode, Option<String>) {
    let err = err_sink();
    // SAFETY: `b` is null or a live batch over live messages; a valid error sink.
    let code = unsafe { vt.publish.expect("publish")(h, b, &raw const err) };
    (code, take_err())
}

pub fn publish(
    vt: &UmbraNetworkVTable,
    h: Handle,
    messages: &[UmbraNetMessage],
) -> (UmbraCode, Option<String>) {
    let b = batch(messages, 7);
    publish_batch(vt, h, &raw const b)
}

pub fn poll_with(
    vt: &UmbraNetworkVTable,
    h: Handle,
    max: usize,
    sink: *const UmbraNetSink,
) -> (UmbraCode, Option<String>) {
    let err = err_sink();
    // SAFETY: `sink` is null or a live sink; a valid error sink.
    let code = unsafe { vt.poll.expect("poll")(h, 8, max, sink, &raw const err) };
    (code, take_err())
}

pub fn poll(vt: &UmbraNetworkVTable, h: Handle, max: usize) -> (UmbraCode, Option<String>) {
    let sink = net_sink();
    poll_with(vt, h, max, &raw const sink)
}

pub fn stop(vt: &UmbraNetworkVTable, h: Handle) -> UmbraCode {
    // SAFETY: as in `key_space`.
    unsafe { vt.stop.expect("stop")(h) }
}

pub fn counters(vt: &UmbraNetworkVTable, h: Handle, n: usize) -> (UmbraCode, Vec<u64>) {
    let mut out = vec![0_u64; n];
    // SAFETY: as in `key_space`; `out` has `n` writable entries.
    let code = unsafe { vt.counters.expect("counters")(h, out.as_mut_ptr(), n) };
    (code, out)
}

pub fn destroy(vt: &UmbraNetworkVTable, h: Handle) {
    // SAFETY: `h` is a live handle from `configure`, not used again.
    unsafe { vt.destroy.expect("destroy")(h) };
}
