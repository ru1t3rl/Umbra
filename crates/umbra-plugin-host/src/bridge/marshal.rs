//! Marshalling between the core types and the C ABI (ADR 0019 "Values", "Bounds", R-B3; ADR 0020
//! "Marshalling"; ADR 0016 intent table).
//!
//! **Host-written structs** (batch, items, entity views, contacts, values, sinks) are written in
//! the layout of the plugin's declared minor (R-B3). ABI v0 has one minor, so that is the host's
//! own layout; [`written_size`] is the one place a later minor adds its size table.
//!
//! **Plugin-written structs** (intents, output values) are read like the descriptor: pointer
//! non-null and 8-byte aligned, the major equal and the minor not newer than the host's,
//! `struct_size` within `[minor-0 size, size of the declared minor]` and a multiple of 8, only
//! `struct_size` bytes read, every value checked (finite numbers, non-zero entities, UTF-8, ADR
//! 0002 id grammar, JSON payloads).
//!
//! **Scratch.** The buffers of a `tick_batch` call live in the image's call state (guarded by the
//! per-image call lock) and are reused: after warm-up a batch call allocates nothing. Views point
//! into kernel data (forces, contacts' forces) and into the scratch's own route and contact arrays
//! only for the duration of the call; every buffer holding pointers is cleared after the call.
//! Routes are copied (core `Ecef` is never cast).

#![expect(
    unsafe_code,
    reason = "reads plugin-written intents and values; scratch holds call-scoped raw pointers"
)]

use std::mem::size_of;
use std::sync::Arc;

use umbra_core::{
    BbType, BbValue, ComponentId, Ecef, EntityId, EntityView, Intent, MAX_ID_LEN, MetersPerSecond,
    ParamValue, Radians, Status, TemplateId, Vec3, WeaponId,
};
use umbra_plugin_api::{
    UMBRA_ABI_VERSION, UMBRA_ABI_VERSION_MAJOR, UMBRA_ABI_VERSION_MINOR, UMBRA_INTENT_CUSTOM,
    UMBRA_INTENT_DESPAWN, UMBRA_INTENT_FIRE, UMBRA_INTENT_FOLLOW_ROUTE, UMBRA_INTENT_HOLD,
    UMBRA_INTENT_MOVE_TO, UMBRA_INTENT_SET_HEADING, UMBRA_INTENT_SET_SPEED, UMBRA_INTENT_SPAWN,
    UMBRA_STATUS_FAILURE, UMBRA_STATUS_RUNNING, UMBRA_STATUS_SUCCESS, UMBRA_VALUE_BOOL,
    UMBRA_VALUE_ENTITY, UMBRA_VALUE_INTEGER, UMBRA_VALUE_NUMBER, UMBRA_VALUE_POSITION,
    UMBRA_VALUE_STRING, UmbraBehavior, UmbraBehaviorItem, UmbraContact, UmbraEntityView,
    UmbraInstance, UmbraIntent, UmbraQuat, UmbraStatus, UmbraStr, UmbraValue, UmbraVec3,
};

use crate::descriptor::{Headed, read_header, read_prefix};
use crate::host_api::{abi_size, bytes};
use crate::tokens::BatchCall;

/// Number of minors this host knows.
const HOST_MINORS: usize = UMBRA_ABI_VERSION_MINOR as usize + 1;
/// Sizes of the plugin-written structs per minor (append-only, R-B3).
const INTENT_SIZES: [usize; HOST_MINORS] = [size_of::<UmbraIntent>()];
const VALUE_SIZES: [usize; HOST_MINORS] = [size_of::<UmbraValue>()];

/// `struct_size` of a host-written struct `T` for a plugin of minor `minor`: ABI v0 has one minor,
/// so it is the host's own size (R-B3; a later minor adds a size table here).
pub(crate) const fn written_size<T>(_minor: u32) -> u32 {
    abi_size::<T>()
}

