//! Conversions of the ABI 0.2 kinds (ADR 0050): motion model, sensor, condition and action, and their
//! per-call services (terrain, blackboard) and sinks (contacts).
//!
//! Host-written input (the step, sense and leaf call records and everything they point to) gets every
//! reader check of ADR 0019 before any trait call: null and alignment, the header rule (the structs new
//! in ABI 0.2 must declare minor 2 or newer), array strides equal to the SDK's own size, UTF-8 strings,
//! known intent kinds and ownership codes, non-zero entity ids, a non-zero tick rate, and every service
//! entry present. Anything else is `UMBRA_ERR_INVALID_ARGUMENT` naming the field.
//!
//! Service calls go only through validated copies (`TerrainSvc`, `BlackboardSvc`, `ContactSink`) that
//! the safe types hold for the duration of their call (they borrow the call, REQ-P-11). Structs the SDK
//! writes during these calls (contacts, values and the value sink of a blackboard read, intents of an
//! action) carry the minor of the capability's kind and the SDK's own size (ADR 0033).
//!
//! A blackboard read is answered through a value sink the SDK passes to the host's `get`. Its token is
//! a fresh number per read, never a pointer: the SDK keeps one entry per read in flight (keyed by its
//! token, removed when the read ends, also on an error or a panic), and the sink's callback accepts
//! exactly one report for an existing, unanswered entry and refuses anything else (`UMBRA_ERR_STATE`).
//! A stale or forged token is never dereferenced, and reads on several threads at once (two engines in
//! one process, in-process tests) do not depend on the host serialising calls (RR-B1).
#![expect(
    unsafe_code,
    reason = "reads host-written ABI 0.2 call records and calls the host's services (ADR 0050)"
)]

use std::collections::BTreeMap;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};

use crate::abi::{
    UMBRA_ERR_INVALID_ARGUMENT, UMBRA_ERR_PANIC, UMBRA_ERR_STATE, UMBRA_INTENT_CUSTOM,
    UMBRA_INTENT_DESPAWN, UMBRA_INTENT_FIRE, UMBRA_INTENT_FOLLOW_ROUTE, UMBRA_INTENT_HOLD,
    UMBRA_INTENT_MOVE_TO, UMBRA_INTENT_SET_HEADING, UMBRA_INTENT_SET_SPEED, UMBRA_INTENT_SPAWN,
    UMBRA_OK, UMBRA_OWNERSHIP_OWNED, UMBRA_OWNERSHIP_REMOTE, UmbraBlackboard, UmbraCode,
    UmbraContact, UmbraContactSink, UmbraIntent, UmbraKinematicState, UmbraLeafCall,
    UmbraMotionStep, UmbraQuat, UmbraSenseCall, UmbraStr, UmbraTarget, UmbraTerrain, UmbraToken,
    UmbraValue, UmbraValueSink, UmbraVec3,
};
use crate::behavior::{Contact, Intent, Value};
use crate::error::Error;
use crate::guard::catch;
use crate::minor::{SENSOR_MINOR, abi_version};
use crate::motion::KinematicState;
use crate::sensor::{Ownership, TargetView};

use super::{
    Headed, ViewIn, checked_ref, element_ok, malformed, size_of_u32, slice_from, str_from,
    umbra_str, umbra_vec3, value_in, value_out, vec3, view_in,
};

/// The minor that defines the structs of the ABI 0.2 kinds (ADR 0050).
const DEFINED_IN: u32 = 2;

macro_rules! kinds_headed {
    ($($t:ident),* $(,)?) => {
        $(impl Headed for $t {
            const FIRST_MINOR: u32 = DEFINED_IN;
        })*
    };
}
kinds_headed!(
    UmbraKinematicState,
    UmbraMotionStep,
    UmbraTerrain,
    UmbraTarget,
    UmbraSenseCall,
    UmbraContactSink,
    UmbraLeafCall,
    UmbraBlackboard,
);
// Read in the intents array of a motion step (host-written, defined in minor 0).
impl Headed for UmbraIntent {}

fn quat(q: UmbraQuat) -> [f64; 4] {
    [q.w, q.x, q.y, q.z]
}

