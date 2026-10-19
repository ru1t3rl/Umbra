//! A hand-written, raw-ABI behavior plugin with selectable misbehavior (M4 plan WI-9 tests: "a
//! hand-written C-style test plugin in the test file, so every misbehavior is reachable").
//!
//! Capability `test.behavior.raw`. `configure` params (JSON text, parsed by substring so the harness
//! needs no dependency): `"mode":"<mode>"` and `"host":<address of the UmbraHostApi>` (the plugin has no
//! other way to find its image's host API on any thread). The harness records what the plugin saw and did
//! in thread-local logs (single-thread tests) and in process atomics (contention tests).
//!
//! Modes (tick): `ok`, `unset`, `bad_status`, `error_code`, `huge_error`, `null_error`, `two_errors`, `log`,
//! `log_bad_level`, `log_bad_utf8`, `bad_kind`, `flood`, `bad_index`,
//! `short_intent`, `zero_fire`, `bad_id`, `bad_custom`, `stale`, `foreign_rng`, `reenter`, `spin`.
//! Modes (other): `null_handle`, `configure_error`, `instantiate_error`, `instantiate_null`, `halt_error`,
//! `output_twice`, `bad_tag`, `bad_bool`, `nan`, `zero_entity`, `bad_utf8`.

use std::cell::{Cell, RefCell};
use std::mem::size_of;
use std::sync::atomic::{AtomicUsize, Ordering};

use umbra_plugin_api::{
    UMBRA_ABI_VERSION, UMBRA_ERR_INTERNAL, UMBRA_ERR_PARAMS, UMBRA_INTENT_CUSTOM,
    UMBRA_INTENT_FIRE, UMBRA_INTENT_SET_SPEED, UMBRA_OK, UMBRA_STATUS_RUNNING,
    UMBRA_STATUS_SUCCESS, UMBRA_VALUE_BOOL, UMBRA_VALUE_ENTITY, UMBRA_VALUE_INTEGER,
    UMBRA_VALUE_NUMBER, UMBRA_VALUE_POSITION, UMBRA_VALUE_STRING, UmbraBehavior,
    UmbraBehaviorBatch, UmbraBehaviorVTable, UmbraCode, UmbraErrorSink, UmbraHostApi,
    UmbraInstance, UmbraIntent, UmbraStr, UmbraValue, UmbraValueSink, UmbraVec3,
};

use super::forge::{self, Parts};

static BAD: [u8; 2] = [0xFF, 0xFE];

pub const CAPABILITY: &str = "test.behavior.raw";
pub const SCHEMA: &str = r#"{"type":"object","description":"raw test behavior","properties":{"mode":{"type":"string"},"host":{"type":"integer"},"in_bool":{"type":"boolean"},"in_number":{"type":"number"},"in_integer":{"type":"integer"},"in_string":{"type":"string"}}}"#;
pub const METADATA: &str = r#"{"param_types":{"in_bool":"bool","in_number":"number","in_integer":"integer","in_string":"string","in_entity":"entity","in_position":"position"},"outputs":{"out_bool":"bool","out_number":"number","out_integer":"integer","out_string":"string","out_entity":"entity","out_position":"position"},"result":"raw result"}"#;