/// Where one member's contacts and route start in the scratch arrays.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Span {
    pub(crate) contacts: usize,
    pub(crate) contact_count: usize,
    pub(crate) route: usize,
    pub(crate) route_len: usize,
}

/// Reusable buffers of the behavior bridge, one set per image (calls into one image are
/// serialised).
#[derive(Default)]
pub(crate) struct Scratch {
    /// Slice indices of the items this call ticks.
    pub(crate) members: Vec<usize>,
    /// Their (behavior, instance) handles.
    pub(crate) handles: Vec<(*mut UmbraBehavior, *mut UmbraInstance)>,
    pub(crate) spans: Vec<Span>,
    pub(crate) contacts: Vec<UmbraContact>,
    pub(crate) routes: Vec<UmbraVec3>,
    pub(crate) views: Vec<UmbraEntityView>,
    pub(crate) items: Vec<UmbraBehaviorItem>,
    pub(crate) statuses: Vec<UmbraStatus>,
    /// RNG pointers, pushed intents and per-item intent counts; moved into the call record during
    /// the call and back afterwards.
    pub(crate) batch: BatchCall,
}

// SAFETY: the raw pointers in the scratch (handles, views, contacts, items, RNG streams) are
// written, used and cleared within one plugin call made under the image's call lock, on the thread
// making the call; `LoadedImage::call` clears the scratch before and after every call body, on
// every return path, so between calls every pointer-holding buffer is empty (only capacity remains)
// and moving the scratch to another thread (with the lock) moves no live pointer. The pointers are
// never dereferenced by the host in any case.
unsafe impl Send for Scratch {}

impl Scratch {
    /// Empties every buffer, keeping capacity (no pointer survives a call).
    pub(crate) fn clear(&mut self) {
        self.members.clear();
        self.handles.clear();
        self.spans.clear();
        self.contacts.clear();
        self.routes.clear();
        self.views.clear();
        self.items.clear();
        self.statuses.clear();
        self.batch.rngs.clear();
        self.batch.intents.clear();
        self.batch.counts.clear();
    }

    /// Sum of the capacities (the warm-up growth probe of the tests).
    pub(crate) fn capacity(&self) -> usize {
        self.members.capacity()
            + self.handles.capacity()
            + self.spans.capacity()
            + self.contacts.capacity()
            + self.routes.capacity()
            + self.views.capacity()
            + self.items.capacity()
            + self.statuses.capacity()
            + self.batch.rngs.capacity()
            + self.batch.intents.capacity()
            + self.batch.counts.capacity()
    }

    /// Copies the contacts and route of `entity` into the scratch arrays; returns their span.
    pub(crate) fn push_entity_data(&mut self, entity: &EntityView<'_>, minor: u32) -> Span {
        let span = Span {
            contacts: self.contacts.len(),
            contact_count: entity.contacts.len(),
            route: self.routes.len(),
            route_len: entity.route.map_or(0, |r| r.waypoints.len()),
        };
        for c in entity.contacts {
            self.contacts.push(UmbraContact {
                abi_version: UMBRA_ABI_VERSION,
                struct_size: written_size::<UmbraContact>(minor),
                entity: c.entity.get(),
                position: ecef(c.position),
                velocity: vec3(c.velocity),
                force: text(c.force.as_ref().map_or("", |f| f.as_str())),
                range_m: c.range.0,
            });
        }
        if let Some(route) = entity.route {
            self.routes.extend(route.waypoints.iter().map(|w| ecef(*w)));
        }
        span
    }