fn umbra_quat(q: [f64; 4]) -> UmbraQuat {
    UmbraQuat {
        w: q[0],
        x: q[1],
        y: q[2],
        z: q[3],
    }
}

fn state_of(s: &UmbraKinematicState) -> KinematicState {
    KinematicState {
        position: vec3(s.position),
        velocity: vec3(s.velocity),
        orientation: quat(s.orientation),
    }
}

/// The path of a field for an error message: a prefix and, for an array element, its index. Formatted
/// only when an error is built, so a well-formed call does not allocate for it.
#[derive(Clone, Copy)]
struct At<'s>(&'s str, Option<usize>);

impl fmt::Display for At<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.1 {
            Some(n) => write!(f, "{}[{n}]", self.0),
            None => f.write_str(self.0),
        }
    }
}

/// The non-zero tick rate of a call record or an `instantiate` call.
pub(crate) fn rate(rate_hz: u32, what: &str) -> Result<u32, Error> {
    if rate_hz == 0 {
        return Err(malformed(&format!("{what}.rate_hz"), "0"));
    }
    Ok(rate_hz)
}

/// `count` headed elements at `ptr`, each with the SDK's own size (the host writes arrays in the
/// plugin's minor; `header` reads an element's `abi_version` and `struct_size`), or `None`.
///
/// # Safety
/// When `count > 0` and `ptr` is non-null and aligned, `count` elements at `ptr` stay readable for
/// `'a` (the ABI contract of every host-written array).
unsafe fn headed_slice<'a, T: Headed>(
    ptr: *const T,
    count: usize,
    header: fn(&T) -> (u32, u32),
) -> Option<&'a [T]> {
    // SAFETY: forwarded from this function's contract; every headed struct is valid for any bits.
    let items = unsafe { slice_from(ptr, count) }?;
    items
        .iter()
        .all(|e| {
            let (v, s) = header(e);
            element_ok::<T>(v, s)
        })
        .then_some(items)
}

// ---------------------------------------------------------------- terrain

type HeightFn = unsafe extern "C" fn(UmbraToken, *const UmbraVec3, *mut f64) -> UmbraCode;
type NormalFn = unsafe extern "C" fn(UmbraToken, *const UmbraVec3, *mut UmbraVec3) -> UmbraCode;
type LosFn =
    unsafe extern "C" fn(UmbraToken, *const UmbraVec3, *const UmbraVec3, *mut u8) -> UmbraCode;

/// A validated copy of the terrain service of one call.
#[derive(Clone, Copy, Debug)]
pub(crate) struct TerrainSvc {
    ctx: UmbraToken,
    height_at: HeightFn,
    normal_at: NormalFn,
    line_of_sight: LosFn,
}

impl TerrainSvc {
    /// Validates the terrain service `p` of field `what`.
    ///
    /// # Safety
    /// As [`checked_ref`].
    unsafe fn from_raw(p: *const UmbraTerrain, what: &str) -> Result<Self, Error> {
        // SAFETY: forwarded from this function's contract.
        let t = unsafe { checked_ref(p) }.ok_or_else(|| malformed(what, "header or pointer"))?;
        let entry = |name: &str| malformed(&format!("{what}.{name}"), "null");
        Ok(Self {
            ctx: t.ctx,
            height_at: t.height_at.ok_or_else(|| entry("height_at"))?,
            normal_at: t.normal_at.ok_or_else(|| entry("normal_at"))?,
            line_of_sight: t.line_of_sight.ok_or_else(|| entry("line_of_sight"))?,
        })
    }

    /// The terrain height under `p`, or the host's refusal code.
    pub(crate) fn height_at(&self, p: [f64; 3]) -> Result<f64, UmbraCode> {
        let at = umbra_vec3(p);
        let mut out = 0.0_f64;
        // SAFETY: `height_at` comes from the service validated for the current call (the safe
        // `Terrain` holding this copy borrows the call and cannot leave its thread); both pointers are
        // live locals for the call; host callbacks cannot unwind.
        let code = unsafe { (self.height_at)(self.ctx, &raw const at, &raw mut out) };
        if code == UMBRA_OK { Ok(out) } else { Err(code) }
    }