/// Everything the plugin saw of one item in `tick_batch`.
#[derive(Debug, Clone, PartialEq)]
pub struct Seen {
    pub batch_version: u32,
    pub batch_size: u32,
    pub item_size: u32,
    pub view_version: u32,
    pub view_size: u32,
    pub tick: u64,
    pub rate_hz: u32,
    pub prefilled_status: u32,
    pub id: u64,
    pub position: [f64; 3],
    pub velocity: [f64; 3],
    pub orientation: [f64; 4],
    pub force: String,
    pub has_route: u8,
    pub route_complete: u8,
    pub route_next: usize,
    pub route: Vec<[f64; 3]>,
    /// `(entity, position, range_m, force)`
    pub contacts: Vec<(u64, [f64; 3], f64, String)>,
    pub contact_size: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Got {
    Unset,
    Bool(u8),
    Number(f64),
    Integer(i64),
    Text(String),
    Entity(u64),
    Position([f64; 3]),
}

thread_local! {
    pub static EVENTS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    pub static SEEN: RefCell<Vec<Seen>> = const { RefCell::new(Vec::new()) };
    pub static INPUTS: RefCell<Vec<(String, Got)>> = const { RefCell::new(Vec::new()) };
    pub static KEPT: Cell<Option<(u64, u64)>> = const { Cell::new(None) };
    pub static CODES: RefCell<Vec<i32>> = const { RefCell::new(Vec::new()) };
    pub static HOOK: RefCell<Option<Box<dyn Fn()>>> = const { RefCell::new(None) };
    /// Problems found inside `extern "C"` callbacks, which must not panic (a panic would abort the test
    /// process): recorded here per test thread, returned as `UMBRA_ERR_INTERNAL`, and asserted empty when
    /// the `Env` drops.
    static FAILURES: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}
pub static IN_CALL: AtomicUsize = AtomicUsize::new(0);
pub static MAX_CONCURRENT: AtomicUsize = AtomicUsize::new(0);
pub static CALLS: AtomicUsize = AtomicUsize::new(0);

fn missing(what: &str) -> UmbraCode {
    FAILURES.with(|f| {
        f.borrow_mut()
            .push(format!("raw plugin: {what} is not set"));
    });
    UMBRA_ERR_INTERNAL
}

/// The callback problems recorded so far on this thread.
pub fn failures() -> Vec<String> {
    FAILURES.with(|f| f.borrow().clone())
}

pub fn events() -> Vec<String> {
    EVENTS.with(|e| e.borrow().clone())
}
pub fn seen() -> Vec<Seen> {
    SEEN.with(|e| e.borrow().clone())
}
pub fn codes() -> Vec<i32> {
    CODES.with(|e| e.borrow().clone())
}
pub fn inputs() -> Vec<(String, Got)> {
    INPUTS.with(|e| e.borrow().clone())
}
pub fn clear() {
    EVENTS.with(|e| e.borrow_mut().clear());
    SEEN.with(|e| e.borrow_mut().clear());
    INPUTS.with(|e| e.borrow_mut().clear());
    KEPT.with(|k| k.set(None));
    CODES.with(|e| e.borrow_mut().clear());
    HOOK.with(|h| *h.borrow_mut() = None);
    FAILURES.with(|f| f.borrow_mut().clear());
}
fn event(name: &str) {
    EVENTS.with(|e| e.borrow_mut().push(name.to_owned()));
}

/// Params text for `configure`: the mode and the host API address captured by `forged!`.
pub fn params(mode: &str, host: usize) -> String {
    format!("{{\"mode\":\"{mode}\",\"host\":{host}}}")
}

struct Behav {
    mode: String,
    host: *const UmbraHostApi,
}
struct Inst {
    mode: String,
    host: *const UmbraHostApi,
    stored: Vec<(String, Got)>,
}

fn text(s: UmbraStr) -> String {
    if s.len == 0 {
        return String::new();
    }
    // SAFETY: the host passes valid bytes for the call.
    String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(s.ptr, s.len) }).into_owned()
}
fn str_of(t: &str) -> UmbraStr {
    UmbraStr {
        ptr: t.as_ptr(),
        len: t.len(),
    }
}
fn find_after<'a>(hay: &'a str, key: &str) -> Option<&'a str> {
    hay.find(key).map(|i| &hay[i + key.len()..])
}
fn mode_of(params: &str) -> String {
    find_after(params, "\"mode\":\"")
        .and_then(|r| r.split('"').next())
        .unwrap_or("ok")
        .to_owned()
}
fn host_of(params: &str) -> usize {
    find_after(params, "\"host\":")
        .map(|r| {
            r.chars()
                .take_while(char::is_ascii_digit)
                .collect::<String>()
        })
        .and_then(|d| d.parse().ok())
        .unwrap_or(0)
}

unsafe fn say(err: *const UmbraErrorSink, message: &str) {
    if err.is_null() {
        return;
    }
    // SAFETY: the host passes a valid sink.
    let e = unsafe { &*err };
    if let Some(set) = e.set {
        // SAFETY: valid ctx and string for the call.
        unsafe { set(e.ctx, str_of(message)) };
    }
}