    /// The view of `entity`, pointing into this scratch's contact and route arrays (filled first,
    /// so they no longer move) and into kernel data (force strings) for the duration of the call.
    pub(crate) fn view(&self, entity: &EntityView<'_>, span: Span, minor: u32) -> UmbraEntityView {
        let s = entity.state;
        UmbraEntityView {
            abi_version: UMBRA_ABI_VERSION,
            struct_size: written_size::<UmbraEntityView>(minor),
            id: entity.id.get(),
            position: ecef(s.position),
            velocity: vec3(s.velocity),
            orientation: UmbraQuat {
                w: s.orientation.w,
                x: s.orientation.x,
                y: s.orientation.y,
                z: s.orientation.z,
            },
            force: text(entity.force.map_or("", |f| f.as_str())),
            has_route: u8::from(entity.route.is_some()),
            route_complete: u8::from(entity.route.is_some_and(|r| r.complete)),
            reserved_0: [0; 6],
            route_next: entity.route.map_or(0, |r| r.next),
            route: if span.route_len == 0 {
                std::ptr::null()
            } else {
                self.routes[span.route..].as_ptr()
            },
            route_len: span.route_len,
            contacts: if span.contact_count == 0 {
                std::ptr::null()
            } else {
                self.contacts[span.contacts..].as_ptr()
            },
            contact_count: span.contact_count,
        }
    }
}

/// A borrowed `UmbraStr` of `s` (never a null pointer).
pub(crate) fn text(s: &str) -> UmbraStr {
    UmbraStr {
        ptr: s.as_ptr(),
        len: s.len(),
    }
}

fn vec3(v: Vec3) -> UmbraVec3 {
    UmbraVec3 {
        x: v.x,
        y: v.y,
        z: v.z,
    }
}

fn ecef(p: Ecef) -> UmbraVec3 {
    vec3(p.0)
}

fn finite_vec3(v: UmbraVec3, what: &str) -> Result<Ecef, String> {
    if v.x.is_finite() && v.y.is_finite() && v.z.is_finite() {
        Ok(Ecef(Vec3 {
            x: v.x,
            y: v.y,
            z: v.z,
        }))
    } else {
        Err(format!("{what} is not finite"))
    }
}

/// The core status of a plugin-written status; `Err` names the problem (unset vs out of range,
/// R-B2).
pub(crate) fn status(value: UmbraStatus) -> Result<Status, String> {
    match value {
        UMBRA_STATUS_SUCCESS => Ok(Status::Success),
        UMBRA_STATUS_FAILURE => Ok(Status::Failure),
        UMBRA_STATUS_RUNNING => Ok(Status::Running),
        0 => Err("status left unset (UMBRA_STATUS_UNSET)".to_owned()),
        other => Err(format!(
            "status {other} is out of range (expected 1, 2 or 3)"
        )),
    }
}

/// Checks the header of a plugin-written struct; returns the number of bytes to read.
fn header(
    what: &str,
    (abi_version, struct_size): (u32, u32),
    sizes: [usize; HOST_MINORS],
) -> Result<usize, String> {
    let (major, minor) = (abi_version >> 16, abi_version & 0xFFFF);
    if major != UMBRA_ABI_VERSION_MAJOR || minor > UMBRA_ABI_VERSION_MINOR {
        return Err(format!(
            "{what}.abi_version {major}.{minor} is not loadable by host ABI \
             {UMBRA_ABI_VERSION_MAJOR}.{UMBRA_ABI_VERSION_MINOR}"
        ));
    }
    let size = struct_size as usize;
    let (min, max) = (sizes[0], sizes[minor as usize]);
    if size < min || size > max || !size.is_multiple_of(8) {
        return Err(format!(
            "{what}.struct_size is {size}; expected a multiple of 8 between {min} and {max}"
        ));
    }
    Ok(size)
}

/// Reads a plugin-written headed struct.
///
/// # Safety
/// A non-null, aligned `ptr` points to at least its own `struct_size` readable bytes for the
/// duration of the call (the plugin's promise).
unsafe fn read<T: Headed>(
    ptr: *const T,
    what: &str,
    sizes: &[usize; HOST_MINORS],
) -> Result<T, String> {
    if ptr.is_null() {
        return Err(format!("{what} pointer is null"));
    }
    if !(ptr as usize).is_multiple_of(8) {
        return Err(format!("{what} pointer is not 8-byte aligned"));
    }
    // SAFETY: non-null and aligned (checked); a headed struct starts with its header (caller).
    let size = header(what, unsafe { read_header(ptr) }, *sizes)?;
    // SAFETY: `size` bytes are readable (caller) and `size <= size_of::<T>()` (checked).
    Ok(unsafe { read_prefix(ptr, size) })
}