    /// The terrain normal (ENU) under `p`, or the host's refusal code.
    pub(crate) fn normal_at(&self, p: [f64; 3]) -> Result<[f64; 3], UmbraCode> {
        let at = umbra_vec3(p);
        let mut out = umbra_vec3([0.0; 3]);
        // SAFETY: as in `height_at`.
        let code = unsafe { (self.normal_at)(self.ctx, &raw const at, &raw mut out) };
        if code == UMBRA_OK {
            Ok(vec3(out))
        } else {
            Err(code)
        }
    }

    /// Whether `b` is visible from `a`; `Ok(None)` when the host answered neither 0 nor 1.
    pub(crate) fn line_of_sight(
        &self,
        a: [f64; 3],
        b: [f64; 3],
    ) -> Result<Option<bool>, UmbraCode> {
        let (from, to) = (umbra_vec3(a), umbra_vec3(b));
        let mut out = u8::MAX;
        // SAFETY: as in `height_at`.
        let code =
            unsafe { (self.line_of_sight)(self.ctx, &raw const from, &raw const to, &raw mut out) };
        if code != UMBRA_OK {
            return Err(code);
        }
        Ok(match out {
            0 => Some(false),
            1 => Some(true),
            _ => None,
        })
    }
}

// ---------------------------------------------------------------- blackboard

type GetFn = unsafe extern "C" fn(UmbraToken, UmbraStr, *const UmbraValueSink) -> UmbraCode;
type SetFn = unsafe extern "C" fn(UmbraToken, UmbraStr, *const UmbraValue) -> UmbraCode;

/// Why a blackboard read failed.
#[derive(Debug)]
pub(crate) enum GetError {
    /// The host's `get` returned this code.
    Refused(UmbraCode),
    /// The host answered `UMBRA_OK` without exactly one well-formed report.
    Malformed(String),
}

/// A validated copy of the blackboard service of one call, with the minor of the capability's kind.
#[derive(Clone, Copy, Debug)]
pub(crate) struct BlackboardSvc {
    ctx: UmbraToken,
    get: GetFn,
    set: SetFn,
    minor: u32,
}

/// The answer of one read: `None` until the host reported, then the value or why it was malformed.
type Answer = Option<Result<Option<Value>, String>>;

/// Every blackboard read in flight of this image, by token. A `static` with a `Mutex`, not a
/// thread-local (ASM-142); never locked while host or plugin code runs, except inside the sink's own
/// callback, which calls neither.
static PENDING: Mutex<BTreeMap<UmbraToken, Answer>> = Mutex::new(BTreeMap::new());
/// The token of the next read.
static NEXT_TOKEN: AtomicU64 = AtomicU64::new(1);

fn pending() -> std::sync::MutexGuard<'static, BTreeMap<UmbraToken, Answer>> {
    PENDING.lock().unwrap_or_else(PoisonError::into_inner)
}

/// One read in flight: its entry exists from `begin` until `finish` removes it, or until the drop on
/// a path that never reaches `finish` (an early return or a panic), so no entry outlives its read.
struct Read(UmbraToken);

impl Read {
    /// Registers a read under a fresh token (never 0, never one in flight).
    fn begin() -> Self {
        let mut map = pending();
        let mut token = NEXT_TOKEN.fetch_add(1, Ordering::Relaxed);
        while token == 0 || map.contains_key(&token) {
            token = NEXT_TOKEN.fetch_add(1, Ordering::Relaxed);
        }
        map.insert(token, None);
        Self(token)
    }

    /// Ends the read and returns its answer; a late report is refused from here on. The one removal on
    /// the normal path (the guard's drop is skipped, it has nothing left to do).
    fn finish(self) -> Answer {
        let answer = pending().remove(&self.0).flatten();
        std::mem::forget(self);
        answer
    }
}

impl Drop for Read {
    fn drop(&mut self) {
        pending().remove(&self.0);
    }
}

