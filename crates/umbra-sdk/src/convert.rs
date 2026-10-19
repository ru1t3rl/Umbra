//! Conversions between the C ABI and the safe SDK types, with every check ADR 0019 asks of a reader
//! (REQ-P-2, REQ-P-6, R-M2): own major, `struct_size` at least the minor-0 size, at most the SDK's own
//! size for a struct that declares a minor the SDK knows, a multiple of 8; non-null and aligned
//! pointers; `len <= isize::MAX` and `checked_mul` before any slice; UTF-8; known enum values and the
//! `BbValue` invariants. Structs the SDK writes carry the SDK's own `UMBRA_ABI_VERSION` and size.
#![expect(
    unsafe_code,
    reason = "turns host pointers into references after checking them (ADR 0019)"
)]

use std::mem::size_of;

use crate::abi::{
    UMBRA_ABI_VERSION, UMBRA_ABI_VERSION_MAJOR, UMBRA_ABI_VERSION_MINOR, UMBRA_INTENT_CUSTOM,
    UMBRA_INTENT_DESPAWN, UMBRA_INTENT_FIRE, UMBRA_INTENT_FOLLOW_ROUTE, UMBRA_INTENT_HOLD,
    UMBRA_INTENT_MOVE_TO, UMBRA_INTENT_SET_HEADING, UMBRA_INTENT_SET_SPEED, UMBRA_INTENT_SPAWN,
    UMBRA_STATUS_FAILURE, UMBRA_STATUS_RUNNING, UMBRA_STATUS_SUCCESS, UMBRA_VALUE_BOOL,
    UMBRA_VALUE_ENTITY, UMBRA_VALUE_INTEGER, UMBRA_VALUE_NUMBER, UMBRA_VALUE_POSITION,
    UMBRA_VALUE_STRING, UmbraBehaviorBatch, UmbraBehaviorItem, UmbraCode, UmbraContact,
    UmbraEntityView, UmbraErrorSink, UmbraHostApi, UmbraIntent, UmbraIntentSink, UmbraStatus,
    UmbraStr, UmbraToken, UmbraValue, UmbraValueSink, UmbraVec3,
};
use crate::behavior::{Contact, Intent, Route, Status, Value};
use crate::error::Error;

// ---------------------------------------------------------------- headers

/// A struct that starts with the ABI header `abi_version: u32, struct_size: u32` (ADR 0019).
///
/// Every bit pattern is a valid value of an implementor (integers, floats, raw pointers, `u8`
/// booleans and nullable function pointers only), so reading one from host memory is never undefined
/// behavior once the memory is readable.
pub(crate) trait Headed: Sized {
    /// `sizeof` the struct at minor 0. The SDK is at minor 0, so this equals `size_of::<Self>()`; when
    /// the SDK appends fields, readers must only touch fields inside `struct_size`.
    const MIN_SIZE: usize = size_of::<Self>();
}

macro_rules! headed {
    ($($t:ident),* $(,)?) => { $(impl Headed for $t {})* };
}
headed!(
    UmbraErrorSink,
    UmbraHostApi,
    UmbraBehaviorBatch,
    UmbraBehaviorItem,
    UmbraEntityView,
    UmbraContact,
    UmbraIntentSink,
    UmbraValue,
    UmbraValueSink,
);

/// The ADR 0019 reader rule for one header.
pub(crate) fn header_ok<T: Headed>(abi_version: u32, struct_size: u32) -> bool {
    let Ok(size) = usize::try_from(struct_size) else {
        return false;
    };
    let newer_minor = abi_version & 0xFFFF > UMBRA_ABI_VERSION_MINOR;
    abi_version >> 16 == UMBRA_ABI_VERSION_MAJOR
        && size >= T::MIN_SIZE
        && size % 8 == 0
        && (newer_minor || size <= size_of::<T>())
}