/// Copies a plugin string.
///
/// # Safety
/// As [`bytes`]: valid for the duration of the call.
unsafe fn string(s: UmbraStr, what: &str) -> Result<String, String> {
    // SAFETY: forwarded caller contract.
    let b = unsafe { bytes(s) }.ok_or_else(|| format!("{what} is a null pointer with a length"))?;
    std::str::from_utf8(b)
        .map(str::to_owned)
        .map_err(|e| format!("{what} is not valid UTF-8 ({e})"))
}

/// Copies an ID string, refusing more than `MAX_ID_LEN` bytes before reading.
///
/// # Safety
/// As [`string`].
unsafe fn id_text(s: UmbraStr, what: &str) -> Result<String, String> {
    if s.len > MAX_ID_LEN {
        return Err(format!("{what} is {} bytes; max is {MAX_ID_LEN}", s.len));
    }
    // SAFETY: forwarded caller contract.
    unsafe { string(s, what) }
}

/// The core intent of a plugin-written `UmbraIntent` (ADR 0016 set v1); `Err` names the problem.
///
/// # Safety
/// `ptr` is what the plugin passed to the intent sink: null, or valid for the call with its
/// strings.
pub(crate) unsafe fn intent(ptr: *const UmbraIntent) -> Result<Intent, String> {
    // SAFETY: forwarded caller contract.
    let i = unsafe { read(ptr, "intent", &INTENT_SIZES) }?;
    let finite = |v: f64, what: &str| {
        if v.is_finite() {
            Ok(v)
        } else {
            Err(format!("intent {what} is not finite"))
        }
    };
    Ok(match i.kind {
        UMBRA_INTENT_MOVE_TO => Intent::MoveTo {
            destination: finite_vec3(i.position, "MoveTo position")?,
        },
        UMBRA_INTENT_FOLLOW_ROUTE => Intent::FollowRoute,
        UMBRA_INTENT_SET_SPEED => Intent::SetSpeed(MetersPerSecond(finite(i.value, "SetSpeed")?)),
        UMBRA_INTENT_SET_HEADING => Intent::SetHeading(Radians(finite(i.value, "SetHeading")?)),
        UMBRA_INTENT_HOLD => Intent::Hold,
        UMBRA_INTENT_FIRE => {
            let target = EntityId::new(i.entity)
                .ok_or_else(|| "intent Fire target is entity 0".to_owned())?;
            // SAFETY: the intent's strings are valid for the call (caller).
            let weapon = unsafe { id_text(i.id, "intent Fire weapon id") }?;
            let weapon = WeaponId::new(&weapon)
                .map_err(|e| format!("intent Fire weapon id {weapon:?}: {e}"))?;
            Intent::Fire { target, weapon }
        }
        UMBRA_INTENT_SPAWN => {
            // SAFETY: as above.
            let template = unsafe { id_text(i.id, "intent Spawn template id") }?;
            let template = TemplateId::new(&template)
                .map_err(|e| format!("intent Spawn template id {template:?}: {e}"))?;
            Intent::Spawn {
                template,
                position: finite_vec3(i.position, "Spawn position")?,
                heading: Radians(finite(i.value, "Spawn heading")?),
            }
        }
        UMBRA_INTENT_DESPAWN => Intent::Despawn,
        UMBRA_INTENT_CUSTOM => {
            // SAFETY: as above.
            let kind = unsafe { id_text(i.id, "intent Custom component id") }?;
            let kind = ComponentId::new(&kind)
                .map_err(|e| format!("intent Custom component id {kind:?}: {e}"))?;
            // SAFETY: as above.
            let payload = unsafe { string(i.payload, "intent Custom payload") }?;
            let payload: ParamValue = serde_json::from_str(&payload)
                .map_err(|e| format!("intent Custom payload is not JSON: {e}"))?;
            Intent::Custom { kind, payload }
        }
        other => return Err(format!("unknown intent kind {other}")),
    })
}

