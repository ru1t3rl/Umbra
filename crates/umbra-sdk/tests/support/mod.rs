//! Shared harness for the AT-3 tests (M4 step 4): a fake host (log, RNG, sinks that record what the
//! SDK sends), a test plugin built with `export_plugin!`, and helpers to call its vtable through the
//! raw ABI. Each test binary includes this module once, so each gets its own exported
//! `umbra_plugin_entry`.
#![warn(clippy::undocumented_unsafe_blocks)]
#![expect(dead_code, reason = "each test binary uses a subset of the harness")]
#![expect(unsafe_code, reason = "the harness plays the host side of the C ABI")]
#![expect(
    clippy::borrow_as_ptr,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::expect_used,
    clippy::ref_as_ptr,
    reason = "test harness casts small ABI sizes and plays the C side"
)]

use std::cell::RefCell;
use std::ffi::c_void;
use std::mem::size_of;

use umbra_sdk::abi::{
    UMBRA_ABI_VERSION, UMBRA_OK, UmbraBehavior, UmbraBehaviorBatch, UmbraBehaviorItem,
    UmbraBehaviorVTable, UmbraCapability, UmbraCode, UmbraEntityView, UmbraErrorSink, UmbraHostApi,
    UmbraInstance, UmbraIntent, UmbraIntentSink, UmbraLogLevel, UmbraPluginDescriptor, UmbraQuat,
    UmbraStr, UmbraToken, UmbraValue, UmbraValueSink, UmbraVec3,
};
use umbra_sdk::{
    Behavior, BehaviorFactory, BehaviorInstance, EntityView, Error, Intent, Intents, Interface,
    Params, Status, TickContext, Value, ValueType, export_plugin,
};

pub const HOST_CTX: UmbraToken = 0xA11CE;
pub const SINK_CTX: UmbraToken = 0x51_4B;

// ---------------------------------------------------------------- the test plugin

/// Where `configure` params ask the plugin to panic: `{"panic": "<mode>"}`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mode {
    None,
    Configure,
    Instantiate,
    Tick,
    DropPayload,
    Halt,
    HaltRunning,
    SetInput,
    Output,
    DestroyBehavior,
    DestroyInstance,
}

struct DropBomb;
impl Drop for DropBomb {
    fn drop(&mut self) {
        assert!(std::thread::panicking(), "payload drop boom");
    }
}

pub struct OkFactory;
pub struct OkBehavior {
    mode: Mode,
}
pub struct OkInstance {
    mode: Mode,
    ticks: u64,
    turns: i64,
    echo: Option<Value>,
}

impl BehaviorFactory for OkFactory {
    const ID: &'static str = "test.behavior.sdk_ok";
    const PARAM_SCHEMA: &'static str = r#"{"type":"object","description":"SDK test behavior","properties":{"speed_mps":{"type":"number"},"panic":{"type":"string"}},"additionalProperties":false}"#;
    type Behavior = OkBehavior;
    fn interface() -> Interface {
        Interface::default()
            .param("speed_mps", ValueType::Number)
            .output("turns", ValueType::Integer)
            .result("sdk done")
    }
    fn configure(params: &Params<'_>) -> Result<OkBehavior, Error> {
        CONFIGURED.with(|c| c.set(c.get() + 1));
        let value = params.value()?;
        let mode = match value.get("panic").and_then(|v| v.as_str()) {
            None => Mode::None,
            Some("configure") => panic!("boom configure"),
            Some("instantiate") => Mode::Instantiate,
            Some("tick") => Mode::Tick,
            Some("drop_payload") => Mode::DropPayload,
            Some("halt") => Mode::Halt,
            Some("halt_running") => Mode::HaltRunning,
            Some("set_input") => Mode::SetInput,
            Some("output") => Mode::Output,
            Some("destroy_behavior") => Mode::DestroyBehavior,
            Some("destroy_instance") => Mode::DestroyInstance,
            Some(other) => return Err(Error::params(format!("unknown panic mode {other}"))),
        };
        Ok(OkBehavior { mode })
    }
}

impl Drop for OkBehavior {
    fn drop(&mut self) {
        assert!(
            self.mode != Mode::DestroyBehavior || std::thread::panicking(),
            "boom destroy_behavior"
        );
    }
}
impl Drop for OkInstance {
    fn drop(&mut self) {
        assert!(
            self.mode != Mode::DestroyInstance || std::thread::panicking(),
            "boom destroy_instance"
        );
    }
}