unsafe extern "C" fn configure(
    params: UmbraStr,
    out: *mut *mut UmbraBehavior,
    err: *const UmbraErrorSink,
) -> UmbraCode {
    let p = text(params);
    let mode = mode_of(&p);
    event("configure");
    // SAFETY: valid out pointer and sink.
    unsafe {
        if mode == "configure_error" {
            say(err, "raw configure error");
            return UMBRA_ERR_PARAMS;
        }
        if mode == "null_handle" {
            *out = std::ptr::null_mut();
            return UMBRA_OK;
        }
        let host = host_of(&p) as *const UmbraHostApi;
        *out = Box::into_raw(Box::new(Behav { mode, host })).cast();
    }
    UMBRA_OK
}
unsafe extern "C" fn destroy_behavior(b: *mut UmbraBehavior) {
    event("destroy_behavior");
    if !b.is_null() {
        // SAFETY: created by `configure`, destroyed once.
        drop(unsafe { Box::from_raw(b.cast::<Behav>()) });
    }
}
unsafe extern "C" fn instantiate(
    b: *mut UmbraBehavior,
    _rate: u32,
    out: *mut *mut UmbraInstance,
    err: *const UmbraErrorSink,
) -> UmbraCode {
    event("instantiate");
    // SAFETY: handle from `configure`; valid out and sink.
    unsafe {
        let b = &*b.cast::<Behav>();
        if b.mode == "instantiate_error" {
            say(err, "raw instantiate error");
            return UMBRA_ERR_INTERNAL;
        }
        if b.mode == "instantiate_null" {
            return UMBRA_OK; // OK with the out handle left null (R-M9)
        }
        *out = Box::into_raw(Box::new(Inst {
            mode: b.mode.clone(),
            host: b.host,
            stored: Vec::new(),
        }))
        .cast();
    }
    UMBRA_OK
}
unsafe extern "C" fn destroy_instance(i: *mut UmbraInstance) {
    event("destroy_instance");
    if !i.is_null() {
        // SAFETY: created by `instantiate`, destroyed once.
        drop(unsafe { Box::from_raw(i.cast::<Inst>()) });
    }
}

fn intent(kind: u32) -> UmbraIntent {
    UmbraIntent {
        abi_version: UMBRA_ABI_VERSION,
        struct_size: size_of::<UmbraIntent>() as u32,
        kind,
        reserved_0: 0,
        position: UmbraVec3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
        value: 0.0,
        entity: 0,
        id: str_of(""),
        payload: str_of(""),
    }
}

unsafe fn draw(host: *const UmbraHostApi, token: u64) -> Result<u64, UmbraCode> {
    // SAFETY: the host API is process-lifetime.
    let h = unsafe { &*host };
    let mut out = 0_u64;
    let Some(rng) = h.rng_next_u64 else {
        return Err(missing("host rng_next_u64"));
    };
    // SAFETY: valid out pointer; the host API stays valid.
    let code = unsafe { rng(h.host_ctx, token, &mut out) };
    if code == UMBRA_OK { Ok(out) } else { Err(code) }
}

unsafe extern "C" fn tick_batch(
    batch: *const UmbraBehaviorBatch,
    err: *const UmbraErrorSink,
) -> UmbraCode {
    CALLS.fetch_add(1, Ordering::SeqCst);
    let now = IN_CALL.fetch_add(1, Ordering::SeqCst) + 1;
    MAX_CONCURRENT.fetch_max(now, Ordering::SeqCst);
    // SAFETY: the host passes a valid batch for the call.
    let code = unsafe { tick_inner(batch, err) };
    IN_CALL.fetch_sub(1, Ordering::SeqCst);
    code
}