/// The core value of a plugin-written `UmbraValue` (`None` for null = unset); checks the `BbValue`
/// invariants (ADR 0019).
///
/// # Safety
/// `ptr` is null or valid for the call with its string.
pub(crate) unsafe fn value_in(ptr: *const UmbraValue) -> Result<Option<BbValue>, String> {
    if ptr.is_null() {
        return Ok(None);
    }
    // SAFETY: forwarded caller contract.
    let v = unsafe { read(ptr, "value", &VALUE_SIZES) }?;
    Ok(Some(match v.ty {
        UMBRA_VALUE_BOOL => match v.boolean {
            0 => BbValue::Bool(false),
            1 => BbValue::Bool(true),
            other => return Err(format!("value boolean is {other}, not 0 or 1")),
        },
        UMBRA_VALUE_NUMBER if v.number.is_finite() => BbValue::Number(v.number),
        UMBRA_VALUE_NUMBER => return Err("value number is not finite".to_owned()),
        UMBRA_VALUE_INTEGER => BbValue::Integer(v.integer),
        UMBRA_VALUE_STRING => {
            // SAFETY: the value's string is valid for the call (caller).
            BbValue::String(Arc::from(unsafe { string(v.string, "value string") }?))
        }
        UMBRA_VALUE_ENTITY => {
            BbValue::Entity(EntityId::new(v.entity).ok_or_else(|| "value entity is 0".to_owned())?)
        }
        UMBRA_VALUE_POSITION => BbValue::Position(finite_vec3(v.position, "value position")?),
        other => return Err(format!("value has unknown type {other}")),
    }))
}

/// A host-written value for `set_input`, its string borrowed from `value` (valid while `value`
/// lives).
pub(crate) fn value_out(value: &BbValue, minor: u32) -> UmbraValue {
    let mut out = UmbraValue {
        abi_version: UMBRA_ABI_VERSION,
        struct_size: written_size::<UmbraValue>(minor),
        ty: 0,
        boolean: 0,
        reserved_0: [0; 3],
        number: 0.0,
        integer: 0,
        string: text(""),
        entity: 0,
        position: UmbraVec3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
    };
    match value {
        BbValue::Bool(b) => {
            out.ty = UMBRA_VALUE_BOOL;
            out.boolean = u8::from(*b);
        }
        BbValue::Number(n) => {
            out.ty = UMBRA_VALUE_NUMBER;
            out.number = *n;
        }
        BbValue::Integer(i) => {
            out.ty = UMBRA_VALUE_INTEGER;
            out.integer = *i;
        }
        BbValue::String(s) => {
            out.ty = UMBRA_VALUE_STRING;
            out.string = text(s);
        }
        BbValue::Entity(e) => {
            out.ty = UMBRA_VALUE_ENTITY;
            out.entity = e.get();
        }
        BbValue::Position(p) => {
            out.ty = UMBRA_VALUE_POSITION;
            out.position = ecef(*p);
        }
        // `BbValue` is non-exhaustive; a future variant has no ABI form yet and is refused by
        // `set_input` before it gets here (its type is not one of the six).
        _ => {}
    }
    out
}

/// Whether `value` is finite where it matters (`BbValue` invariant).
pub(crate) fn finite(value: &BbValue) -> bool {
    match value {
        BbValue::Number(n) => n.is_finite(),
        BbValue::Position(p) => p.0.x.is_finite() && p.0.y.is_finite() && p.0.z.is_finite(),
        _ => true,
    }
}