/// The `set` of the value sink the SDK passes to the host's `get`: accepts exactly one report for a
/// read in flight (`ctx` is its token), validated with the `BbValue` invariants.
unsafe extern "C" fn report(ctx: UmbraToken, value: *const UmbraValue) -> UmbraCode {
    catch(|| {
        let mut map = pending();
        let Some(answer) = map.get_mut(&ctx).filter(|a| a.is_none()) else {
            return UMBRA_ERR_STATE;
        };
        // SAFETY: the `UmbraValueSink::set` contract: `value` is null or a host-written value valid
        // for this callback, with its string; it is read only now that the token matched the read in
        // flight (a stale, foreign or forged token was refused above without reading it).
        let parsed = unsafe { value_in(value) };
        let code = if parsed.is_ok() {
            UMBRA_OK
        } else {
            UMBRA_ERR_INVALID_ARGUMENT
        };
        *answer = Some(parsed.map_err(|e| e.message().to_owned()));
        code
    })
    .unwrap_or(UMBRA_ERR_PANIC)
}

impl BlackboardSvc {
    /// Validates the blackboard service `p` of field `what` for a kind of minor `minor`.
    ///
    /// # Safety
    /// As [`checked_ref`].
    unsafe fn from_raw(p: *const UmbraBlackboard, what: &str, minor: u32) -> Result<Self, Error> {
        // SAFETY: forwarded from this function's contract.
        let b = unsafe { checked_ref(p) }.ok_or_else(|| malformed(what, "header or pointer"))?;
        let entry = |name: &str| malformed(&format!("{what}.{name}"), "null");
        Ok(Self {
            ctx: b.ctx,
            get: b.get.ok_or_else(|| entry("get"))?,
            set: b.set.ok_or_else(|| entry("set"))?,
            minor,
        })
    }

    /// Reads `key`: the value, `None` when unset, or why it failed.
    pub(crate) fn get(&self, key: &str) -> Result<Option<Value>, GetError> {
        let read = Read::begin();
        let sink = UmbraValueSink {
            abi_version: abi_version(self.minor),
            struct_size: size_of_u32::<UmbraValueSink>(),
            ctx: read.0,
            set: Some(report),
        };
        // SAFETY: `get` comes from the service validated for the current call (the safe
        // `BlackboardAccess` holding this copy borrows the call and cannot leave its thread); the key
        // and the sink are live for the call; host callbacks cannot unwind.
        let code = unsafe { (self.get)(self.ctx, umbra_str(key), &raw const sink) };
        let answer = read.finish();
        if code != UMBRA_OK {
            return Err(GetError::Refused(code));
        }
        match answer {
            Some(Ok(value)) => Ok(value),
            Some(Err(why)) => Err(GetError::Malformed(why)),
            None => Err(GetError::Malformed("the host reported no value".to_owned())),
        }
    }

    /// Writes `key` (`None` unsets it) in the kind's minor; returns the host's code.
    pub(crate) fn set(&self, key: &str, value: Option<&Value>) -> UmbraCode {
        let raw = value.map(|v| value_out(v, self.minor));
        let ptr = raw.as_ref().map_or(std::ptr::null(), std::ptr::from_ref);
        // SAFETY: `set` comes from the service validated for the current call; the key and `raw`
        // (with the string it borrows from `value`) live across the call, and the host copies them;
        // host callbacks cannot unwind.
        unsafe { (self.set)(self.ctx, umbra_str(key), ptr) }
    }
}

// ---------------------------------------------------------------- contacts

type ContactPushFn = unsafe extern "C" fn(UmbraToken, *const UmbraContact) -> UmbraCode;

/// A validated copy of the contact sink of one `sense` call.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ContactSink {
    ctx: UmbraToken,
    push: ContactPushFn,
}

impl ContactSink {
    /// Validates the contact sink of a `sense` call.
    ///
    /// # Safety
    /// As [`checked_ref`].
    pub(crate) unsafe fn from_raw(p: *const UmbraContactSink) -> Result<Self, Error> {
        // SAFETY: forwarded from this function's contract.
        let s = unsafe { checked_ref(p) }.ok_or_else(|| malformed("sink", "header or pointer"))?;
        Ok(Self {
            ctx: s.ctx,
            push: s.push.ok_or_else(|| malformed("sink.push", "null"))?,
        })
    }