#[expect(
    clippy::too_many_lines,
    reason = "one match over the misbehavior modes"
)]
unsafe fn tick_inner(batch: *const UmbraBehaviorBatch, err: *const UmbraErrorSink) -> UmbraCode {
    // SAFETY: valid batch, items, views and sink for the whole call (host contract).
    unsafe {
        let b = &*batch;
        let items = std::slice::from_raw_parts(b.items, b.count);
        let sink = &*b.intents;
        let Some(push) = sink.push else {
            return missing("intent sink push");
        };
        for (n, item) in items.iter().enumerate() {
            let beh = &*item.behavior.cast::<Behav>();
            let view = &*item.entity;
            let route = if view.route_len == 0 {
                Vec::new()
            } else {
                std::slice::from_raw_parts(view.route, view.route_len)
                    .iter()
                    .map(|v| [v.x, v.y, v.z])
                    .collect()
            };
            let contacts = if view.contact_count == 0 {
                Vec::new()
            } else {
                std::slice::from_raw_parts(view.contacts, view.contact_count)
                    .iter()
                    .map(|c| {
                        (
                            c.entity,
                            [c.position.x, c.position.y, c.position.z],
                            c.range_m,
                            text(c.force),
                        )
                    })
                    .collect()
            };
            SEEN.with(|s| {
                s.borrow_mut().push(Seen {
                    batch_version: b.abi_version,
                    batch_size: b.struct_size,
                    item_size: item.struct_size,
                    view_version: view.abi_version,
                    view_size: view.struct_size,
                    tick: b.tick,
                    rate_hz: b.rate_hz,
                    prefilled_status: *b.statuses.add(n),
                    id: view.id,
                    position: [view.position.x, view.position.y, view.position.z],
                    velocity: [view.velocity.x, view.velocity.y, view.velocity.z],
                    orientation: [
                        view.orientation.w,
                        view.orientation.x,
                        view.orientation.y,
                        view.orientation.z,
                    ],
                    force: text(view.force),
                    has_route: view.has_route,
                    route_complete: view.route_complete,
                    route_next: view.route_next,
                    route,
                    contacts,
                    contact_size: if view.contact_count == 0 {
                        0
                    } else {
                        (*view.contacts).struct_size
                    },
                });
            });
            let host = beh.host;
            let mut status = UMBRA_STATUS_SUCCESS + (view.id % 2) as u32 * 2; // odd id: Running (3)
            let send =
                |kind: u32, item_index: usize, edit: &dyn Fn(&mut UmbraIntent)| -> UmbraCode {
                    let mut i = intent(kind);
                    edit(&mut i);
                    push(sink.ctx, item_index, &i)
                };
            let speed = |i: &mut UmbraIntent| i.value = 1.0;
            match beh.mode.as_str() {
                "unset" => continue,
                "bad_status" => status = 7,
                "error_code" => {
                    say(err, "raw tick failed");
                    return UMBRA_ERR_INTERNAL;
                }
                "huge_error" => {
                    say(err, &"x".repeat(10_000));
                    return UMBRA_ERR_INTERNAL;
                }
                "null_error" => {
                    let e = &*err;
                    let Some(set) = e.set else {
                        return missing("error sink set");
                    };
                    set(
                        e.ctx,
                        UmbraStr {
                            ptr: std::ptr::null(),
                            len: 5,
                        },
                    );
                    return UMBRA_ERR_INTERNAL;
                }
                "two_errors" => {
                    say(err, "first message");
                    say(err, "second message");
                    return UMBRA_ERR_INTERNAL;
                }
                "bad_kind" => {
                    let c = send(0, n, &|_| {});
                    if c != UMBRA_OK {
                        return c;
                    }
                }
                "flood" if n == 0 => {
                    for _ in 0..65 {
                        let c = send(UMBRA_INTENT_SET_SPEED, n, &speed);
                        if c != UMBRA_OK {
                            return c;
                        }
                    }
                }
                "bad_index" => {
                    let c = send(UMBRA_INTENT_SET_SPEED, b.count, &speed);
                    if c != UMBRA_OK {
                        return c;
                    }
                }
                "short_intent" => {
                    let c = send(UMBRA_INTENT_SET_SPEED, n, &|i| i.struct_size = 16);
                    if c != UMBRA_OK {
                        return c;
                    }
                }
                "zero_fire" => {
                    let c = send(UMBRA_INTENT_FIRE, n, &|i| i.id = str_of("test.weapon.gun"));
                    if c != UMBRA_OK {
                        return c;
                    }
                }
                "bad_id" => {
                    let c = send(UMBRA_INTENT_FIRE, n, &|i| {
                        i.entity = 1;
                        i.id = str_of("Bad ID!");
                    });
                    if c != UMBRA_OK {
                        return c;
                    }
                }
                "bad_custom" => {
                    let c = send(UMBRA_INTENT_CUSTOM, n, &|i| {
                        i.id = str_of("test.component.note");
                        i.payload = str_of("{not json");
                    });
                    if c != UMBRA_OK {
                        return c;
                    }
                }
                "stale" => {
                    if let Some((ctx, token)) = KEPT.with(Cell::get) {
                        let mut i = intent(UMBRA_INTENT_SET_SPEED);
                        i.value = 2.0;
                        let c = push(ctx, n, &i);
                        if c != UMBRA_OK {
                            return c;
                        }
                        if let Err(c) = draw(host, token) {
                            return c;
                        }
                    } else if n == 0 {
                        KEPT.with(|k| k.set(Some((sink.ctx, item.rng))));
                    }
                }
                "foreign_rng" => {
                    let other = items.get(1).map_or(item.rng, |o| o.rng);
                    let token = if n == 0 { other } else { item.rng };
                    match draw(host, token) {
                        Ok(v) => {
                            let c = send(UMBRA_INTENT_SET_SPEED, n, &|i| i.value = speed_of(v));
                            if c != UMBRA_OK {
                                return c;
                            }
                        }
                        Err(c) => return c,
                    }
                }
                "log" | "log_bad_level" | "log_bad_utf8" => {
                    let h = &*host;
                    let level = if beh.mode == "log_bad_level" { 99 } else { 3 };
                    let msg = if beh.mode == "log_bad_utf8" {
                        UmbraStr {
                            ptr: BAD.as_ptr(),
                            len: 2,
                        }
                    } else {
                        str_of("raw says hello")
                    };
                    let Some(log) = h.log else {
                        return missing("host log");
                    };
                    let code = log(h.host_ctx, level, msg);
                    CODES.with(|c| c.borrow_mut().push(code));
                }
                "reenter" => {
                    HOOK.with(|h| {
                        if let Some(f) = h.borrow().as_ref() {
                            f();
                        }
                    });
                }
                "spin" => {
                    for _ in 0..20_000 {
                        std::hint::spin_loop();
                    }
                    match draw(host, item.rng) {
                        Ok(v) => {
                            let c = send(UMBRA_INTENT_SET_SPEED, n, &|i| i.value = speed_of(v));
                            if c != UMBRA_OK {
                                return c;
                            }
                        }
                        Err(c) => return c,
                    }
                }
                _ => {
                    // "ok": one draw from the item's own token, reported as SetSpeed.
                    match draw(host, item.rng) {
                        Ok(v) => {
                            let c = send(UMBRA_INTENT_SET_SPEED, n, &|i| i.value = speed_of(v));
                            if c != UMBRA_OK {
                                return c;
                            }
                        }
                        Err(c) => return c,
                    }
                }
            }
            *b.statuses.add(n) = status;
        }
    }
    let _ = UMBRA_STATUS_RUNNING;
    UMBRA_OK
}

