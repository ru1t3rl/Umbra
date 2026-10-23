//! The host side of the SDK's ABI 0.2 call-level tests (M7 WI-10, AT-17, ADR 0050): a fake host API
//! (log and RNG), an error sink, the per-call services (terrain, blackboard) and the contact and intent
//! sinks, each recording what the SDK sends (token, arguments, the header it wrote) and able to refuse
//! with a code; host-written call records; one helper per vtable call. It exports no plugin, so the
//! test binary has its own `export_plugin!`.
#![warn(clippy::undocumented_unsafe_blocks)]
#![expect(unsafe_code, reason = "the harness plays the host side of the C ABI")]
#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::expect_used,
    reason = "test harness casts small ABI sizes and plays the C side"
)]

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::mem::size_of;

use umbra_sdk::abi::{
    UMBRA_ABI_VERSION, UMBRA_OK, UMBRA_VALUE_BOOL, UMBRA_VALUE_ENTITY, UMBRA_VALUE_INTEGER,
    UMBRA_VALUE_NUMBER, UMBRA_VALUE_POSITION, UMBRA_VALUE_STRING, UmbraAction, UmbraActionInstance,
    UmbraActionVTable, UmbraBlackboard, UmbraCapability, UmbraCapabilityKind, UmbraCode,
    UmbraCondition, UmbraConditionVTable, UmbraContact, UmbraContactSink, UmbraEntityView,
    UmbraErrorSink, UmbraHostApi, UmbraIntent, UmbraIntentSink, UmbraKinematicState, UmbraLeafCall,
    UmbraLogLevel, UmbraMotionModel, UmbraMotionStep, UmbraMotionVTable, UmbraPluginDescriptor,
    UmbraQuat, UmbraSenseCall, UmbraSensor, UmbraSensorVTable, UmbraStatus, UmbraStr, UmbraTarget,
    UmbraTerrain, UmbraToken, UmbraValue, UmbraValueSink, UmbraVec3,
};

pub const HOST_CTX: UmbraToken = 0xA11CE;
pub const ERR_CTX: UmbraToken = 0xE_22;
pub const TERRAIN_CTX: UmbraToken = 0x7E_44;
pub const BB_CTX: UmbraToken = 0xBB_01;
pub const CONTACT_CTX: UmbraToken = 0xC0_7A;
pub const INTENT_CTX: UmbraToken = 0x1_7E;
pub const RNG_TOKEN: UmbraToken = 0x52_4E_47;

/// The `abi_version`/`struct_size` a struct written by the SDK carried, with its call.
#[derive(Debug, Clone, PartialEq)]
pub struct Header {
    pub what: &'static str,
    pub abi_version: u32,
    pub struct_size: u32,
}

/// One service call the SDK made, with the token it passed back.
#[derive(Debug, Clone, PartialEq)]
pub struct ServiceCall {
    pub ctx: UmbraToken,
    pub call: &'static str,
    /// The points of a terrain query, or `[key]` as bytes-free text in `key`.
    pub points: Vec<[f64; 3]>,
    pub key: String,
    pub value: Option<HostValue>,
}

/// A value as the host sees it.
#[derive(Debug, Clone, PartialEq)]
pub enum HostValue {
    Bool(u8),
    Number(f64),
    Integer(i64),
    Text(String),
    Entity(u64),
    Position([f64; 3]),
    /// A value of an unknown type code (forged by a test).
    Unknown(u32),
}

/// A contact the SDK pushed.
#[derive(Debug, Clone, PartialEq)]
pub struct PushedContact {
    pub ctx: UmbraToken,
    pub entity: u64,
    pub position: [f64; 3],
    pub velocity: [f64; 3],
    pub force: String,
    pub range_m: f64,
}

/// An intent the SDK pushed.
#[derive(Debug, Clone, PartialEq)]
pub struct PushedIntent {
    pub ctx: UmbraToken,
    pub item: usize,
    pub kind: u32,
    pub value: f64,
    pub id: String,
    pub payload: String,
}