impl Behavior for OkBehavior {
    type Instance = OkInstance;
    fn instantiate(&self, _rate_hz: u32) -> Result<OkInstance, Error> {
        assert!(!(self.mode == Mode::Instantiate), "boom instantiate");
        Ok(OkInstance {
            mode: self.mode,
            ticks: 0,
            turns: 0,
            echo: None,
        })
    }
}

impl BehaviorInstance for OkInstance {
    fn tick(
        &mut self,
        ctx: &mut TickContext<'_>,
        entity: &EntityView<'_>,
        out: &mut Intents<'_>,
    ) -> Status {
        match self.mode {
            Mode::Tick => panic!("boom tick"),
            Mode::DropPayload => std::panic::panic_any(DropBomb),
            _ => {}
        }
        record_view(entity);
        let draw = ctx.rng().next_u64();
        if self.ticks == 0 {
            // Every intent variant once, in ABI kind order (AT-3 `every_intent_variant_converts`).
            let all = [
                Intent::MoveTo([1.0, 2.0, 3.0]),
                Intent::FollowRoute,
                Intent::SetSpeed(4.5),
                Intent::SetHeading(0.25),
                Intent::Hold,
                Intent::Fire {
                    target: 7,
                    weapon: "test.weapon.gun",
                },
                Intent::Spawn {
                    template: "test.template.unit",
                    position: [9.0, 8.0, 7.0],
                    heading: 1.5,
                },
                Intent::Despawn,
                Intent::Custom {
                    kind: "test.component.note",
                    payload_json: "{\"n\":1}",
                },
            ];
            for i in all {
                let _ = out.push(i);
            }
        } else {
            let _ = out.push(Intent::SetSpeed(
                u8::try_from(draw % 100).map_or(0.0, f64::from),
            ));
            let _ = out.push(Intent::SetHeading(ctx.rng().next_f64()));
        }
        self.ticks += 1;
        self.turns += 1;
        match entity.id % 3 {
            0 => Status::Success,
            1 => Status::Failure,
            _ => Status::Running,
        }
    }
    fn halt(&mut self) {
        assert!(!(self.mode == Mode::Halt), "boom halt");
        self.ticks = 0;
    }
    fn halt_running(&mut self) {
        assert!(!(self.mode == Mode::HaltRunning), "boom halt_running");
        self.halt();
    }
    fn set_input(&mut self, param: &str, value: Option<Value>) -> Result<(), Error> {
        assert!(!(self.mode == Mode::SetInput), "boom set_input");
        if param == "echo" {
            self.echo = value;
            Ok(())
        } else {
            Err(Error::new(format!("undeclared input {param}")))
        }
    }
    fn output(&self, port: &str) -> Option<Value> {
        assert!(!(self.mode == Mode::Output), "boom output");
        match port {
            "turns" => Some(Value::Integer(self.turns)),
            "echo" => self.echo.clone(),
            _ => None,
        }
    }
}

pub struct SecondFactory;
pub struct SecondBehavior;
impl BehaviorFactory for SecondFactory {
    const ID: &'static str = "test.behavior.sdk_second";
    const PARAM_SCHEMA: &'static str = r#"{"type":"object"}"#;
    const VERSION: Option<&'static str> = Some("2.0.0");
    type Behavior = SecondBehavior;
    fn configure(_: &Params<'_>) -> Result<SecondBehavior, Error> {
        Ok(SecondBehavior)
    }
}
impl Behavior for SecondBehavior {
    type Instance = OkInstance;
    fn instantiate(&self, _: u32) -> Result<OkInstance, Error> {
        Ok(OkInstance {
            mode: Mode::None,
            ticks: 0,
            turns: 0,
            echo: None,
        })
    }
}

export_plugin! {
    id: "test.plugin.sdk",
    name: "SDK test plugin",
    version: "0.3.1",
    behaviors: [OkFactory, SecondFactory],
}

// ---------------------------------------------------------------- the fake host