unsafe extern "C" fn halt(i: *mut UmbraInstance) -> UmbraCode {
    event("halt");
    // SAFETY: live instance.
    if unsafe { &*i.cast::<Inst>() }.mode == "halt_error" {
        return UMBRA_ERR_INTERNAL;
    }
    UMBRA_OK
}
unsafe extern "C" fn halt_running(_: *mut UmbraInstance) -> UmbraCode {
    event("halt_running");
    UMBRA_OK
}

fn from_abi(v: &UmbraValue) -> Got {
    match v.ty {
        UMBRA_VALUE_BOOL => Got::Bool(v.boolean),
        UMBRA_VALUE_NUMBER => Got::Number(v.number),
        UMBRA_VALUE_INTEGER => Got::Integer(v.integer),
        UMBRA_VALUE_STRING => Got::Text(text(v.string)),
        UMBRA_VALUE_ENTITY => Got::Entity(v.entity),
        _ => Got::Position([v.position.x, v.position.y, v.position.z]),
    }
}

/// How the plugin turns one RNG draw into the `SetSpeed` value it reports (24 bits, exact in `f64`).
pub fn speed_of(draw: u64) -> f64 {
    f64::from(u32::try_from(draw >> 40).unwrap_or(0))
}

unsafe extern "C" fn set_input(
    i: *mut UmbraInstance,
    param: UmbraStr,
    value: *const UmbraValue,
    _err: *const UmbraErrorSink,
) -> UmbraCode {
    // SAFETY: live instance; value null or valid for the call.
    let inst = unsafe { &mut *i.cast::<Inst>() };
    let name = text(param);
    let got = if value.is_null() {
        Got::Unset
    } else {
        // SAFETY: the host passes null (handled above) or a valid value for the call.
        from_abi(unsafe { &*value })
    };
    INPUTS.with(|l| l.borrow_mut().push((name.clone(), got.clone())));
    inst.stored.retain(|(n, _)| *n != name);
    inst.stored.push((name, got));
    UMBRA_OK
}