/// How the fake blackboard's `get` answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GetMode {
    /// Reports the stored value (or unset) once.
    Normal,
    /// Returns `UMBRA_OK` without reporting anything.
    Silent,
    /// Reports a value of an unknown type code.
    Malformed,
    /// Reports the stored value twice and records the second report's code.
    Twice,
    /// Reports the stored value once, under a token the SDK never issued, and records the code.
    ForgedToken,
    /// Waits for a second thread's read to be in flight too (a two-party barrier), then reports.
    Concurrent,
    /// From the second read on: first reports under the first read's token (records the code), then
    /// reports normally.
    ReplayFirst,
}

/// The barrier of `GetMode::Concurrent` (two reads, each on its own thread).
pub static CONCURRENT: std::sync::LazyLock<std::sync::Barrier> =
    std::sync::LazyLock::new(|| std::sync::Barrier::new(2));

thread_local! {
    static ERR: RefCell<Option<String>> = const { RefCell::new(None) };
    static LOGS: RefCell<Vec<(u32, String)>> = const { RefCell::new(Vec::new()) };
    static DRAWS: RefCell<Vec<(UmbraToken, UmbraToken)>> = const { RefCell::new(Vec::new()) };
    static SERVICES: RefCell<Vec<ServiceCall>> = const { RefCell::new(Vec::new()) };
    static HEADERS: RefCell<Vec<Header>> = const { RefCell::new(Vec::new()) };
    static CONTACTS: RefCell<Vec<PushedContact>> = const { RefCell::new(Vec::new()) };
    static INTENTS: RefCell<Vec<PushedIntent>> = const { RefCell::new(Vec::new()) };
    static BOARD: RefCell<BTreeMap<String, HostValue>> = const { RefCell::new(BTreeMap::new()) };
    /// Callback name -> the code it refuses with.
    static REFUSE: RefCell<BTreeMap<&'static str, UmbraCode>> = const { RefCell::new(BTreeMap::new()) };
    static GET_MODE: Cell<GetMode> = const { Cell::new(GetMode::Normal) };
    /// The code the SDK's value sink returned to a second report (`GetMode::Twice`).
    static SECOND_REPORT: Cell<Option<UmbraCode>> = const { Cell::new(None) };
    /// What `line_of_sight` writes into `*out` (1 or 0 normally; a test forges other bytes).
    static LOS_BYTE: Cell<Option<u8>> = const { Cell::new(None) };
    /// The token and `set` of the last value sink the SDK passed to `get`.
    static LAST_SINK: Cell<Option<(UmbraToken, ValueSetFn)>> = const { Cell::new(None) };
    /// The token of every value sink the SDK passed to `get`, in order.
    static SINK_TOKENS: RefCell<Vec<UmbraToken>> = const { RefCell::new(Vec::new()) };
}

/// The `set` entry of a value sink.
pub type ValueSetFn = unsafe extern "C" fn(UmbraToken, *const UmbraValue) -> UmbraCode;

pub fn reset() {
    ERR.with(|e| e.borrow_mut().take());
    LOGS.with(|l| l.borrow_mut().clear());
    DRAWS.with(|d| d.borrow_mut().clear());
    SERVICES.with(|s| s.borrow_mut().clear());
    HEADERS.with(|h| h.borrow_mut().clear());
    CONTACTS.with(|c| c.borrow_mut().clear());
    INTENTS.with(|i| i.borrow_mut().clear());
    BOARD.with(|b| b.borrow_mut().clear());
    REFUSE.with(|r| r.borrow_mut().clear());
    GET_MODE.with(|m| m.set(GetMode::Normal));
    SECOND_REPORT.with(|s| s.set(None));
    LOS_BYTE.with(|l| l.set(None));
    LAST_SINK.with(|l| l.set(None));
    SINK_TOKENS.with(|s| s.borrow_mut().clear());
}

/// The token of every value sink the SDK passed to `get` since `reset`.
pub fn sink_tokens() -> Vec<UmbraToken> {
    SINK_TOKENS.with(|s| s.borrow().clone())
}