#[derive(Debug, Clone, PartialEq)]
pub struct Draw {
    pub host_ctx: UmbraToken,
    pub token: UmbraToken,
    pub value: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Pushed {
    pub item: usize,
    pub kind: u32,
    pub position: [f64; 3],
    pub value: f64,
    pub entity: u64,
    pub id: String,
    pub payload: String,
    pub abi_version: u32,
    pub struct_size: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Out {
    Unset,
    Bool(u8),
    Number(f64),
    Integer(i64),
    Text(String),
    Entity(u64),
    Position([f64; 3]),
}

/// One contact as a tick saw it: `(entity, position, velocity, force, range)`.
pub type SeenContact = (u64, [f64; 3], [f64; 3], Option<String>, f64);

/// What a tick saw of its entity view (WI-7 view conversion checks).
#[derive(Debug, Clone, PartialEq)]
pub struct Seen {
    pub id: u64,
    pub position: [f64; 3],
    pub velocity: [f64; 3],
    pub orientation: [f64; 4],
    pub force: Option<String>,
    /// `(waypoints, next index, complete)`.
    pub route: Option<(Vec<[f64; 3]>, usize, bool)>,
    pub contacts: Vec<SeenContact>,
}

fn record_view(entity: &EntityView<'_>) {
    let seen = Seen {
        id: entity.id,
        position: entity.position,
        velocity: entity.velocity,
        orientation: entity.orientation,
        force: entity.force.map(str::to_owned),
        route: entity
            .route
            .map(|r| (r.waypoints().collect(), r.next_index(), r.is_complete())),
        contacts: entity
            .contacts()
            .iter()
            .map(|c| {
                (
                    c.entity,
                    c.position,
                    c.velocity,
                    c.force.map(str::to_owned),
                    c.range_m,
                )
            })
            .collect(),
    };
    SEEN.with(|s| s.borrow_mut().push(seen));
}

thread_local! {
    static SEEN: RefCell<Vec<Seen>> = const { RefCell::new(Vec::new()) };
    static DRAWS: RefCell<Vec<Draw>> = const { RefCell::new(Vec::new()) };
    static PUSHED: RefCell<Vec<Pushed>> = const { RefCell::new(Vec::new()) };
    static ERR: RefCell<Option<String>> = const { RefCell::new(None) };
    static VALUES: RefCell<Vec<Out>> = const { RefCell::new(Vec::new()) };
    static LOGS: RefCell<Vec<(u32, String)>> = const { RefCell::new(Vec::new()) };
    /// How often `OkFactory::configure` (the trait method) ran on this thread (M4 WI-10, deferred
    /// step-4 item: direct evidence that refused calls never reach the trait).
    static CONFIGURED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

pub fn reset() {
    SEEN.with(|d| d.borrow_mut().clear());
    DRAWS.with(|d| d.borrow_mut().clear());
    PUSHED.with(|d| d.borrow_mut().clear());
    ERR.with(|d| *d.borrow_mut() = None);
    VALUES.with(|d| d.borrow_mut().clear());
    LOGS.with(|d| d.borrow_mut().clear());
    CONFIGURED.with(|c| c.set(0));
}

/// How often the trait's `configure` ran on this thread since `reset`.
pub fn configured() -> usize {
    CONFIGURED.with(std::cell::Cell::get)
}
pub fn seen() -> Vec<Seen> {
    SEEN.with(|d| d.borrow().clone())
}
pub fn draws() -> Vec<Draw> {
    DRAWS.with(|d| d.borrow().clone())
}
pub fn pushed() -> Vec<Pushed> {
    PUSHED.with(|d| d.borrow().clone())
}
pub fn take_err() -> Option<String> {
    ERR.with(|d| d.borrow_mut().take())
}
pub fn values() -> Vec<Out> {
    VALUES.with(|d| d.borrow().clone())
}
pub fn logs() -> Vec<(u32, String)> {
    LOGS.with(|d| d.borrow().clone())
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

fn mix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

fn next_draw(host_ctx: UmbraToken, token: UmbraToken) -> u64 {
    DRAWS.with(|d| {
        let mut d = d.borrow_mut();
        let n = d.iter().filter(|x| x.token == token).count() as u64;
        let value = mix(token ^ n.wrapping_mul(0x1000_0001));
        d.push(Draw {
            host_ctx,
            token,
            value,
        });
        value
    })
}

unsafe extern "C" fn fake_log(_ctx: UmbraToken, level: UmbraLogLevel, msg: UmbraStr) -> UmbraCode {
    LOGS.with(|l| l.borrow_mut().push((level, text(msg))));
    UMBRA_OK
}
unsafe extern "C" fn fake_rng_u64(ctx: UmbraToken, rng: UmbraToken, out: *mut u64) -> UmbraCode {
    // SAFETY: the SDK passes a valid out pointer.
    unsafe { *out = next_draw(ctx, rng) };
    UMBRA_OK
}
unsafe extern "C" fn fake_rng_f64(ctx: UmbraToken, rng: UmbraToken, out: *mut f64) -> UmbraCode {
    let v = next_draw(ctx, rng);
    // SAFETY: the SDK passes a valid out pointer.
    unsafe { *out = (v >> 11) as f64 / (1u64 << 53) as f64 };
    UMBRA_OK
}

pub fn host_api() -> UmbraHostApi {
    UmbraHostApi {
        abi_version: UMBRA_ABI_VERSION,
        struct_size: size_of::<UmbraHostApi>() as u32,
        host_ctx: HOST_CTX,
        log: Some(fake_log),
        rng_next_u64: Some(fake_rng_u64),
        rng_next_f64: Some(fake_rng_f64),
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
        ctx: SINK_CTX,
        set: Some(err_set),
    }
}

unsafe extern "C" fn intent_push(
    _ctx: UmbraToken,
    item: usize,
    i: *const UmbraIntent,
) -> UmbraCode {
    // SAFETY: the SDK passes a valid intent for the duration of the call.
    let i = unsafe { &*i };
    PUSHED.with(|p| {
        p.borrow_mut().push(Pushed {
            item,
            kind: i.kind,
            position: [i.position.x, i.position.y, i.position.z],
            value: i.value,
            entity: i.entity,
            id: text(i.id),
            payload: text(i.payload),
            abi_version: i.abi_version,
            struct_size: i.struct_size,
        });
    });
    UMBRA_OK
}
pub fn intent_sink() -> UmbraIntentSink {
    UmbraIntentSink {
        abi_version: UMBRA_ABI_VERSION,
        struct_size: size_of::<UmbraIntentSink>() as u32,
        ctx: SINK_CTX,
        push: Some(intent_push),
    }
}

unsafe extern "C" fn value_set(_ctx: UmbraToken, v: *const UmbraValue) -> UmbraCode {
    let out = if v.is_null() {
        Out::Unset
    } else {
        // SAFETY: non-null, valid for the call.
        let v = unsafe { &*v };
        match v.ty {
            umbra_sdk::abi::UMBRA_VALUE_BOOL => Out::Bool(v.boolean),
            umbra_sdk::abi::UMBRA_VALUE_NUMBER => Out::Number(v.number),
            umbra_sdk::abi::UMBRA_VALUE_INTEGER => Out::Integer(v.integer),
            umbra_sdk::abi::UMBRA_VALUE_STRING => Out::Text(text(v.string)),
            umbra_sdk::abi::UMBRA_VALUE_ENTITY => Out::Entity(v.entity),
            _ => Out::Position([v.position.x, v.position.y, v.position.z]),
        }
    };
    VALUES.with(|l| l.borrow_mut().push(out));
    UMBRA_OK
}
pub fn value_sink() -> UmbraValueSink {
    UmbraValueSink {
        abi_version: UMBRA_ABI_VERSION,
        struct_size: size_of::<UmbraValueSink>() as u32,
        ctx: SINK_CTX,
        set: Some(value_set),
    }
}

// ---------------------------------------------------------------- calling the plugin

/// The descriptor after the entry call with the fake host.
pub fn descriptor() -> &'static UmbraPluginDescriptor {
    let host = Box::leak(Box::new(host_api()));
    // SAFETY: `host` is a leaked, well-formed host API (process lifetime).
    let ptr = unsafe { umbra_plugin_entry(host) };
    assert!(!ptr.is_null(), "entry returned null");
    // SAFETY: the SDK's descriptor is a process-lifetime static.
    unsafe { &*ptr }
}

pub fn capability(index: usize) -> &'static UmbraCapability {
    let d = descriptor();
    assert!(index < d.capability_count, "capability {index}");
    // SAFETY: bounds checked above; the array lives as long as the descriptor.
    unsafe { &*d.capabilities.add(index) }
}

pub fn vtable() -> &'static UmbraBehaviorVTable {
    // SAFETY: capability 0 is a behavior capability whose vtable is a static.
    unsafe { &*(capability(0).vtable.cast::<UmbraBehaviorVTable>()) }
}

pub fn raw<T>(r: &T) -> *const c_void {
    (r as *const T).cast()
}

/// `configure` result: code, handle, error message.
pub fn configure(params: &str) -> (UmbraCode, *mut UmbraBehavior, Option<String>) {
    let err = err_sink();
    let mut out: *mut UmbraBehavior = std::ptr::null_mut();
    let f = vtable().configure.expect("configure");
    // SAFETY: valid params, out and sink for the call.
    let code = unsafe { f(s(params), &mut out, &err) };
    (code, out, take_err())
}

pub fn instantiate(b: *mut UmbraBehavior) -> (UmbraCode, *mut UmbraInstance, Option<String>) {
    let err = err_sink();
    let mut out: *mut UmbraInstance = std::ptr::null_mut();
    let f = vtable().instantiate.expect("instantiate");
    // SAFETY: handle from `configure`.
    let code = unsafe { f(b, 30, &mut out, &err) };
    (code, out, take_err())
}

/// An owned batch under construction: entity views, items, statuses.
pub struct Rig {
    pub views: Vec<UmbraEntityView>,
    pub items: Vec<UmbraBehaviorItem>,
    pub statuses: Vec<u32>,
    pub sink: UmbraIntentSink,
}

pub fn view(id: u64) -> UmbraEntityView {
    UmbraEntityView {
        abi_version: UMBRA_ABI_VERSION,
        struct_size: size_of::<UmbraEntityView>() as u32,
        id,
        position: UmbraVec3 {
            x: 1.0,
            y: 2.0,
            z: 3.0,
        },
        velocity: UmbraVec3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
        orientation: UmbraQuat {
            w: 1.0,
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
        force: UmbraStr {
            ptr: std::ptr::null(),
            len: 0,
        },
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

impl Rig {
    /// One item per `(entity id, rng token)`, each with its own configured behavior and instance.
    pub fn new(members: &[(u64, UmbraToken)], params: &str) -> Self {
        let views: Vec<_> = members.iter().map(|(id, _)| view(*id)).collect();
        let mut items = Vec::new();
        for (n, (_, token)) in members.iter().enumerate() {
            let (code, b, _) = configure(params);
            assert_eq!(code, UMBRA_OK);
            let (code, i, _) = instantiate(b);
            assert_eq!(code, UMBRA_OK);
            items.push(UmbraBehaviorItem {
                abi_version: UMBRA_ABI_VERSION,
                struct_size: size_of::<UmbraBehaviorItem>() as u32,
                behavior: b,
                instance: i,
                entity: std::ptr::null(),
                rng: *token,
            });
            let _ = n;
        }
        let mut rig = Rig {
            views,
            items,
            statuses: vec![0; members.len()],
            sink: intent_sink(),
        };
        rig.relink();
        rig
    }

    /// Points each item at its view (call after moving the rig).
    pub fn relink(&mut self) {
        for (item, v) in self.items.iter_mut().zip(self.views.iter()) {
            item.entity = v;
        }
    }

    pub fn batch(&mut self, tick: u64) -> UmbraBehaviorBatch {
        self.relink();
        self.statuses.fill(0);
        UmbraBehaviorBatch {
            abi_version: UMBRA_ABI_VERSION,
            struct_size: size_of::<UmbraBehaviorBatch>() as u32,
            tick,
            rate_hz: 30,
            reserved_0: 0,
            items: self.items.as_ptr(),
            count: self.items.len(),
            statuses: self.statuses.as_mut_ptr(),
            intents: &self.sink,
        }
    }

    /// Runs `tick_batch` over the whole rig.
    pub fn tick(&mut self, tick: u64) -> (UmbraCode, Option<String>) {
        let b = self.batch(tick);
        call_tick(&b)
    }
}

pub fn call_tick(b: &UmbraBehaviorBatch) -> (UmbraCode, Option<String>) {
    let err = err_sink();
    let f = vtable().tick_batch.expect("tick_batch");
    // SAFETY: the batch is valid for the call except where a test forges it on purpose; the SDK must
    // reject that before reading.
    let code = unsafe { f(b, &err) };
    (code, take_err())
}