fn abi_value(ty: u32) -> UmbraValue {
    UmbraValue {
        abi_version: UMBRA_ABI_VERSION,
        struct_size: size_of::<UmbraValue>() as u32,
        ty,
        boolean: 0,
        reserved_0: [0; 3],
        number: 0.0,
        integer: 0,
        string: str_of(""),
        entity: 0,
        position: UmbraVec3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
    }
}

unsafe extern "C" fn output(
    i: *mut UmbraInstance,
    port: UmbraStr,
    sink: *const UmbraValueSink,
) -> UmbraCode {
    // SAFETY: live instance; valid sink.
    let (inst, sink) = unsafe { (&*i.cast::<Inst>(), &*sink) };
    let Some(set) = sink.set else {
        return missing("value sink set");
    };
    let name = text(port).replacen("out_", "in_", 1);
    let call = |v: *const UmbraValue| {
        // SAFETY: the sink is valid for the call (host contract); `v` is null or a valid `UmbraValue`
        // that outlives the call.
        unsafe { set(sink.ctx, v) }
    };
    match inst.mode.as_str() {
        "output_twice" => {
            let v = abi_value(UMBRA_VALUE_INTEGER);
            call(&v);
            return call(&v);
        }
        "bad_tag" => return call(&abi_value(99)),
        "bad_bool" => {
            let mut v = abi_value(UMBRA_VALUE_BOOL);
            v.boolean = 2;
            return call(&v);
        }
        "nan" => {
            let mut v = abi_value(UMBRA_VALUE_NUMBER);
            v.number = f64::NAN;
            return call(&v);
        }
        "zero_entity" => return call(&abi_value(UMBRA_VALUE_ENTITY)),
        "bad_utf8" => {
            let mut v = abi_value(UMBRA_VALUE_STRING);
            v.string = UmbraStr {
                ptr: BAD.as_ptr(),
                len: 2,
            };
            return call(&v);
        }
        _ => {}
    }
    let stored = inst
        .stored
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, g)| g.clone());
    let mut v;
    let text_keep;
    match stored {
        None | Some(Got::Unset) => return call(std::ptr::null()),
        Some(Got::Bool(b)) => {
            v = abi_value(UMBRA_VALUE_BOOL);
            v.boolean = b;
        }
        Some(Got::Number(n)) => {
            v = abi_value(UMBRA_VALUE_NUMBER);
            v.number = n;
        }
        Some(Got::Integer(n)) => {
            v = abi_value(UMBRA_VALUE_INTEGER);
            v.integer = n;
        }
        Some(Got::Text(t)) => {
            v = abi_value(UMBRA_VALUE_STRING);
            text_keep = t;
            v.string = str_of(&text_keep);
        }
        Some(Got::Entity(e)) => {
            v = abi_value(UMBRA_VALUE_ENTITY);
            v.entity = e;
        }
        Some(Got::Position(p)) => {
            v = abi_value(UMBRA_VALUE_POSITION);
            v.position = UmbraVec3 {
                x: p[0],
                y: p[1],
                z: p[2],
            };
        }
    }
    call(&v)
}

/// The raw plugin as forge parts: id `test.plugin.raw`, one capability `test.behavior.raw`.
pub fn parts(with_halt_running: bool) -> Parts {
    let mut p = forge::parts();
    p.desc.id = forge::leak_str("test.plugin.raw");
    p.caps[0] = forge::capability(CAPABILITY);
    p.caps[0].param_schema = forge::leak_str(SCHEMA);
    p.caps[0].metadata = forge::leak_str(METADATA);
    *p.vtable = UmbraBehaviorVTable {
        abi_version: UMBRA_ABI_VERSION,
        struct_size: size_of::<UmbraBehaviorVTable>() as u32,
        configure: Some(configure),
        destroy_behavior: Some(destroy_behavior),
        instantiate: Some(instantiate),
        destroy_instance: Some(destroy_instance),
        tick_batch: Some(tick_batch),
        halt: Some(halt),
        halt_running: with_halt_running
            .then_some(halt_running as unsafe extern "C" fn(*mut UmbraInstance) -> UmbraCode),
        set_input: Some(set_input),
        output: Some(output),
    };
    p.link();
    p
}