pub fn take_err() -> Option<String> {
    ERR.with(|e| e.borrow_mut().take())
}
pub fn logs() -> Vec<(u32, String)> {
    LOGS.with(|l| l.borrow().clone())
}
pub fn draws() -> Vec<(UmbraToken, UmbraToken)> {
    DRAWS.with(|d| d.borrow().clone())
}
pub fn services() -> Vec<ServiceCall> {
    SERVICES.with(|s| s.borrow().clone())
}
pub fn headers() -> Vec<Header> {
    HEADERS.with(|h| h.borrow().clone())
}
pub fn contacts() -> Vec<PushedContact> {
    CONTACTS.with(|c| c.borrow().clone())
}
pub fn intents() -> Vec<PushedIntent> {
    INTENTS.with(|i| i.borrow().clone())
}
pub fn board_value(key: &str) -> Option<HostValue> {
    BOARD.with(|b| b.borrow().get(key).cloned())
}
pub fn set_board(key: &str, value: HostValue) {
    BOARD.with(|b| b.borrow_mut().insert(key.to_owned(), value));
}
/// The callback `call` refuses with `code` from now on.
pub fn refuse(call: &'static str, code: UmbraCode) {
    REFUSE.with(|r| r.borrow_mut().insert(call, code));
}
pub fn get_mode(mode: GetMode) {
    GET_MODE.with(|m| m.set(mode));
}
/// The code of the second report (`GetMode::Twice`) or of the forged one (`GetMode::ForgedToken`).
pub fn second_report() -> Option<UmbraCode> {
    SECOND_REPORT.with(Cell::get)
}
/// The token and `set` of the last value sink the SDK passed to the blackboard's `get`.
pub fn last_value_sink() -> (UmbraToken, ValueSetFn) {
    LAST_SINK
        .with(Cell::get)
        .expect("the SDK passed a value sink")
}
pub fn los_byte(byte: u8) {
    LOS_BYTE.with(|l| l.set(Some(byte)));
}

fn refused(call: &'static str) -> Option<UmbraCode> {
    REFUSE.with(|r| r.borrow().get(call).copied())
}

fn header(what: &'static str, abi_version: u32, struct_size: u32) {
    HEADERS.with(|h| {
        h.borrow_mut().push(Header {
            what,
            abi_version,
            struct_size,
        });
    });
}