/// `p` as a reference after the null, alignment and header checks; `None` if any fails.
///
/// # Safety
/// `p` is null or points at memory that stays readable for `'a`: at least the 8 header bytes, and
/// `struct_size` bytes when the header passes (the ABI contract of every host-written struct).
pub(crate) unsafe fn checked_ref<'a, T: Headed>(p: *const T) -> Option<&'a T> {
    if p.is_null() || !p.is_aligned() {
        return None;
    }
    // SAFETY: `p` is non-null and aligned for `T` (alignment 8, so also for `[u32; 2]`), and the
    // caller guarantees the 8 header bytes are readable; every headed struct starts with two `u32`.
    let [abi_version, struct_size] = unsafe { p.cast::<[u32; 2]>().read() };
    if !header_ok::<T>(abi_version, struct_size) {
        return None;
    }
    // SAFETY: the header says at least `MIN_SIZE == size_of::<T>()` bytes, which the caller
    // guarantees readable for `'a`; any bit pattern is a valid `T` (see `Headed`); the host does not
    // write to the struct during the call (ADR 0019: host-written input is read-only for the plugin).
    Some(unsafe { &*p })
}

/// As [`checked_ref`] for an array element: the host writes arrays in the plugin's minor (R-B3), so an
/// element's `struct_size` must equal the SDK's own `sizeof`, the stride the SDK indexes with.
fn element_ok<T: Headed>(abi_version: u32, struct_size: u32) -> bool {
    header_ok::<T>(abi_version, struct_size) && usize::try_from(struct_size) == Ok(size_of::<T>())
}

// ---------------------------------------------------------------- strings and slices

/// `isize::MAX` as a `usize`: the largest byte length of a slice.
const MAX_BYTES: usize = isize::MAX.unsigned_abs();

/// A borrowed `&str` as an [`UmbraStr`] (valid while `s` is).
pub(crate) fn umbra_str(s: &str) -> UmbraStr {
    UmbraStr {
        ptr: s.as_ptr(),
        len: s.len(),
    }
}

/// The text of `s`, or `None` for a null pointer with a non-zero length, a length above `isize::MAX`
/// or invalid UTF-8. `len == 0` is the empty string whatever `ptr` is.
///
/// # Safety
/// When `len > 0` and `ptr` is non-null, `len` bytes at `ptr` stay readable and unchanged for `'a`.
pub(crate) unsafe fn str_from<'a>(s: UmbraStr) -> Option<&'a str> {
    if s.len == 0 {
        return Some("");
    }
    if s.ptr.is_null() || s.len > MAX_BYTES {
        return None;
    }
    // SAFETY: non-null, `len <= isize::MAX`, readable for `'a` by the caller's contract; `u8` has
    // alignment 1.
    let bytes = unsafe { std::slice::from_raw_parts(s.ptr, s.len) };
    std::str::from_utf8(bytes).ok()
}

/// `len` elements at `ptr` as a slice, or `None` for a null pointer with `len > 0`, a misaligned
/// pointer or a byte size above `isize::MAX`.
///
/// # Safety
/// When `len > 0` and `ptr` is non-null and aligned, `len * size_of::<T>()` bytes at `ptr` stay
/// readable and unchanged for `'a`, and any bit pattern is a valid `T`.
unsafe fn slice_from<'a, T>(ptr: *const T, len: usize) -> Option<&'a [T]> {
    if len == 0 {
        return Some(&[]);
    }
    if ptr.is_null() || !ptr.is_aligned() {
        return None;
    }
    if len.checked_mul(size_of::<T>())? > MAX_BYTES {
        return None;
    }
    // SAFETY: non-null, aligned, total size checked against `isize::MAX`; readable for `'a` and valid
    // for any bit pattern by the caller's contract.
    Some(unsafe { std::slice::from_raw_parts(ptr, len) })
}

/// `Some(text)` for a non-empty string, `None` for the empty one ("none" in the ABI).
fn non_empty(text: &str) -> Option<&str> {
    (!text.is_empty()).then_some(text)
}

fn vec3(v: UmbraVec3) -> [f64; 3] {
    [v.x, v.y, v.z]
}

fn umbra_vec3(v: [f64; 3]) -> UmbraVec3 {
    UmbraVec3 {
        x: v[0],
        y: v[1],
        z: v[2],
    }
}

/// A `u8` boolean: 0 or 1, anything else is malformed (ADR 0019).
fn flag(v: u8) -> Option<bool> {
    match v {
        0 => Some(false),
        1 => Some(true),
        _ => None,
    }
}

// ---------------------------------------------------------------- the batch

/// A validated copy of the host's intent sink.
#[derive(Clone, Copy, Debug)]
pub(crate) struct IntentSink {
    pub(crate) ctx: UmbraToken,
    pub(crate) push: unsafe extern "C" fn(UmbraToken, usize, *const UmbraIntent) -> UmbraCode,
}