    /// Pushes `contact` in the sensor kind's minor; returns the host's code.
    pub(crate) fn push(&self, contact: &Contact<'_>) -> UmbraCode {
        let raw = UmbraContact {
            abi_version: abi_version(SENSOR_MINOR),
            struct_size: size_of_u32::<UmbraContact>(),
            entity: contact.entity,
            position: umbra_vec3(contact.position),
            velocity: umbra_vec3(contact.velocity),
            force: umbra_str(contact.force.unwrap_or("")),
            range_m: contact.range_m,
        };
        // SAFETY: `push` comes from the sink validated for the current `sense` call (the `Contacts`
        // holding this copy borrows the call); `raw` and the string it borrows live across the call,
        // and the host copies them; host callbacks cannot unwind.
        unsafe { (self.push)(self.ctx, &raw const raw) }
    }
}

// ---------------------------------------------------------------- intents (host-written)

/// One host-written intent of a motion step (ADR 0016 table), its strings borrowed for `'a`.
///
/// # Safety
/// The strings of `i` stay readable for `'a`.
unsafe fn intent_in<'a>(i: &UmbraIntent, what: At<'_>) -> Result<Intent<'a>, Error> {
    let text = |s: UmbraStr, field: &str| {
        // SAFETY: a string of the intent, readable for `'a` by this function's contract.
        unsafe { str_from(s) }.ok_or_else(|| malformed(&format!("{what}.{field}"), "UTF-8"))
    };
    Ok(match i.kind {
        UMBRA_INTENT_MOVE_TO => Intent::MoveTo(vec3(i.position)),
        UMBRA_INTENT_FOLLOW_ROUTE => Intent::FollowRoute,
        UMBRA_INTENT_SET_SPEED => Intent::SetSpeed(i.value),
        UMBRA_INTENT_SET_HEADING => Intent::SetHeading(i.value),
        UMBRA_INTENT_HOLD => Intent::Hold,
        UMBRA_INTENT_FIRE => Intent::Fire {
            target: i.entity,
            weapon: text(i.id, "id")?,
        },
        UMBRA_INTENT_SPAWN => Intent::Spawn {
            template: text(i.id, "id")?,
            position: vec3(i.position),
            heading: i.value,
        },
        UMBRA_INTENT_DESPAWN => Intent::Despawn,
        UMBRA_INTENT_CUSTOM => Intent::Custom {
            kind: text(i.id, "id")?,
            payload_json: text(i.payload, "payload")?,
        },
        other => {
            return Err(malformed(
                &format!("{what}.kind"),
                &format!("unknown kind {other}"),
            ));
        }
    })
}

// ---------------------------------------------------------------- the motion step

/// A validated motion step record.
pub(crate) struct StepIn<'a> {
    pub(crate) tick: u64,
    pub(crate) rate_hz: u32,
    pub(crate) dt: f64,
    pub(crate) state: KinematicState,
    pub(crate) intents: Vec<Intent<'a>>,
    pub(crate) terrain: TerrainSvc,
}

/// Validates and reads a motion step record and everything it points to.
///
/// # Safety
/// `step` is null or a host-written record valid for the call `'a` (ADR 0050), with every pointer in
/// it readable for `'a` with the counts it states.
pub(crate) unsafe fn step_in<'a>(step: *const UmbraMotionStep) -> Result<StepIn<'a>, Error> {
    // SAFETY: forwarded from this function's contract.
    let s = unsafe { checked_ref(step) }.ok_or_else(|| malformed("step", "header or pointer"))?;
    let rate_hz = rate(s.rate_hz, "step")?;
    if !s.dt.is_finite() {
        return Err(malformed("step.dt", "not finite"));
    }
    // SAFETY: the state pointer of the record, readable for `'a`.
    let state = unsafe { checked_ref(s.state) }
        .ok_or_else(|| malformed("step.state", "header or pointer"))?;
    // SAFETY: the intent array of the record, `intent_count` intents readable for `'a`.
    let raw = unsafe {
        headed_slice(s.intents, s.intent_count, |i| {
            (i.abi_version, i.struct_size)
        })
    }
    .ok_or_else(|| malformed("step.intents", "null, misaligned, too long or a bad header"))?;
    let intents = raw
        .iter()
        .enumerate()
        // SAFETY: the strings of the record's intents are readable for `'a`.
        .map(|(n, i)| unsafe { intent_in(i, At("step.intents", Some(n))) })
        .collect::<Result<Vec<_>, _>>()?;
    // SAFETY: the terrain pointer of the record, readable for `'a`.
    let terrain = unsafe { TerrainSvc::from_raw(s.terrain, "step.terrain") }?;
    Ok(StepIn {
        tick: s.tick,
        rate_hz,
        dt: s.dt,
        state: state_of(state),
        intents,
        terrain,
    })
}