fn service(ctx: UmbraToken, call: &'static str, points: Vec<[f64; 3]>, key: String) {
    SERVICES.with(|s| {
        s.borrow_mut().push(ServiceCall {
            ctx,
            call,
            points,
            key,
            value: None,
        });
    });
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

pub fn v3(v: [f64; 3]) -> UmbraVec3 {
    UmbraVec3 {
        x: v[0],
        y: v[1],
        z: v[2],
    }
}

fn arr(v: &UmbraVec3) -> [f64; 3] {
    [v.x, v.y, v.z]
}

const fn size<T>() -> u32 {
    size_of::<T>() as u32
}

// ---------------------------------------------------------------- host API and error sink

unsafe extern "C" fn log(_ctx: UmbraToken, level: UmbraLogLevel, msg: UmbraStr) -> UmbraCode {
    LOGS.with(|l| l.borrow_mut().push((level, text(msg))));
    UMBRA_OK
}

/// A draw: `token * 1000 + n` for the `n`-th draw of `token` (u64), or that divided by 2^32 (f64).
fn draw(ctx: UmbraToken, token: UmbraToken) -> u64 {
    DRAWS.with(|d| {
        let mut d = d.borrow_mut();
        let n = d.iter().filter(|(_, t)| *t == token).count() as u64;
        d.push((ctx, token));
        token.wrapping_mul(1000).wrapping_add(n)
    })
}

unsafe extern "C" fn rng_u64(ctx: UmbraToken, token: UmbraToken, out: *mut u64) -> UmbraCode {
    if let Some(code) = refused("rng") {
        return code;
    }
    // SAFETY: the SDK passes a valid out pointer.
    unsafe { *out = draw(ctx, token) };
    UMBRA_OK
}

unsafe extern "C" fn rng_f64(ctx: UmbraToken, token: UmbraToken, out: *mut f64) -> UmbraCode {
    if let Some(code) = refused("rng") {
        return code;
    }
    let v = draw(ctx, token) % 1000;
    // SAFETY: the SDK passes a valid out pointer.
    unsafe { *out = v as f64 / 1000.0 };
    UMBRA_OK
}

pub fn host_api() -> UmbraHostApi {
    UmbraHostApi {
        abi_version: UMBRA_ABI_VERSION,
        struct_size: size::<UmbraHostApi>(),
        host_ctx: HOST_CTX,
        log: Some(log),
        rng_next_u64: Some(rng_u64),
        rng_next_f64: Some(rng_f64),
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
        struct_size: size::<UmbraErrorSink>(),
        ctx: ERR_CTX,
        set: Some(err_set),
    }
}

// ---------------------------------------------------------------- terrain

/// The fake terrain height under `p`: `p.x / 1000 + p.y / 10_000` (a pure function of the query).
pub fn height_of(p: [f64; 3]) -> f64 {
    p[0] / 1000.0 + p[1] / 10_000.0
}

/// The fake terrain normal (ENU) everywhere.
pub const NORMAL: [f64; 3] = [0.6, 0.0, 0.8];

/// The fake line of sight: visible iff `to.z >= from.z`.
pub fn visible(from: [f64; 3], to: [f64; 3]) -> bool {
    to[2] >= from[2]
}

unsafe extern "C" fn height_at(ctx: UmbraToken, p: *const UmbraVec3, out: *mut f64) -> UmbraCode {
    // SAFETY: the SDK passes valid pointers for the call.
    let p = arr(unsafe { &*p });
    service(ctx, "height_at", vec![p], String::new());
    if let Some(code) = refused("height_at") {
        return code;
    }
    // SAFETY: as above.
    unsafe { *out = height_of(p) };
    UMBRA_OK
}

unsafe extern "C" fn normal_at(
    ctx: UmbraToken,
    p: *const UmbraVec3,
    out: *mut UmbraVec3,
) -> UmbraCode {
    // SAFETY: the SDK passes valid pointers for the call.
    let p = arr(unsafe { &*p });
    service(ctx, "normal_at", vec![p], String::new());
    if let Some(code) = refused("normal_at") {
        return code;
    }
    // SAFETY: as above.
    unsafe { *out = v3(NORMAL) };
    UMBRA_OK
}

unsafe extern "C" fn line_of_sight(
    ctx: UmbraToken,
    from: *const UmbraVec3,
    to: *const UmbraVec3,
    out: *mut u8,
) -> UmbraCode {
    // SAFETY: the SDK passes valid pointers for the call.
    let (from, to) = unsafe { (arr(&*from), arr(&*to)) };
    service(ctx, "line_of_sight", vec![from, to], String::new());
    if let Some(code) = refused("line_of_sight") {
        return code;
    }
    let byte = LOS_BYTE
        .with(Cell::get)
        .unwrap_or(u8::from(visible(from, to)));
    // SAFETY: as above.
    unsafe { *out = byte };
    UMBRA_OK
}

pub fn terrain() -> UmbraTerrain {
    UmbraTerrain {
        abi_version: UMBRA_ABI_VERSION,
        struct_size: size::<UmbraTerrain>(),
        ctx: TERRAIN_CTX,
        height_at: Some(height_at),
        normal_at: Some(normal_at),
        line_of_sight: Some(line_of_sight),
    }
}

// ---------------------------------------------------------------- blackboard

fn host_value(v: &UmbraValue) -> HostValue {
    match v.ty {
        UMBRA_VALUE_BOOL => HostValue::Bool(v.boolean),
        UMBRA_VALUE_NUMBER => HostValue::Number(v.number),
        UMBRA_VALUE_INTEGER => HostValue::Integer(v.integer),
        UMBRA_VALUE_STRING => HostValue::Text(text(v.string)),
        UMBRA_VALUE_ENTITY => HostValue::Entity(v.entity),
        UMBRA_VALUE_POSITION => HostValue::Position(arr(&v.position)),
        other => HostValue::Unknown(other),
    }
}

/// A host-written value (the string borrowed from `text`).
pub fn raw_value(value: &HostValue, text: &str) -> UmbraValue {
    let mut out = UmbraValue {
        abi_version: UMBRA_ABI_VERSION,
        struct_size: size::<UmbraValue>(),
        ty: 0,
        boolean: 0,
        reserved_0: [0; 3],
        number: 0.0,
        integer: 0,
        string: s(""),
        entity: 0,
        position: v3([0.0; 3]),
    };
    match value {
        HostValue::Bool(b) => (out.ty, out.boolean) = (UMBRA_VALUE_BOOL, *b),
        HostValue::Number(n) => (out.ty, out.number) = (UMBRA_VALUE_NUMBER, *n),
        HostValue::Integer(i) => (out.ty, out.integer) = (UMBRA_VALUE_INTEGER, *i),
        HostValue::Text(_) => (out.ty, out.string) = (UMBRA_VALUE_STRING, s(text)),
        HostValue::Entity(e) => (out.ty, out.entity) = (UMBRA_VALUE_ENTITY, *e),
        HostValue::Position(p) => (out.ty, out.position) = (UMBRA_VALUE_POSITION, v3(*p)),
        HostValue::Unknown(code) => out.ty = *code,
    }
    out
}

unsafe extern "C" fn bb_get(
    ctx: UmbraToken,
    key: UmbraStr,
    sink: *const UmbraValueSink,
) -> UmbraCode {
    let key = text(key);
    service(ctx, "bb_get", Vec::new(), key.clone());
    // SAFETY: the SDK passes its value sink for the call.
    let sink = unsafe { &*sink };
    header("value_sink", sink.abi_version, sink.struct_size);
    if let Some(code) = refused("bb_get") {
        return code;
    }
    let set = sink.set.expect("the SDK's value sink has `set`");
    LAST_SINK.with(|l| l.set(Some((sink.ctx, set))));
    let first = SINK_TOKENS.with(|s| {
        let mut s = s.borrow_mut();
        s.push(sink.ctx);
        (s.len() > 1).then(|| s[0])
    });
    let stored = BOARD.with(|b| b.borrow().get(&key).cloned());
    let report_as = |ctx: UmbraToken, value: Option<&HostValue>| {
        let string = match value {
            Some(HostValue::Text(t)) => t.clone(),
            _ => String::new(),
        };
        let raw = value.map(|v| raw_value(v, &string));
        let ptr = raw.as_ref().map_or(std::ptr::null(), std::ptr::from_ref);
        // SAFETY: the SDK's sink for this call; `raw` and `string` outlive the call.
        unsafe { set(ctx, ptr) }
    };
    let report = |value: Option<&HostValue>| report_as(sink.ctx, value);
    match GET_MODE.with(Cell::get) {
        GetMode::Normal => {
            let _ = report(stored.as_ref());
        }
        GetMode::Silent => {}
        GetMode::Malformed => {
            let _ = report(Some(&HostValue::Unknown(99)));
        }
        GetMode::Twice => {
            let _ = report(stored.as_ref());
            let second = report(stored.as_ref());
            SECOND_REPORT.with(|s| s.set(Some(second)));
        }
        GetMode::ForgedToken => {
            // A token the SDK never issues (it counts up from 1), so it cannot be another live read's
            // token when tests run on several threads.
            let forged = report_as(sink.ctx | (1 << 63), stored.as_ref());
            SECOND_REPORT.with(|s| s.set(Some(forged)));
        }
        GetMode::Concurrent => {
            CONCURRENT.wait();
            let _ = report(stored.as_ref());
            // Neither thread returns before both reported, so both reads overlap throughout.
            CONCURRENT.wait();
        }
        GetMode::ReplayFirst => {
            if let Some(old) = first {
                let replayed = report_as(old, stored.as_ref());
                SECOND_REPORT.with(|s| s.set(Some(replayed)));
            }
            let _ = report(stored.as_ref());
        }
    }
    UMBRA_OK
}

unsafe extern "C" fn bb_set(ctx: UmbraToken, key: UmbraStr, value: *const UmbraValue) -> UmbraCode {
    let key = text(key);
    // SAFETY: the SDK passes a value valid for the call, or null.
    let value = unsafe { value.as_ref() };
    if let Some(v) = value {
        header("value", v.abi_version, v.struct_size);
    }
    let value = value.map(host_value);
    SERVICES.with(|s| {
        s.borrow_mut().push(ServiceCall {
            ctx,
            call: "bb_set",
            points: Vec::new(),
            key: key.clone(),
            value: value.clone(),
        });
    });
    if let Some(code) = refused("bb_set") {
        return code;
    }
    BOARD.with(|b| match value {
        Some(v) => {
            b.borrow_mut().insert(key, v);
        }
        None => {
            b.borrow_mut().remove(&key);
        }
    });
    UMBRA_OK
}

pub fn blackboard() -> UmbraBlackboard {
    UmbraBlackboard {
        abi_version: UMBRA_ABI_VERSION,
        struct_size: size::<UmbraBlackboard>(),
        ctx: BB_CTX,
        get: Some(bb_get),
        set: Some(bb_set),
    }
}

// ---------------------------------------------------------------- contact and intent sinks

unsafe extern "C" fn contact_push(ctx: UmbraToken, c: *const UmbraContact) -> UmbraCode {
    // SAFETY: the SDK passes a valid contact for the call.
    let c = unsafe { &*c };
    header("contact", c.abi_version, c.struct_size);
    if let Some(code) = refused("contact") {
        return code;
    }
    CONTACTS.with(|l| {
        l.borrow_mut().push(PushedContact {
            ctx,
            entity: c.entity,
            position: arr(&c.position),
            velocity: arr(&c.velocity),
            force: text(c.force),
            range_m: c.range_m,
        });
    });
    UMBRA_OK
}

pub fn contact_sink() -> UmbraContactSink {
    UmbraContactSink {
        abi_version: UMBRA_ABI_VERSION,
        struct_size: size::<UmbraContactSink>(),
        ctx: CONTACT_CTX,
        push: Some(contact_push),
    }
}

unsafe extern "C" fn intent_push(ctx: UmbraToken, item: usize, i: *const UmbraIntent) -> UmbraCode {
    // SAFETY: the SDK passes a valid intent for the call.
    let i = unsafe { &*i };
    header("intent", i.abi_version, i.struct_size);
    if let Some(code) = refused("intent") {
        return code;
    }
    INTENTS.with(|l| {
        l.borrow_mut().push(PushedIntent {
            ctx,
            item,
            kind: i.kind,
            value: i.value,
            id: text(i.id),
            payload: text(i.payload),
        });
    });
    UMBRA_OK
}

pub fn intent_sink() -> UmbraIntentSink {
    UmbraIntentSink {
        abi_version: UMBRA_ABI_VERSION,
        struct_size: size::<UmbraIntentSink>(),
        ctx: INTENT_CTX,
        push: Some(intent_push),
    }
}

// ---------------------------------------------------------------- host-written records

pub const IDENTITY: UmbraQuat = UmbraQuat {
    w: 1.0,
    x: 0.0,
    y: 0.0,
    z: 0.0,
};

pub fn kinematic(position: [f64; 3], velocity: [f64; 3]) -> UmbraKinematicState {
    UmbraKinematicState {
        abi_version: UMBRA_ABI_VERSION,
        struct_size: size::<UmbraKinematicState>(),
        position: v3(position),
        velocity: v3(velocity),
        orientation: IDENTITY,
    }
}

/// A host-written intent of `kind` (unused fields zero).
pub fn raw_intent(kind: u32) -> UmbraIntent {
    UmbraIntent {
        abi_version: UMBRA_ABI_VERSION,
        struct_size: size::<UmbraIntent>(),
        kind,
        reserved_0: 0,
        position: v3([0.0; 3]),
        value: 0.0,
        entity: 0,
        id: s(""),
        payload: s(""),
    }
}

pub fn target(id: u64, position: [f64; 3], ownership: u32, force: &'static str) -> UmbraTarget {
    UmbraTarget {
        abi_version: UMBRA_ABI_VERSION,
        struct_size: size::<UmbraTarget>(),
        id,
        position: v3(position),
        velocity: v3([id as f64, 0.0, 0.0]),
        orientation: IDENTITY,
        force: s(force),
        ownership,
        reserved_0: 0,
    }
}

pub fn view(id: u64) -> UmbraEntityView {
    UmbraEntityView {
        abi_version: UMBRA_ABI_VERSION,
        struct_size: size::<UmbraEntityView>(),
        id,
        position: v3([1.0, 2.0, 3.0]),
        velocity: v3([4.0, 5.0, 6.0]),
        orientation: IDENTITY,
        force: s("blue"),
        has_route: 0,
        route_complete: 0,
        reserved_0: [0; 6],
        route_next: 0,
        route: std::ptr::null(),
        route_len: 0,
        contacts: std::ptr::null(),
        contact_count: 0,
    }
}

/// A motion step record (pointers into the caller's values; valid while they are).
pub fn motion_step(
    state: &UmbraKinematicState,
    intents: &[UmbraIntent],
    terrain: &UmbraTerrain,
    tick: u64,
) -> UmbraMotionStep {
    UmbraMotionStep {
        abi_version: UMBRA_ABI_VERSION,
        struct_size: size::<UmbraMotionStep>(),
        tick,
        rate_hz: 30,
        reserved_0: 0,
        dt: 1.0 / 30.0,
        state,
        intents: if intents.is_empty() {
            std::ptr::null()
        } else {
            intents.as_ptr()
        },
        intent_count: intents.len(),
        terrain,
    }
}

pub fn sense_call(
    observer: &UmbraTarget,
    candidates: &[UmbraTarget],
    terrain: &UmbraTerrain,
    tick: u64,
) -> UmbraSenseCall {
    UmbraSenseCall {
        abi_version: UMBRA_ABI_VERSION,
        struct_size: size::<UmbraSenseCall>(),
        tick,
        rate_hz: 30,
        reserved_0: 0,
        observer,
        candidates: if candidates.is_empty() {
            std::ptr::null()
        } else {
            candidates.as_ptr()
        },
        candidate_count: candidates.len(),
        rng: RNG_TOKEN,
        terrain,
    }
}

pub fn leaf_call(entity: &UmbraEntityView, bb: &UmbraBlackboard, tick: u64) -> UmbraLeafCall {
    UmbraLeafCall {
        abi_version: UMBRA_ABI_VERSION,
        struct_size: size::<UmbraLeafCall>(),
        tick,
        rate_hz: 30,
        reserved_0: 0,
        entity,
        rng: RNG_TOKEN,
        blackboard: bb,
    }
}

// ---------------------------------------------------------------- descriptor and vtables

pub fn capabilities(d: &UmbraPluginDescriptor) -> &[UmbraCapability] {
    if d.capability_count == 0 {
        return &[];
    }
    // SAFETY: the SDK's descriptor holds `capability_count` capabilities for its lifetime.
    unsafe { std::slice::from_raw_parts(d.capabilities, d.capability_count) }
}

/// The capability of `d` with id `id` (its kind must be `kind`).
pub fn capability<'a>(
    d: &'a UmbraPluginDescriptor,
    id: &str,
    kind: UmbraCapabilityKind,
) -> &'a UmbraCapability {
    let cap = capabilities(d)
        .iter()
        .find(|c| text(c.id) == id)
        .unwrap_or_else(|| panic!("capability {id}"));
    assert_eq!(cap.kind, kind, "kind of {id}");
    cap
}