impl IntentSink {
    /// Pushes `intent` (in the SDK's own minor) for batch item `item`; returns the host's code.
    pub(crate) fn push(&self, item: usize, intent: &Intent<'_>) -> UmbraCode {
        let raw = intent_out(intent);
        // SAFETY: `push` comes from the sink validated by `batch_in` for the current call; `raw` and
        // the strings it borrows from `intent` live across the call, and the host copies them;
        // host callbacks cannot unwind.
        unsafe { (self.push)(self.ctx, item, &raw const raw) }
    }
}

/// A batch whose every pointer, header, string and array was checked (pass 1 of `tick_batch`).
pub(crate) struct BatchIn<'a> {
    pub(crate) tick: u64,
    pub(crate) rate_hz: u32,
    pub(crate) items: &'a [UmbraBehaviorItem],
    statuses: *mut UmbraStatus,
    pub(crate) sink: IntentSink,
}

impl BatchIn<'_> {
    /// Writes the status of item `index`.
    pub(crate) fn set_status(&self, index: usize, status: Status) {
        if index < self.items.len() {
            // SAFETY: `batch_in` checked `statuses` non-null and aligned for `count == items.len()`
            // elements. Its byte size `count * 4` is below `isize::MAX` because `items` already passed
            // `count * size_of::<UmbraBehaviorItem>() <= isize::MAX` and an item (40 bytes) is larger
            // than a status (4 bytes). The array is host-owned and writable during this call, written
            // only by the plugin (R-B2), and `index < count`.
            unsafe { self.statuses.add(index).write(status_code(status)) };
        }
    }
}

/// Validates a batch completely (every item, view, contact and string) before any trait call (R-M2).
///
/// # Safety
/// `batch` is null or a host-written batch valid for the call `'a` (ADR 0020): every pointer in it,
/// with the lengths it states, readable for `'a`, and `statuses` writable.
pub(crate) unsafe fn batch_in<'a>(batch: *const UmbraBehaviorBatch) -> Result<BatchIn<'a>, Error> {
    // SAFETY: forwarded from this function's contract.
    let b = unsafe { checked_ref(batch) }.ok_or_else(|| malformed("batch", "header or pointer"))?;
    // SAFETY: the sink pointer comes from the batch, valid for the call by this function's contract.
    let sink = unsafe { checked_ref(b.intents) }
        .ok_or_else(|| malformed("batch.intents", "header or pointer"))?;
    let push = sink
        .push
        .ok_or_else(|| malformed("batch.intents.push", "null"))?;
    // SAFETY: `items` holds `count` items, readable for the call; every bit pattern is valid.
    let items = unsafe { slice_from(b.items, b.count) }
        .ok_or_else(|| malformed("batch.items", "null, misaligned or too long"))?;
    if b.count > 0
        && (b.statuses.is_null()
            || !b.statuses.is_aligned()
            || b.count.checked_mul(size_of::<UmbraStatus>()).is_none())
    {
        return Err(malformed("batch.statuses", "null or misaligned"));
    }
    for (i, item) in items.iter().enumerate() {
        if !element_ok::<UmbraBehaviorItem>(item.abi_version, item.struct_size) {
            return Err(malformed(&format!("batch.items[{i}]"), "header"));
        }
        if item.instance.is_null() {
            return Err(malformed(&format!("batch.items[{i}].instance"), "null"));
        }
        // SAFETY: the view pointer comes from a batch item, valid for the call.
        unsafe { view_in(item.entity) }
            .map_err(|e| Error::invalid(format!("batch.items[{i}].{}", e.message())))?;
    }
    Ok(BatchIn {
        tick: b.tick,
        rate_hz: b.rate_hz,
        items,
        statuses: b.statuses,
        sink: IntentSink {
            ctx: sink.ctx,
            push,
        },
    })
}

fn malformed(field: &str, what: &str) -> Error {
    Error::invalid(format!("{field}: malformed ({what})"))
}

/// A validated entity view; contacts stay raw until [`contacts_into`] converts them.
pub(crate) struct ViewIn<'a> {
    pub(crate) id: u64,
    pub(crate) position: [f64; 3],
    pub(crate) velocity: [f64; 3],
    pub(crate) orientation: [f64; 4],
    pub(crate) force: Option<&'a str>,
    pub(crate) route: Option<Route<'a>>,
    contacts: &'a [UmbraContact],
}