/// The host-owned `out` of a motion step, checked before the trait runs and written after it.
pub(crate) struct StateOut(*mut UmbraKinematicState);

/// Checks the `out` struct of a motion step (non-null, aligned, header).
///
/// # Safety
/// `out` is null or points at a host-owned `UmbraKinematicState` writable for the call (ADR 0050).
pub(crate) unsafe fn state_out(out: *mut UmbraKinematicState) -> Result<StateOut, Error> {
    // SAFETY: forwarded from this function's contract (readable, as it is writable); the reference
    // ends at once.
    if unsafe { checked_ref(out.cast_const()) }.is_none() {
        return Err(malformed("out", "header or pointer"));
    }
    Ok(StateOut(out))
}

impl StateOut {
    /// Writes the result's fields; the header stays the host's.
    pub(crate) fn write(self, state: &KinematicState) {
        let out = self.0;
        // SAFETY: `state_out` checked `out` non-null, aligned and with a header of at least the SDK's
        // size; it is host-owned and writable during this call (the vtable contract), and no
        // reference to it is live (the check's reference ended).
        unsafe {
            (&raw mut (*out).position).write(umbra_vec3(state.position));
            (&raw mut (*out).velocity).write(umbra_vec3(state.velocity));
            (&raw mut (*out).orientation).write(umbra_quat(state.orientation));
        }
    }
}

// ---------------------------------------------------------------- the sense call

/// A validated sense call record.
pub(crate) struct SenseIn<'a> {
    pub(crate) tick: u64,
    pub(crate) rate_hz: u32,
    pub(crate) rng: UmbraToken,
    pub(crate) observer: TargetView<'a>,
    pub(crate) candidates: Vec<TargetView<'a>>,
    pub(crate) terrain: TerrainSvc,
}

/// One target, its force string borrowed for `'a`.
///
/// # Safety
/// The force string of `t` stays readable for `'a`.
unsafe fn target_in<'a>(t: &UmbraTarget, what: At<'_>) -> Result<TargetView<'a>, Error> {
    if t.id == 0 {
        return Err(malformed(&format!("{what}.id"), "0"));
    }
    let ownership = match t.ownership {
        UMBRA_OWNERSHIP_OWNED => Ownership::Owned,
        UMBRA_OWNERSHIP_REMOTE => Ownership::Remote,
        other => {
            return Err(malformed(
                &format!("{what}.ownership"),
                &format!("unknown code {other}"),
            ));
        }
    };
    // SAFETY: the target's string, readable for `'a` by this function's contract.
    let force =
        unsafe { str_from(t.force) }.ok_or_else(|| malformed(&format!("{what}.force"), "UTF-8"))?;
    Ok(TargetView {
        id: t.id,
        state: KinematicState {
            position: vec3(t.position),
            velocity: vec3(t.velocity),
            orientation: quat(t.orientation),
        },
        force: (!force.is_empty()).then_some(force),
        ownership,
    })
}

/// Validates and reads a sense call record and everything it points to.
///
/// # Safety
/// `call` is null or a host-written record valid for the call `'a` (ADR 0050), with every pointer in
/// it readable for `'a` with the counts it states.
pub(crate) unsafe fn sense_in<'a>(call: *const UmbraSenseCall) -> Result<SenseIn<'a>, Error> {
    // SAFETY: forwarded from this function's contract.
    let c = unsafe { checked_ref(call) }.ok_or_else(|| malformed("call", "header or pointer"))?;
    let rate_hz = rate(c.rate_hz, "call")?;
    // SAFETY: the observer pointer of the record, readable for `'a`.
    let observer = unsafe { checked_ref(c.observer) }
        .ok_or_else(|| malformed("call.observer", "header or pointer"))?;
    // SAFETY: as above; its force string is readable for `'a`.
    let observer = unsafe { target_in(observer, At("call.observer", None)) }?;
    // SAFETY: the candidate array of the record, `candidate_count` targets readable for `'a`.
    let raw = unsafe {
        headed_slice(c.candidates, c.candidate_count, |t| {
            (t.abi_version, t.struct_size)
        })
    }
    .ok_or_else(|| {
        malformed(
            "call.candidates",
            "null, misaligned, too long or a bad header",
        )
    })?;
    let candidates = raw
        .iter()
        .enumerate()
        // SAFETY: the force strings of the candidates are readable for `'a`.
        .map(|(n, t)| unsafe { target_in(t, At("call.candidates", Some(n))) })
        .collect::<Result<Vec<_>, _>>()?;
    // SAFETY: the terrain pointer of the record, readable for `'a`.
    let terrain = unsafe { TerrainSvc::from_raw(c.terrain, "call.terrain") }?;
    Ok(SenseIn {
        tick: c.tick,
        rate_hz,
        rng: c.rng,
        observer,
        candidates,
        terrain,
    })
}