/// The vtable of `cap` as `T` (the caller names the kind's vtable type).
pub fn vtable<T>(cap: &UmbraCapability) -> &T {
    // SAFETY: the SDK's vtables are statics of the descriptor; the caller checked the kind.
    unsafe { &*cap.vtable.cast::<T>() }
}

/// `configure` through any of the four vtables' signature.
pub fn configure_with<H>(
    f: unsafe extern "C" fn(UmbraStr, *mut *mut H, *const UmbraErrorSink) -> UmbraCode,
    params: &str,
) -> (UmbraCode, *mut H, Option<String>) {
    let err = err_sink();
    let mut out: *mut H = std::ptr::null_mut();
    // SAFETY: valid params, out and sink for the call.
    let code = unsafe { f(s(params), &raw mut out, &raw const err) };
    (code, out, take_err())
}

pub fn motion_configure(
    vt: &UmbraMotionVTable,
    params: &str,
) -> (UmbraCode, *mut UmbraMotionModel, Option<String>) {
    configure_with(vt.configure.expect("configure"), params)
}

pub fn sensor_configure(
    vt: &UmbraSensorVTable,
    params: &str,
) -> (UmbraCode, *mut UmbraSensor, Option<String>) {
    configure_with(vt.configure.expect("configure"), params)
}