/// Validates and reads one entity view.
///
/// # Safety
/// `view` is null or a host-written view valid for `'a`, with its arrays and strings.
pub(crate) unsafe fn view_in<'a>(view: *const UmbraEntityView) -> Result<ViewIn<'a>, Error> {
    // SAFETY: forwarded from this function's contract.
    let v = unsafe { checked_ref(view) }.ok_or_else(|| malformed("entity", "header or pointer"))?;
    if v.id == 0 {
        return Err(malformed("entity.id", "0"));
    }
    // SAFETY: a string of the view, valid for `'a`.
    let force = unsafe { str_from(v.force) }.ok_or_else(|| malformed("entity.force", "UTF-8"))?;
    let (Some(has_route), Some(complete)) = (flag(v.has_route), flag(v.route_complete)) else {
        return Err(malformed("entity.has_route", "boolean"));
    };
    // SAFETY: the route array of the view, `route_len` waypoints valid for `'a`.
    let waypoints = unsafe { slice_from(v.route, v.route_len) }
        .ok_or_else(|| malformed("entity.route", "null, misaligned or too long"))?;
    // SAFETY: the contact array of the view, `contact_count` contacts valid for `'a`.
    let contacts = unsafe { slice_from(v.contacts, v.contact_count) }
        .ok_or_else(|| malformed("entity.contacts", "null, misaligned or too long"))?;
    for (i, c) in contacts.iter().enumerate() {
        if !element_ok::<UmbraContact>(c.abi_version, c.struct_size) {
            return Err(malformed(&format!("entity.contacts[{i}]"), "header"));
        }
        // SAFETY: a string of a contact of the view, valid for `'a`.
        if unsafe { str_from(c.force) }.is_none() {
            return Err(malformed(&format!("entity.contacts[{i}].force"), "UTF-8"));
        }
    }
    Ok(ViewIn {
        id: v.id,
        position: vec3(v.position),
        velocity: vec3(v.velocity),
        orientation: [
            v.orientation.w,
            v.orientation.x,
            v.orientation.y,
            v.orientation.z,
        ],
        force: non_empty(force),
        route: has_route.then(|| Route::new(waypoints, v.route_next, complete)),
        contacts,
    })
}

impl<'a> ViewIn<'a> {
    /// Replaces `out` with the view's contacts (already validated by [`view_in`]).
    pub(crate) fn contacts_into(&self, out: &mut Vec<Contact<'a>>) {
        out.clear();
        out.extend(self.contacts.iter().map(|c| Contact {
            entity: c.entity,
            position: vec3(c.position),
            velocity: vec3(c.velocity),
            // SAFETY: the same string `view_in` checked, valid for `'a`.
            force: unsafe { str_from(c.force) }.and_then(non_empty),
            range_m: c.range_m,
        }));
    }
}

// ---------------------------------------------------------------- statuses and intents

/// The ABI number of a status (R-B2: never `UNSET`).
pub(crate) fn status_code(status: Status) -> UmbraStatus {
    match status {
        Status::Success => UMBRA_STATUS_SUCCESS,
        Status::Failure => UMBRA_STATUS_FAILURE,
        Status::Running => UMBRA_STATUS_RUNNING,
    }
}

/// An intent in the SDK's own minor, its strings borrowed from `intent` (ADR 0016 table).
pub(crate) fn intent_out(intent: &Intent<'_>) -> UmbraIntent {
    let empty = umbra_str("");
    let mut out = UmbraIntent {
        abi_version: UMBRA_ABI_VERSION,
        struct_size: size_of_u32::<UmbraIntent>(),
        kind: 0,
        reserved_0: 0,
        position: umbra_vec3([0.0; 3]),
        value: 0.0,
        entity: 0,
        id: empty,
        payload: empty,
    };
    match *intent {
        Intent::MoveTo(p) => {
            out.kind = UMBRA_INTENT_MOVE_TO;
            out.position = umbra_vec3(p);
        }
        Intent::FollowRoute => out.kind = UMBRA_INTENT_FOLLOW_ROUTE,
        Intent::SetSpeed(v) => {
            out.kind = UMBRA_INTENT_SET_SPEED;
            out.value = v;
        }
        Intent::SetHeading(v) => {
            out.kind = UMBRA_INTENT_SET_HEADING;
            out.value = v;
        }
        Intent::Hold => out.kind = UMBRA_INTENT_HOLD,
        Intent::Fire { target, weapon } => {
            out.kind = UMBRA_INTENT_FIRE;
            out.entity = target;
            out.id = umbra_str(weapon);
        }
        Intent::Spawn {
            template,
            position,
            heading,
        } => {
            out.kind = UMBRA_INTENT_SPAWN;
            out.id = umbra_str(template);
            out.position = umbra_vec3(position);
            out.value = heading;
        }
        Intent::Despawn => out.kind = UMBRA_INTENT_DESPAWN,
        Intent::Custom { kind, payload_json } => {
            out.kind = UMBRA_INTENT_CUSTOM;
            out.id = umbra_str(kind);
            out.payload = umbra_str(payload_json);
        }
    }
    out
}