// ---------------------------------------------------------------- the leaf call

/// A validated leaf call record (condition `check`, action `tick`).
pub(crate) struct LeafIn<'a> {
    pub(crate) tick: u64,
    pub(crate) rate_hz: u32,
    pub(crate) rng: UmbraToken,
    pub(crate) view: ViewIn<'a>,
    pub(crate) blackboard: BlackboardSvc,
}

/// Validates and reads a leaf call record for a kind of minor `minor`.
///
/// # Safety
/// `call` is null or a host-written record valid for the call `'a` (ADR 0050), with every pointer in
/// it readable for `'a`.
pub(crate) unsafe fn leaf_in<'a>(
    call: *const UmbraLeafCall,
    minor: u32,
) -> Result<LeafIn<'a>, Error> {
    // SAFETY: forwarded from this function's contract.
    let c = unsafe { checked_ref(call) }.ok_or_else(|| malformed("call", "header or pointer"))?;
    let rate_hz = rate(c.rate_hz, "call")?;
    // SAFETY: the entity view of the record, readable for `'a` with its arrays and strings.
    let view = unsafe { view_in(c.entity) }
        .map_err(|e| Error::invalid(format!("call.{}", e.message())))?;
    // SAFETY: the blackboard pointer of the record, readable for `'a`.
    let blackboard = unsafe { BlackboardSvc::from_raw(c.blackboard, "call.blackboard", minor) }?;
    Ok(LeafIn {
        tick: c.tick,
        rate_hz,
        rng: c.rng,
        view,
        blackboard,
    })
}

#[cfg(test)]
mod tests {
    use std::mem::size_of;

    use super::{DEFINED_IN, Headed};
    use crate::abi::{
        UMBRA_ABI_VERSION_MAJOR, UmbraBlackboard, UmbraContactSink, UmbraKinematicState,
        UmbraLeafCall, UmbraMotionStep, UmbraSenseCall, UmbraTarget, UmbraTerrain,
    };
    use crate::convert::header_ok;

    fn refuses_minor_1_and_accepts_minor_2<T: Headed>() -> bool {
        let own = u32::try_from(size_of::<T>()).unwrap_or(u32::MAX);
        let packed = |minor: u32| (UMBRA_ABI_VERSION_MAJOR << 16) | minor;
        !header_ok::<T>(packed(DEFINED_IN - 1), own) && header_ok::<T>(packed(DEFINED_IN), own)
    }

    /// ADR 0050: the structs of the ABI 0.2 kinds do not exist before minor 2, so an older header is
    /// malformed.
    #[test]
    fn the_abi_0_2_structs_need_a_header_of_minor_2() {
        assert!(refuses_minor_1_and_accepts_minor_2::<UmbraKinematicState>());
        assert!(refuses_minor_1_and_accepts_minor_2::<UmbraMotionStep>());
        assert!(refuses_minor_1_and_accepts_minor_2::<UmbraTerrain>());
        assert!(refuses_minor_1_and_accepts_minor_2::<UmbraTarget>());
        assert!(refuses_minor_1_and_accepts_minor_2::<UmbraSenseCall>());
        assert!(refuses_minor_1_and_accepts_minor_2::<UmbraContactSink>());
        assert!(refuses_minor_1_and_accepts_minor_2::<UmbraLeafCall>());
        assert!(refuses_minor_1_and_accepts_minor_2::<UmbraBlackboard>());
    }
}