pub fn condition_configure(
    vt: &UmbraConditionVTable,
    params: &str,
) -> (UmbraCode, *mut UmbraCondition, Option<String>) {
    configure_with(vt.configure.expect("configure"), params)
}

pub fn action_configure(
    vt: &UmbraActionVTable,
    params: &str,
) -> (UmbraCode, *mut UmbraAction, Option<String>) {
    configure_with(vt.configure.expect("configure"), params)
}

/// One motion step; returns the code, the `out` struct after the call and the error message.
pub fn step(
    vt: &UmbraMotionVTable,
    model: *mut UmbraMotionModel,
    call: &UmbraMotionStep,
) -> (UmbraCode, UmbraKinematicState, Option<String>) {
    // SAFETY: the record's state pointer is valid (the caller built it with `motion_step`).
    let start = unsafe { &*call.state };
    let mut out = kinematic(arr(&start.position), arr(&start.velocity));
    out.orientation = start.orientation;
    let err = err_sink();
    let f = vt.step.expect("step");
    // SAFETY: the call record, `out` and the sink are valid for the call (or forged by a test that
    // expects the SDK to refuse them before reading).
    let code = unsafe { f(model, call, &raw mut out, &raw const err) };
    (code, out, take_err())
}

pub fn sense(
    vt: &UmbraSensorVTable,
    sensor: *mut UmbraSensor,
    call: &UmbraSenseCall,
) -> (UmbraCode, Option<String>) {
    let sink = contact_sink();
    let err = err_sink();
    let f = vt.sense.expect("sense");
    // SAFETY: as in `step`.
    let code = unsafe { f(sensor, call, &raw const sink, &raw const err) };
    (code, take_err())
}