/// `size_of::<T>()` as the `u32` of a `struct_size` field (every ABI struct is far below 4 GiB).
pub(crate) fn size_of_u32<T>() -> u32 {
    u32::try_from(size_of::<T>()).unwrap_or(u32::MAX)
}

// ---------------------------------------------------------------- values

/// A validated copy of the host's value sink.
#[derive(Clone, Copy)]
pub(crate) struct ValueSink {
    pub(crate) ctx: UmbraToken,
    pub(crate) set: unsafe extern "C" fn(UmbraToken, *const UmbraValue) -> UmbraCode,
}

impl ValueSink {
    /// Reports `value` (`None` = unset) to the host, once; returns the host's code.
    pub(crate) fn set(&self, value: Option<&Value>) -> UmbraCode {
        let raw = value.map(value_out);
        let ptr = raw.as_ref().map_or(std::ptr::null(), std::ptr::from_ref);
        // SAFETY: `set` comes from the sink validated by `value_sink` for the current call; `ptr` is
        // null or points at `raw`, which with the string it borrows from `value` lives across the
        // call; the host copies it; host callbacks cannot unwind.
        unsafe { (self.set)(self.ctx, ptr) }
    }
}

/// Validates the value sink of an `output` call.
///
/// # Safety
/// As [`checked_ref`].
pub(crate) unsafe fn value_sink(sink: *const UmbraValueSink) -> Option<ValueSink> {
    // SAFETY: forwarded from this function's contract.
    let s = unsafe { checked_ref(sink) }?;
    Some(ValueSink {
        ctx: s.ctx,
        set: s.set?,
    })
}

/// Reads an input value; null is "unset" (`Ok(None)`). Checks the `BbValue` invariants.
///
/// # Safety
/// `value` is null or a host-written value valid for the call, with its string.
pub(crate) unsafe fn value_in(value: *const UmbraValue) -> Result<Option<Value>, Error> {
    if value.is_null() {
        return Ok(None);
    }
    // SAFETY: forwarded from this function's contract.
    let v = unsafe { checked_ref(value) }.ok_or_else(|| malformed("value", "header or pointer"))?;
    let out = match v.ty {
        UMBRA_VALUE_BOOL => {
            Value::Bool(flag(v.boolean).ok_or_else(|| malformed("value.boolean", "not 0 or 1"))?)
        }
        UMBRA_VALUE_NUMBER if v.number.is_finite() => Value::Number(v.number),
        UMBRA_VALUE_INTEGER => Value::Integer(v.integer),
        UMBRA_VALUE_STRING => {
            // SAFETY: the value's string, valid for the call.
            let text =
                unsafe { str_from(v.string) }.ok_or_else(|| malformed("value.string", "UTF-8"))?;
            Value::String(text.to_owned())
        }
        UMBRA_VALUE_ENTITY if v.entity != 0 => Value::Entity(v.entity),
        UMBRA_VALUE_POSITION if vec3(v.position).iter().all(|c| c.is_finite()) => {
            Value::Position(vec3(v.position))
        }
        UMBRA_VALUE_NUMBER | UMBRA_VALUE_ENTITY | UMBRA_VALUE_POSITION => {
            return Err(malformed("value", "non-finite number or entity 0"));
        }
        other => return Err(malformed("value.ty", &format!("unknown type {other}"))),
    };
    Ok(Some(out))
}