/// Whether `ty` has an ABI form (`BbType` is non-exhaustive).
pub(crate) fn has_abi_form(ty: BbType) -> bool {
    matches!(
        ty,
        BbType::Bool
            | BbType::Number
            | BbType::Integer
            | BbType::String
            | BbType::Entity
            | BbType::Position
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw_intent(kind: u32) -> UmbraIntent {
        UmbraIntent {
            abi_version: UMBRA_ABI_VERSION,
            struct_size: abi_size::<UmbraIntent>(),
            kind,
            reserved_0: 0,
            position: UmbraVec3 {
                x: 1.0,
                y: 2.0,
                z: 3.0,
            },
            value: 0.5,
            entity: 7,
            id: text("test.thing.x"),
            payload: text("{\"a\":1}"),
        }
    }

    fn convert(i: &UmbraIntent) -> Result<Intent, String> {
        // SAFETY: a valid local intent with static strings.
        unsafe { intent(i) }
    }

    #[test]
    fn every_intent_kind_converts() {
        for kind in 1..=9 {
            assert!(convert(&raw_intent(kind)).is_ok(), "kind {kind}");
        }
        assert!(matches!(
            convert(&raw_intent(UMBRA_INTENT_SET_SPEED)),
            Ok(Intent::SetSpeed(MetersPerSecond(v))) if (v - 0.5).abs() < f64::EPSILON
        ));
    }

    #[test]
    fn bad_intents_name_the_problem() {
        let mut i = raw_intent(0);
        assert!(convert(&i).is_err_and(|e| e.contains("kind")));
        i = raw_intent(UMBRA_INTENT_SET_SPEED);
        i.value = f64::NAN;
        assert!(convert(&i).is_err_and(|e| e.contains("finite")));
        i = raw_intent(UMBRA_INTENT_FIRE);
        i.entity = 0;
        assert!(convert(&i).is_err_and(|e| e.contains("target")));
        i = raw_intent(UMBRA_INTENT_SPAWN);
        i.id = text("Bad");
        assert!(convert(&i).is_err_and(|e| e.contains("template id")));
        i = raw_intent(UMBRA_INTENT_CUSTOM);
        i.payload = text("{");
        assert!(convert(&i).is_err_and(|e| e.contains("payload")));
        i = raw_intent(UMBRA_INTENT_HOLD);
        i.struct_size = 16;
        assert!(convert(&i).is_err_and(|e| e.contains("struct_size")));
        i.struct_size = abi_size::<UmbraIntent>();
        i.abi_version = 1 << 16;
        assert!(convert(&i).is_err_and(|e| e.contains("abi_version")));
        // SAFETY: refused before any read.
        assert!(unsafe { intent(std::ptr::null()) }.is_err_and(|e| e.contains("null")));
    }

    #[test]
    fn values_round_trip_and_bad_values_are_refused() {
        let values = [
            BbValue::Bool(true),
            BbValue::Number(-1.5),
            BbValue::Integer(i64::MAX),
            BbValue::String("h\u{e9}".into()),
            BbValue::Entity(EntityId::new(4).unwrap_or_else(|| panic!("id"))),
            BbValue::Position(Ecef(Vec3 {
                x: 1.0,
                y: 2.0,
                z: 3.0,
            })),
        ];
        for v in values {
            let raw = value_out(&v, 0);
            // SAFETY: a valid local value whose string borrows `v`.
            assert_eq!(unsafe { value_in(&raw const raw) }, Ok(Some(v.clone())));
            assert!(finite(&v) && has_abi_form(v.ty()));
        }
        let mut raw = value_out(&BbValue::Bool(true), 0);
        raw.boolean = 2;
        // SAFETY: valid local values.
        unsafe {
            assert!(value_in(&raw const raw).is_err_and(|e| e.contains("boolean")));
            raw.ty = 99;
            assert!(value_in(&raw const raw).is_err_and(|e| e.contains("unknown type")));
            assert_eq!(value_in(std::ptr::null()), Ok(None));
        }
        assert!(!finite(&BbValue::Number(f64::INFINITY)));
    }

    #[test]
    fn statuses_distinguish_unset_from_out_of_range() {
        assert_eq!(status(1), Ok(Status::Success));
        assert_eq!(status(3), Ok(Status::Running));
        assert!(status(0).is_err_and(|e| e.contains("unset")));
        assert!(status(7).is_err_and(|e| e.contains('7')));
    }
}