/// One check; returns the code, the result byte and the error message.
pub fn check(
    vt: &UmbraConditionVTable,
    condition: *mut UmbraCondition,
    call: &UmbraLeafCall,
) -> (UmbraCode, u8, Option<String>) {
    let mut out = umbra_sdk::abi::UMBRA_CONDITION_UNSET;
    let err = err_sink();
    let f = vt.check.expect("check");
    // SAFETY: as in `step`.
    let code = unsafe { f(condition, call, &raw mut out, &raw const err) };
    (code, out, take_err())
}

pub fn instantiate(
    vt: &UmbraActionVTable,
    action: *mut UmbraAction,
) -> (UmbraCode, *mut UmbraActionInstance, Option<String>) {
    instantiate_at(vt, action, 30)
}

/// `instantiate` at `rate_hz`.
pub fn instantiate_at(
    vt: &UmbraActionVTable,
    action: *mut UmbraAction,
    rate_hz: u32,
) -> (UmbraCode, *mut UmbraActionInstance, Option<String>) {
    let err = err_sink();
    let mut out: *mut UmbraActionInstance = std::ptr::null_mut();
    let f = vt.instantiate.expect("instantiate");
    // SAFETY: a handle from `configure` (or a forged one the SDK must refuse).
    let code = unsafe { f(action, rate_hz, &raw mut out, &raw const err) };
    (code, out, take_err())
}

/// One action tick; returns the code, the status written and the error message.
pub fn tick(
    vt: &UmbraActionVTable,
    instance: *mut UmbraActionInstance,
    call: &UmbraLeafCall,
) -> (UmbraCode, UmbraStatus, Option<String>) {
    let mut out = umbra_sdk::abi::UMBRA_STATUS_UNSET;
    let sink = intent_sink();
    let err = err_sink();
    let f = vt.tick.expect("tick");
    // SAFETY: as in `step`.
    let code = unsafe {
        f(
            instance,
            call,
            &raw const sink,
            &raw mut out,
            &raw const err,
        )
    };
    (code, out, take_err())
}

pub fn halt(vt: &UmbraActionVTable, instance: *mut UmbraActionInstance) -> UmbraCode {
    let f = vt.halt.expect("halt");
    // SAFETY: a handle from `instantiate`.
    unsafe { f(instance) }
}