/// A value in the SDK's own minor, its string borrowed from `value`.
pub(crate) fn value_out(value: &Value) -> UmbraValue {
    let mut out = UmbraValue {
        abi_version: UMBRA_ABI_VERSION,
        struct_size: size_of_u32::<UmbraValue>(),
        ty: 0,
        boolean: 0,
        reserved_0: [0; 3],
        number: 0.0,
        integer: 0,
        string: umbra_str(""),
        entity: 0,
        position: umbra_vec3([0.0; 3]),
    };
    match value {
        Value::Bool(b) => {
            out.ty = UMBRA_VALUE_BOOL;
            out.boolean = u8::from(*b);
        }
        Value::Number(n) => {
            out.ty = UMBRA_VALUE_NUMBER;
            out.number = *n;
        }
        Value::Integer(i) => {
            out.ty = UMBRA_VALUE_INTEGER;
            out.integer = *i;
        }
        Value::String(s) => {
            out.ty = UMBRA_VALUE_STRING;
            out.string = umbra_str(s);
        }
        Value::Entity(e) => {
            out.ty = UMBRA_VALUE_ENTITY;
            out.entity = *e;
        }
        Value::Position(p) => {
            out.ty = UMBRA_VALUE_POSITION;
            out.position = umbra_vec3(*p);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use std::mem::size_of;

    use super::{Headed, header_ok, str_from, umbra_str};
    use crate::abi::{
        UMBRA_ABI_VERSION, UmbraBehaviorBatch, UmbraBehaviorItem, UmbraContact, UmbraEntityView,
        UmbraErrorSink, UmbraHostApi, UmbraIntentSink, UmbraStr, UmbraValue, UmbraValueSink,
    };

    fn min_is_own<T: Headed>() -> bool {
        T::MIN_SIZE == size_of::<T>()
    }

    /// The SDK is at minor 0: every reader's minimum is its own size (see `Headed::MIN_SIZE`).
    #[test]
    fn min_size_is_own_size_at_minor_0() {
        assert!(min_is_own::<UmbraErrorSink>());
        assert!(min_is_own::<UmbraHostApi>());
        assert!(min_is_own::<UmbraBehaviorBatch>());
        assert!(min_is_own::<UmbraBehaviorItem>());
        assert!(min_is_own::<UmbraEntityView>());
        assert!(min_is_own::<UmbraContact>());
        assert!(min_is_own::<UmbraIntentSink>());
        assert!(min_is_own::<UmbraValue>());
        assert!(min_is_own::<UmbraValueSink>());
    }

    #[test]
    fn header_rule_of_adr_0019() {
        let own = u32::try_from(size_of::<UmbraValue>()).expect("small");
        assert!(header_ok::<UmbraValue>(UMBRA_ABI_VERSION, own));
        assert!(
            !header_ok::<UmbraValue>(UMBRA_ABI_VERSION, own - 8),
            "short"
        );
        assert!(
            !header_ok::<UmbraValue>(UMBRA_ABI_VERSION, own + 4),
            "not a multiple of 8"
        );
        assert!(
            !header_ok::<UmbraValue>(UMBRA_ABI_VERSION, own + 8),
            "larger than minor 0"
        );
        assert!(
            header_ok::<UmbraValue>(UMBRA_ABI_VERSION + 1, own + 8),
            "newer minor may grow"
        );
        assert!(!header_ok::<UmbraValue>(1 << 16, own), "other major");
    }

    #[test]
    fn strings_check_null_length_and_utf8() {
        let ok = "abc";
        // SAFETY: points at a live `&str`.
        assert_eq!(unsafe { str_from(umbra_str(ok)) }, Some("abc"));
        let empty = UmbraStr {
            ptr: std::ptr::null(),
            len: 0,
        };
        // SAFETY: `len == 0` is never read.
        assert_eq!(unsafe { str_from(empty) }, Some(""));
        let null = UmbraStr {
            ptr: std::ptr::null(),
            len: 3,
        };
        // SAFETY: a null pointer is refused before reading.
        assert_eq!(unsafe { str_from(null) }, None);
        let bad = [0xFF_u8, 0xFE];
        let bad = UmbraStr {
            ptr: bad.as_ptr(),
            len: 2,
        };
        // SAFETY: points at a live array of 2 bytes.
        assert_eq!(unsafe { str_from(bad) }, None);
        let huge = UmbraStr {
            ptr: ok.as_ptr(),
            len: usize::MAX,
        };
        // SAFETY: the length is refused before reading.
        assert_eq!(unsafe { str_from(huge) }, None);
    }
}
