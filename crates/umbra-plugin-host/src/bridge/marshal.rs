//! Marshalling between the core types and the C ABI (ADR 0019 "Values", "Bounds", R-B3; ADR 0020
//! "Marshalling"; ADR 0016 intent table).
//!
//! **Host-written structs** (batch, items, entity views, contacts, values, sinks) are written in
//! the layout of the plugin's declared minor (R-B3). ABI v0 has one minor, so that is the host's
//! own layout; [`written_size`] is the one place a later minor adds its size table.
//!
//! **Network messages** (ABI 0.1, ADR 0033): [`net_message_out`] writes a host message in the plugin's
//! declared minor (version and size), borrowing its strings from the core message for the call;
//! [`net_message_within`] reads a plugin message like the other plugin-written structs (the struct
//! exists since minor 1), checking only the fields its `kind` uses.
//!
//! **Plugin-written structs** (intents, output values, network messages) are read like the descriptor: pointer
//! non-null and 8-byte aligned, the major equal and the minor not newer than the host's nor than the
//! plugin's declared minor (the ceiling: a struct written during a capability's call carries at most
//! the capability's minor, ADR 0033 clarified 2026-10-21),
//! `struct_size` within `[minor-0 size, size of the declared minor]` and a multiple of 8, only
//! `struct_size` bytes read, every value checked (finite numbers, non-zero entities, UTF-8, ADR
//! 0002 id grammar, JSON payloads).
//!
//! **ABI 0.2 kinds** (ADR 0050): [`kinematic_out`], [`intent_out`] and [`target_out`] write the
//! records of a motion `step` and a sensor `sense` call in the plugin's declared minor (version and
//! size), borrowing strings from core values for the call; [`kinematic_in`] checks a motion result
//! (finite, unit quaternion within 1e-9, ASM-616) and [`contact_within`] reads a plugin-written contact
//! like the other plugin-written structs.
//!
//! **Scratch.** The buffers of a `tick_batch` call live in the image's call state (guarded by the
//! per-image call lock) and are reused: after warm-up a batch call allocates nothing. Views point
//! into kernel data (forces, contacts' forces) and into the scratch's own route and contact arrays
//! only for the duration of the call; every buffer holding pointers is cleared after the call.
//! Routes are copied (core `Ecef` is never cast). The motion and sensor bridges keep their intent,
//! payload, target and contact buffers here too.

#![expect(
    unsafe_code,
    reason = "reads plugin-written intents and values; scratch holds call-scoped raw pointers"
)]

use std::mem::size_of;
use std::sync::Arc;

use umbra_core::dead_reckoning::DeadReckoning;
use umbra_core::net::{NetEntityState, NetEntityType, NetKey, NetMessage};
use umbra_core::{
    BbType, BbValue, ComponentId, Contact, ControlAction, Ecef, EntityId, EntityView, Force,
    Intent, KinematicState, MAX_ID_LEN, Meters, MetersPerSecond, ParamValue, Quaternion, Radians,
    Status, TemplateId, Vec3, WeaponId, WorldEuler,
};
use umbra_plugin_api::{
    UMBRA_ABI_VERSION, UMBRA_ABI_VERSION_MAJOR, UMBRA_ABI_VERSION_MINOR, UMBRA_DR_ACCELERATION,
    UMBRA_DR_ACCELERATION_ROTATION, UMBRA_DR_BODY_ACCELERATION,
    UMBRA_DR_BODY_ACCELERATION_ROTATION, UMBRA_DR_BODY_VELOCITY, UMBRA_DR_BODY_VELOCITY_ROTATION,
    UMBRA_DR_STATIC, UMBRA_DR_VELOCITY, UMBRA_DR_VELOCITY_ROTATION, UMBRA_INTENT_CUSTOM,
    UMBRA_INTENT_DESPAWN, UMBRA_INTENT_FIRE, UMBRA_INTENT_FOLLOW_ROUTE, UMBRA_INTENT_HOLD,
    UMBRA_INTENT_MOVE_TO, UMBRA_INTENT_SET_HEADING, UMBRA_INTENT_SET_SPEED, UMBRA_INTENT_SPAWN,
    UMBRA_MAX_ID_LEN, UMBRA_NET_CONTROL, UMBRA_NET_CONTROL_PAUSE, UMBRA_NET_CONTROL_RESUME,
    UMBRA_NET_CONTROL_START, UMBRA_NET_CONTROL_STOP, UMBRA_NET_DETONATION,
    UMBRA_NET_ENTITY_REMOVED, UMBRA_NET_ENTITY_STATE, UMBRA_NET_FIRE, UMBRA_STATUS_FAILURE,
    UMBRA_STATUS_RUNNING, UMBRA_STATUS_SUCCESS, UMBRA_VALUE_BOOL, UMBRA_VALUE_ENTITY,
    UMBRA_VALUE_INTEGER, UMBRA_VALUE_NUMBER, UMBRA_VALUE_POSITION, UMBRA_VALUE_STRING,
    UmbraBehavior, UmbraBehaviorItem, UmbraCode, UmbraContact, UmbraEntityView, UmbraInstance,
    UmbraIntent, UmbraKinematicState, UmbraNetMessage, UmbraQuat, UmbraStatus, UmbraStr,
    UmbraTarget, UmbraToken, UmbraValue, UmbraValueSink, UmbraVec3,
};

use crate::descriptor::{Headed, NETWORK_ADAPTER_MINOR, read_header, read_prefix};
use crate::host_api::{abi_size, bytes};
use crate::tokens::{BatchCall, LeafCall, SenseCall};

/// Number of minors this host knows.
const HOST_MINORS: usize = UMBRA_ABI_VERSION_MINOR as usize + 1;
/// Sizes of the plugin-written structs per minor (append-only, R-B3).
/// Minor 1 (ADR 0033) and minor 2 (ADR 0050) changed none of them. Contacts are plugin-written since
/// minor 2 (a sensor's contact sink); the struct exists since minor 0.
const INTENT_SIZES: [usize; HOST_MINORS] = [
    size_of::<UmbraIntent>(),
    size_of::<UmbraIntent>(),
    size_of::<UmbraIntent>(),
];
const CONTACT_SIZES: [usize; HOST_MINORS] = [
    size_of::<UmbraContact>(),
    size_of::<UmbraContact>(),
    size_of::<UmbraContact>(),
];
const VALUE_SIZES: [usize; HOST_MINORS] = [
    size_of::<UmbraValue>(),
    size_of::<UmbraValue>(),
    size_of::<UmbraValue>(),
];
/// Value sinks are plugin-written since minor 2 (the sink a plugin passes to the blackboard service's
/// `get`, ADR 0050); the struct exists since minor 0.
const VALUE_SINK_SIZES: [usize; HOST_MINORS] = [
    size_of::<UmbraValueSink>(),
    size_of::<UmbraValueSink>(),
    size_of::<UmbraValueSink>(),
];
/// `UmbraNetMessage` exists since minor 1 (ADR 0033); the minor-0 entry is never read. Minor 2 did not
/// change it.
pub(crate) const NET_MESSAGE_SIZES: [usize; HOST_MINORS] = [
    0,
    size_of::<UmbraNetMessage>(),
    size_of::<UmbraNetMessage>(),
];

/// `struct_size` of a host-written struct `T` for a plugin of minor `minor`: no host-written 0.0 struct
/// changed in minor 1 (ADR 0033) or minor 2 (ADR 0050), so it is the host's own size (R-B3; a minor
/// that grows one adds a size table here).
pub(crate) const fn written_size<T>(_minor: u32) -> u32 {
    abi_size::<T>()
}

/// `abi_version` of a host-written network struct for a plugin of minor `minor` (at most the host's,
/// checked at load): the plugin's declared ABI, so a plugin built against an older minor never reads
/// a newer one in a struct written for it (ADR 0033, clarified 2026-10-21).
pub(crate) const fn written_version(minor: u32) -> u32 {
    (UMBRA_ABI_VERSION_MAJOR << 16) | minor
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
    /// The intents of a motion `step` call (ABI 0.2), borrowing strings from the core intents and
    /// from `payloads`.
    pub(crate) step_intents: Vec<UmbraIntent>,
    /// The JSON text of the `Custom` payloads of a motion `step` call.
    pub(crate) payloads: Vec<String>,
    /// The candidates of a sensor `sense` call (ABI 0.2), borrowing their force strings from the
    /// kernel.
    pub(crate) targets: Vec<UmbraTarget>,
    /// The observer stream, candidate ids and pushed contacts of a `sense` call; moved into the call
    /// record during the call and back afterwards.
    pub(crate) sense: SenseCall,
    /// The entity stream, blackboard, grants and undo log of a condition `check` or action `tick`
    /// call (ABI 0.2); moved into the call record during the call and back afterwards.
    pub(crate) leaf: LeafCall,
}

// SAFETY: the raw pointers in the scratch (handles, views, contacts, items, RNG streams, the strings
// of step intents and targets, the leaf call's blackboard and grants) are written, used and cleared within one plugin call made under the
// image's call lock, on the thread making the call; `LoadedImage::call` clears the scratch before and after every call body, on
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
        self.step_intents.clear();
        self.payloads.clear();
        self.targets.clear();
        self.sense.clear();
        self.leaf.clear();
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
            + self.step_intents.capacity()
            + self.payloads.capacity()
            + self.targets.capacity()
            + self.sense.candidates.capacity()
            + self.sense.contacts.capacity()
            + self.leaf.undo.capacity()
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

/// Checks the header of a plugin-written struct that exists since minor `first`, of a plugin
/// declaring minor `ceiling`; returns the number of bytes to read.
fn header(
    what: &str,
    (abi_version, struct_size): (u32, u32),
    sizes: [usize; HOST_MINORS],
    first: u32,
    ceiling: u32,
) -> Result<usize, String> {
    let (major, minor) = (abi_version >> 16, abi_version & 0xFFFF);
    if major != UMBRA_ABI_VERSION_MAJOR || minor > UMBRA_ABI_VERSION_MINOR {
        return Err(format!(
            "{what}.abi_version {major}.{minor} is not loadable by host ABI \
             {UMBRA_ABI_VERSION_MAJOR}.{UMBRA_ABI_VERSION_MINOR}"
        ));
    }
    if minor > ceiling {
        return Err(format!(
            "{what}.abi_version {major}.{minor} is newer than the plugin's declared ABI \
             {major}.{ceiling}"
        ));
    }
    if minor < first {
        return Err(format!(
            "{what}.abi_version {major}.{minor} is older than ABI {major}.{first}, which defines it"
        ));
    }
    let size = struct_size as usize;
    let (min, max) = (sizes[first as usize], sizes[minor as usize]);
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
    first: u32,
    ceiling: u32,
) -> Result<T, String> {
    if ptr.is_null() {
        return Err(format!("{what} pointer is null"));
    }
    if !(ptr as usize).is_multiple_of(8) {
        return Err(format!("{what} pointer is not 8-byte aligned"));
    }
    // SAFETY: non-null and aligned (checked); a headed struct starts with its header (caller).
    let size = header(what, unsafe { read_header(ptr) }, *sizes, first, ceiling)?;
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

/// [`intent_within`] with the host's own minor as the ceiling (unit tests).
///
/// # Safety
/// As [`intent_within`].
#[cfg(test)]
pub(crate) unsafe fn intent(ptr: *const UmbraIntent) -> Result<Intent, String> {
    // SAFETY: forwarded caller contract.
    unsafe { intent_within(ptr, UMBRA_ABI_VERSION_MINOR) }
}

/// The core intent of a plugin-written `UmbraIntent` (ADR 0016 set v1) of a plugin declaring minor
/// `ceiling`; `Err` names the problem.
///
/// # Safety
/// `ptr` is what the plugin passed to the intent sink: null, or valid for the call with its
/// strings.
pub(crate) unsafe fn intent_within(
    ptr: *const UmbraIntent,
    ceiling: u32,
) -> Result<Intent, String> {
    // SAFETY: forwarded caller contract.
    let i = unsafe { read(ptr, "intent", &INTENT_SIZES, 0, ceiling) }?;
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

/// [`value_within`] with the host's own minor as the ceiling (unit tests).
///
/// # Safety
/// As [`value_within`].
#[cfg(test)]
pub(crate) unsafe fn value_in(ptr: *const UmbraValue) -> Result<Option<BbValue>, String> {
    // SAFETY: forwarded caller contract.
    unsafe { value_within(ptr, UMBRA_ABI_VERSION_MINOR) }
}

/// The core value of a plugin-written `UmbraValue` (`None` for null = unset) of a plugin declaring
/// minor `ceiling`; checks the `BbValue` invariants (ADR 0019).
///
/// # Safety
/// `ptr` is null or valid for the call with its string.
pub(crate) unsafe fn value_within(
    ptr: *const UmbraValue,
    ceiling: u32,
) -> Result<Option<BbValue>, String> {
    if ptr.is_null() {
        return Ok(None);
    }
    // SAFETY: forwarded caller contract.
    let v = unsafe { read(ptr, "value", &VALUE_SIZES, 0, ceiling) }?;
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

/// The `set` entry of a value sink.
pub(crate) type ValueSetFn = unsafe extern "C" fn(UmbraToken, *const UmbraValue) -> UmbraCode;

/// The token and the `set` entry of a plugin-written value sink (the sink a plugin passes to the
/// blackboard service's `get`, ABI 0.2) of a plugin declaring minor `ceiling`, read like the other
/// plugin-written structs; `set` must be non-null.
///
/// # Safety
/// `ptr` is null or valid for the call (the plugin's promise).
pub(crate) unsafe fn value_sink_within(
    ptr: *const UmbraValueSink,
    ceiling: u32,
) -> Result<(UmbraToken, ValueSetFn), String> {
    // SAFETY: forwarded caller contract.
    let sink = unsafe { read(ptr, "value sink", &VALUE_SINK_SIZES, 0, ceiling) }?;
    let set = sink
        .set
        .ok_or_else(|| "value sink.set is null".to_owned())?;
    Ok((sink.ctx, set))
}

/// The ABI code of a dead-reckoning model (ADR 0032 order); `None` for a model this ABI has no code
/// for (`DeadReckoning` is non-exhaustive).
fn dead_reckoning_code(model: DeadReckoning) -> Option<u32> {
    Some(match model {
        DeadReckoning::Static => UMBRA_DR_STATIC,
        DeadReckoning::Velocity => UMBRA_DR_VELOCITY,
        DeadReckoning::VelocityRotation => UMBRA_DR_VELOCITY_ROTATION,
        DeadReckoning::AccelerationRotation => UMBRA_DR_ACCELERATION_ROTATION,
        DeadReckoning::Acceleration => UMBRA_DR_ACCELERATION,
        DeadReckoning::BodyVelocity => UMBRA_DR_BODY_VELOCITY,
        DeadReckoning::BodyVelocityRotation => UMBRA_DR_BODY_VELOCITY_ROTATION,
        DeadReckoning::BodyAccelerationRotation => UMBRA_DR_BODY_ACCELERATION_ROTATION,
        DeadReckoning::BodyAcceleration => UMBRA_DR_BODY_ACCELERATION,
        _ => return None,
    })
}

/// The model of an ABI dead-reckoning code.
fn dead_reckoning(code: u32) -> Result<DeadReckoning, String> {
    Ok(match code {
        UMBRA_DR_STATIC => DeadReckoning::Static,
        UMBRA_DR_VELOCITY => DeadReckoning::Velocity,
        UMBRA_DR_VELOCITY_ROTATION => DeadReckoning::VelocityRotation,
        UMBRA_DR_ACCELERATION_ROTATION => DeadReckoning::AccelerationRotation,
        UMBRA_DR_ACCELERATION => DeadReckoning::Acceleration,
        UMBRA_DR_BODY_VELOCITY => DeadReckoning::BodyVelocity,
        UMBRA_DR_BODY_VELOCITY_ROTATION => DeadReckoning::BodyVelocityRotation,
        UMBRA_DR_BODY_ACCELERATION_ROTATION => DeadReckoning::BodyAccelerationRotation,
        UMBRA_DR_BODY_ACCELERATION => DeadReckoning::BodyAcceleration,
        other => {
            return Err(format!(
                "dead_reckoning {other} is not a model (expected 1..=9)"
            ));
        }
    })
}

/// The ABI code of a run-control action; `None` for an action the ABI cannot carry (`Step`, and
/// future variants of the non-exhaustive enum).
fn control_code(action: ControlAction) -> Option<u32> {
    Some(match action {
        ControlAction::Start => UMBRA_NET_CONTROL_START,
        ControlAction::Pause => UMBRA_NET_CONTROL_PAUSE,
        ControlAction::Resume => UMBRA_NET_CONTROL_RESUME,
        ControlAction::Stop => UMBRA_NET_CONTROL_STOP,
        _ => return None,
    })
}

/// The run-control action of an ABI code.
fn control(code: u32) -> Result<ControlAction, String> {
    Ok(match code {
        UMBRA_NET_CONTROL_START => ControlAction::Start,
        UMBRA_NET_CONTROL_PAUSE => ControlAction::Pause,
        UMBRA_NET_CONTROL_RESUME => ControlAction::Resume,
        UMBRA_NET_CONTROL_STOP => ControlAction::Stop,
        other => return Err(format!("control {other} is not an action (expected 1..=4)")),
    })
}

/// An all-zero message of `kind` in the layout of minor `minor` (unused fields stay zero).
fn net_blank(kind: u32, minor: u32) -> UmbraNetMessage {
    let zero = UmbraVec3 {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
    UmbraNetMessage {
        abi_version: written_version(minor),
        struct_size: written_size::<UmbraNetMessage>(minor),
        kind,
        dead_reckoning: 0,
        key: 0,
        other_key: 0,
        position: zero,
        velocity: zero,
        acceleration: zero,
        angular_velocity: zero,
        orientation: zero,
        entity_type: text(""),
        force: text(""),
        name: text(""),
        weapon: text(""),
        control: 0,
        reserved_0: 0,
    }
}

/// The host-written form of `message` for a plugin of minor `minor`, its strings borrowed from
/// `message` (valid while `message` lives); `None` for a message the ABI cannot carry (a `Step`
/// control action, a dead-reckoning model or message variant newer than this ABI).
pub(crate) fn net_message_out(message: &NetMessage, minor: u32) -> Option<UmbraNetMessage> {
    let key = |k: Option<NetKey>| k.map_or(0, |k| k.0);
    Some(match message {
        NetMessage::EntityState(s) => {
            let mut out = net_blank(UMBRA_NET_ENTITY_STATE, minor);
            out.dead_reckoning = dead_reckoning_code(s.dead_reckoning)?;
            out.key = s.key.0;
            out.position = ecef(s.position);
            out.velocity = vec3(s.velocity);
            out.acceleration = vec3(s.acceleration);
            out.angular_velocity = vec3(s.angular_velocity);
            out.orientation = UmbraVec3 {
                x: s.orientation.yaw.0,
                y: s.orientation.pitch.0,
                z: s.orientation.roll.0,
            };
            // The ABI bounds every string by `UMBRA_MAX_ID_LEN` (ASM-409): an opaque type token is
            // not cut (a prefix would be another token), so an over-long one is sent as none; a
            // display name is cut at a char boundary. `force` and `weapon` are bounded by their
            // grammar.
            out.entity_type = text(
                s.entity_type
                    .as_ref()
                    .map(NetEntityType::as_str)
                    .filter(|t| t.len() <= UMBRA_MAX_ID_LEN)
                    .unwrap_or(""),
            );
            out.force = text(s.force.as_ref().map_or("", Force::as_str));
            out.name = text(bounded_prefix(
                s.name.as_deref().unwrap_or(""),
                UMBRA_MAX_ID_LEN,
            ));
            out
        }
        NetMessage::EntityRemoved { key } => {
            let mut out = net_blank(UMBRA_NET_ENTITY_REMOVED, minor);
            out.key = key.0;
            out
        }
        NetMessage::Fire {
            shooter,
            target,
            weapon,
            position,
        } => {
            let mut out = net_blank(UMBRA_NET_FIRE, minor);
            out.key = shooter.0;
            out.other_key = key(*target);
            out.weapon = text(weapon.as_str());
            out.position = ecef(*position);
            out
        }
        NetMessage::Detonation {
            shooter,
            target,
            position,
        } => {
            let mut out = net_blank(UMBRA_NET_DETONATION, minor);
            out.key = key(*shooter);
            out.other_key = key(*target);
            out.position = ecef(*position);
            out
        }
        NetMessage::SimulationControl(action) => {
            let mut out = net_blank(UMBRA_NET_CONTROL, minor);
            out.control = control_code(*action)?;
            out
        }
        _ => return None,
    })
}

/// The longest prefix of `s` of at most `max` bytes that ends at a char boundary.
fn bounded_prefix(s: &str, max: usize) -> &str {
    &s[..s.floor_char_boundary(max)]
}

/// An optional string of at most `MAX_ID_LEN` bytes; empty is `None`.
///
/// # Safety
/// As [`string`].
unsafe fn optional_text(s: UmbraStr, what: &str) -> Result<Option<String>, String> {
    // SAFETY: forwarded caller contract.
    let t = unsafe { id_text(s, what) }?;
    Ok(if t.is_empty() { None } else { Some(t) })
}

/// A key that must name something (`0` is never a key).
fn subject(key: u64, what: &str) -> Result<NetKey, String> {
    if key == 0 {
        Err(format!("{what} is 0"))
    } else {
        Ok(NetKey(key))
    }
}

/// A key where `0` means none.
fn optional_key(key: u64) -> Option<NetKey> {
    (key != 0).then_some(NetKey(key))
}

fn finite_plain(v: UmbraVec3, what: &str) -> Result<Vec3, String> {
    finite_vec3(v, what).map(|e| e.0)
}

/// The core message of a plugin-written `UmbraNetMessage` (ABI 0.1) of a plugin declaring minor
/// `ceiling`. Only the fields the message's `kind` uses are checked (ADR 0033): known codes, finite
/// numbers, UTF-8 strings of at most `UMBRA_MAX_ID_LEN` bytes, a non-zero subject key, a non-empty
/// weapon ID for `FIRE`; the entity type token is passed through unchanged. `Err` names the problem.
///
/// # Safety
/// `ptr` is what the plugin passed to the message sink: null, or valid for the call with its
/// strings.
pub(crate) unsafe fn net_message_within(
    ptr: *const UmbraNetMessage,
    ceiling: u32,
) -> Result<NetMessage, String> {
    // SAFETY: forwarded caller contract.
    let m = unsafe {
        read(
            ptr,
            "message",
            &NET_MESSAGE_SIZES,
            NETWORK_ADAPTER_MINOR,
            ceiling,
        )
    }?;
    Ok(match m.kind {
        UMBRA_NET_ENTITY_STATE => {
            let o = finite_plain(m.orientation, "ENTITY_STATE orientation")?;
            // SAFETY: the message's strings are valid for the call (caller).
            let entity_type = unsafe { optional_text(m.entity_type, "ENTITY_STATE entity_type") }?;
            // SAFETY: as above.
            let force = unsafe { optional_text(m.force, "ENTITY_STATE force") }?
                .map(|f| Force::new(&f).map_err(|e| format!("ENTITY_STATE force {f:?}: {e}")))
                .transpose()?;
            // SAFETY: as above.
            let name = unsafe { optional_text(m.name, "ENTITY_STATE name") }?;
            NetMessage::EntityState(NetEntityState {
                key: subject(m.key, "ENTITY_STATE key")?,
                entity_type: entity_type.map(NetEntityType::new),
                force,
                name: name.map(Arc::from),
                position: finite_vec3(m.position, "ENTITY_STATE position")?,
                velocity: finite_plain(m.velocity, "ENTITY_STATE velocity")?,
                acceleration: finite_plain(m.acceleration, "ENTITY_STATE acceleration")?,
                angular_velocity: finite_plain(
                    m.angular_velocity,
                    "ENTITY_STATE angular_velocity",
                )?,
                orientation: WorldEuler {
                    yaw: Radians(o.x),
                    pitch: Radians(o.y),
                    roll: Radians(o.z),
                },
                dead_reckoning: dead_reckoning(m.dead_reckoning)
                    .map_err(|e| format!("ENTITY_STATE {e}"))?,
            })
        }
        UMBRA_NET_ENTITY_REMOVED => NetMessage::EntityRemoved {
            key: subject(m.key, "ENTITY_REMOVED key")?,
        },
        UMBRA_NET_FIRE => {
            // SAFETY: the message's strings are valid for the call (caller).
            let weapon = unsafe { optional_text(m.weapon, "FIRE weapon") }?
                .ok_or_else(|| "FIRE weapon is empty (required for FIRE)".to_owned())?;
            NetMessage::Fire {
                shooter: subject(m.key, "FIRE key (the shooter)")?,
                target: optional_key(m.other_key),
                weapon: WeaponId::new(&weapon)
                    .map_err(|e| format!("FIRE weapon {weapon:?}: {e}"))?,
                position: finite_vec3(m.position, "FIRE position")?,
            }
        }
        UMBRA_NET_DETONATION => NetMessage::Detonation {
            shooter: optional_key(m.key),
            target: optional_key(m.other_key),
            position: finite_vec3(m.position, "DETONATION position")?,
        },
        UMBRA_NET_CONTROL => {
            NetMessage::SimulationControl(control(m.control).map_err(|e| format!("CONTROL {e}"))?)
        }
        other => return Err(format!("message kind {other} is unknown (expected 1..=5)")),
    })
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

/// Largest accepted deviation of a motion result's orientation norm from 1 (the core contract
/// tolerance, `umbra_core::motion::contract`; ASM-616).
pub(crate) const UNIT_TOLERANCE: f64 = 1e-9;

fn quat(q: Quaternion) -> UmbraQuat {
    UmbraQuat {
        w: q.w,
        x: q.x,
        y: q.y,
        z: q.z,
    }
}

/// The host-written kinematic state `state` for a plugin of minor `minor` (ABI 0.2).
pub(crate) fn kinematic_out(state: &KinematicState, minor: u32) -> UmbraKinematicState {
    UmbraKinematicState {
        abi_version: written_version(minor),
        struct_size: written_size::<UmbraKinematicState>(minor),
        position: ecef(state.position),
        velocity: vec3(state.velocity),
        orientation: quat(state.orientation),
    }
}

/// The core state of a motion result the plugin wrote into the host-owned `out` (its header is the
/// host's and is not read): every component finite and the orientation a unit quaternion within
/// [`UNIT_TOLERANCE`] (ADR 0050, ASM-616); `Err` names the problem.
pub(crate) fn kinematic_in(out: &UmbraKinematicState) -> Result<KinematicState, String> {
    let position = finite_vec3(out.position, "step output position")?;
    let velocity = finite_vec3(out.velocity, "step output velocity")?.0;
    let q = out.orientation;
    if ![q.w, q.x, q.y, q.z].iter().all(|c| c.is_finite()) {
        return Err("step output orientation is not finite".to_owned());
    }
    let norm = (q.w * q.w + q.x * q.x + q.y * q.y + q.z * q.z).sqrt();
    if (norm - 1.0).abs() > UNIT_TOLERANCE {
        return Err(format!(
            "step output orientation is not a unit quaternion (norm {norm}, tolerance \
             {UNIT_TOLERANCE})"
        ));
    }
    Ok(KinematicState {
        position,
        velocity,
        orientation: Quaternion {
            w: q.w,
            x: q.x,
            y: q.y,
            z: q.z,
        },
    })
}

/// An all-zero intent of `kind` in the layout of minor `minor` (unused fields stay zero or empty).
fn intent_blank(kind: u32, minor: u32) -> UmbraIntent {
    UmbraIntent {
        abi_version: written_version(minor),
        struct_size: written_size::<UmbraIntent>(minor),
        kind,
        reserved_0: 0,
        position: UmbraVec3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
        value: 0.0,
        entity: 0,
        id: text(""),
        payload: text(""),
    }
}

/// The host-written form of `intent` for a motion step of a plugin of minor `minor` (ADR 0016 table,
/// the reverse of [`intent_within`]), its strings borrowed from `intent` and, for `Custom`, from
/// `payload` (the payload's JSON text); `None` for a variant this ABI has no kind for (`Intent` is
/// non-exhaustive).
pub(crate) fn intent_out(intent: &Intent, payload: &str, minor: u32) -> Option<UmbraIntent> {
    Some(match intent {
        Intent::MoveTo { destination } => {
            let mut out = intent_blank(UMBRA_INTENT_MOVE_TO, minor);
            out.position = ecef(*destination);
            out
        }
        Intent::FollowRoute => intent_blank(UMBRA_INTENT_FOLLOW_ROUTE, minor),
        Intent::SetSpeed(speed) => {
            let mut out = intent_blank(UMBRA_INTENT_SET_SPEED, minor);
            out.value = speed.0;
            out
        }
        Intent::SetHeading(heading) => {
            let mut out = intent_blank(UMBRA_INTENT_SET_HEADING, minor);
            out.value = heading.0;
            out
        }
        Intent::Hold => intent_blank(UMBRA_INTENT_HOLD, minor),
        Intent::Fire { target, weapon } => {
            let mut out = intent_blank(UMBRA_INTENT_FIRE, minor);
            out.entity = target.get();
            out.id = text(weapon.as_str());
            out
        }
        Intent::Spawn {
            template,
            position,
            heading,
        } => {
            let mut out = intent_blank(UMBRA_INTENT_SPAWN, minor);
            out.id = text(template.as_str());
            out.position = ecef(*position);
            out.value = heading.0;
            out
        }
        Intent::Despawn => intent_blank(UMBRA_INTENT_DESPAWN, minor),
        Intent::Custom { kind, .. } => {
            let mut out = intent_blank(UMBRA_INTENT_CUSTOM, minor);
            out.id = text(kind.as_str());
            out.payload = text(payload);
            out
        }
        _ => return None,
    })
}

/// The host-written target view of entity `id` for a sense call of a plugin of minor `minor`, its
/// force string borrowed from `force` (ABI 0.2); `ownership` is `UMBRA_OWNERSHIP_*`.
pub(crate) fn target_out(
    id: EntityId,
    state: &KinematicState,
    force: Option<&Force>,
    ownership: u32,
    minor: u32,
) -> UmbraTarget {
    UmbraTarget {
        abi_version: written_version(minor),
        struct_size: written_size::<UmbraTarget>(minor),
        id: id.get(),
        position: ecef(state.position),
        velocity: vec3(state.velocity),
        orientation: quat(state.orientation),
        force: text(force.map_or("", Force::as_str)),
        ownership,
        reserved_0: 0,
    }
}

/// The core contact of a plugin-written `UmbraContact` (a sensor's contact sink, ABI 0.2) of a plugin
/// declaring minor `ceiling`: a non-zero entity, finite position and velocity, a finite range of at
/// least 0, and a force that is empty (none) or a valid force name (ASM-719). Whether the entity is a
/// candidate is the sink's check. `Err` names the problem.
///
/// # Safety
/// `ptr` is what the plugin passed to the contact sink: null, or valid for the call with its string.
pub(crate) unsafe fn contact_within(
    ptr: *const UmbraContact,
    ceiling: u32,
) -> Result<Contact, String> {
    // SAFETY: forwarded caller contract.
    let c = unsafe { read(ptr, "contact", &CONTACT_SIZES, 0, ceiling) }?;
    let entity = EntityId::new(c.entity).ok_or_else(|| "contact entity is 0".to_owned())?;
    let position = finite_vec3(c.position, "contact position")?;
    let velocity = finite_vec3(c.velocity, "contact velocity")?.0;
    if !(c.range_m.is_finite() && c.range_m >= 0.0) {
        return Err(format!(
            "contact range_m {} is not a finite distance of at least 0",
            c.range_m
        ));
    }
    // SAFETY: the contact's string is valid for the call (caller).
    let force = match unsafe { id_text(c.force, "contact force") }?.as_str() {
        "" => None,
        name => Some(Force::new(name).map_err(|e| format!("contact force {name:?}: {e}"))?),
    };
    Ok(Contact::new(
        entity,
        position,
        velocity,
        force,
        Meters(c.range_m),
    ))
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

    // ------------------------------------------------------------ ABI 0.2 kinds (M7 WI-8)

    fn p3(x: f64, y: f64, z: f64) -> Ecef {
        Ecef(Vec3 { x, y, z })
    }

    /// ADR 0016 table: every intent a step carries reads back as itself through the plugin-side
    /// reader (the host's own), in the plugin's minor.
    #[test]
    fn step_intents_round_trip_in_the_plugins_minor() {
        let intents = [
            Intent::MoveTo {
                destination: p3(1.0, 2.0, 3.0),
            },
            Intent::SetSpeed(MetersPerSecond(4.5)),
            Intent::Hold,
            Intent::FollowRoute,
            Intent::SetHeading(Radians(-0.25)),
            Intent::Fire {
                target: EntityId::new(9).unwrap_or_else(|| panic!("id")),
                weapon: WeaponId::new("test.gun").unwrap_or_else(|e| panic!("{e}")),
            },
            Intent::Spawn {
                template: TemplateId::new("test.tpl").unwrap_or_else(|e| panic!("{e}")),
                position: p3(4.0, 5.0, 6.0),
                heading: Radians(1.5),
            },
            Intent::Despawn,
            Intent::Custom {
                kind: ComponentId::new("test.event.ping").unwrap_or_else(|e| panic!("{e}")),
                payload: serde_json::json!({"a": 1}),
            },
        ];
        for intent in &intents {
            let payload = match intent {
                Intent::Custom { payload, .. } => payload.to_string(),
                _ => String::new(),
            };
            let raw = intent_out(intent, &payload, 2).unwrap_or_else(|| panic!("{intent:?}"));
            assert_eq!(raw.abi_version, written_version(2));
            assert_eq!(raw.struct_size, abi_size::<UmbraIntent>());
            // SAFETY: a valid local intent whose strings borrow `intent` and `payload`.
            let back = unsafe { intent_within(&raw const raw, 2) };
            assert_eq!(back.as_ref(), Ok(intent));
        }
    }

    #[test]
    fn a_motion_result_must_be_finite_with_a_unit_orientation() {
        let state = KinematicState {
            position: p3(1.0, 2.0, 3.0),
            velocity: Vec3 {
                x: 0.5,
                y: 0.0,
                z: -0.5,
            },
            orientation: Quaternion {
                w: 0.6,
                x: 0.0,
                y: 0.8,
                z: 0.0,
            },
        };
        let out = kinematic_out(&state, 2);
        assert_eq!(out.abi_version, written_version(2));
        assert_eq!(kinematic_in(&out), Ok(state));
        let mut near = kinematic_out(&state, 2);
        near.orientation.w = 1.0 + 0.9e-9;
        near.orientation.y = 0.0;
        assert!(kinematic_in(&near).is_ok(), "within the tolerance");
        near.orientation.w = 1.0 + 1.1e-9;
        assert!(kinematic_in(&near).is_err_and(|e| e.contains("unit quaternion")));
        let mut bad = kinematic_out(&state, 2);
        bad.velocity.y = f64::INFINITY;
        assert!(kinematic_in(&bad).is_err_and(|e| e.contains("velocity is not finite")));
        bad = kinematic_out(&state, 2);
        bad.position.z = f64::NAN;
        assert!(kinematic_in(&bad).is_err_and(|e| e.contains("position is not finite")));
        bad = kinematic_out(&state, 2);
        bad.orientation.x = f64::NAN;
        assert!(kinematic_in(&bad).is_err_and(|e| e.contains("orientation is not finite")));
    }

    fn raw_contact() -> UmbraContact {
        UmbraContact {
            abi_version: written_version(2),
            struct_size: abi_size::<UmbraContact>(),
            entity: 3,
            position: UmbraVec3 {
                x: 1.0,
                y: 2.0,
                z: 3.0,
            },
            velocity: UmbraVec3 {
                x: 0.0,
                y: 1.0,
                z: 0.0,
            },
            force: text("blue"),
            range_m: 10.0,
        }
    }

    /// An edit that breaks a contact.
    type ContactEdit = fn(&mut UmbraContact);

    fn contact_of(c: &UmbraContact) -> Result<Contact, String> {
        // SAFETY: a valid local contact with a static string.
        unsafe { contact_within(c, 2) }
    }

    /// ASM-719: what a plugin contact must be.
    #[test]
    fn plugin_contacts_are_checked_and_copied() {
        let c = contact_of(&raw_contact()).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(c.entity.get(), 3);
        assert_eq!(c.position, p3(1.0, 2.0, 3.0));
        assert_eq!(c.velocity.y, 1.0);
        assert_eq!(c.force.as_ref().map(Force::as_str), Some("blue"));
        assert_eq!(c.range, Meters(10.0));
        let mut none = raw_contact();
        none.force = text("");
        assert_eq!(contact_of(&none).map(|c| c.force), Ok(None));
        let mut zero = raw_contact();
        zero.range_m = 0.0;
        assert!(contact_of(&zero).is_ok(), "range 0 is a distance");
        let cases: [(ContactEdit, &str); 6] = [
            (|c| c.entity = 0, "entity is 0"),
            (|c| c.position.x = f64::NAN, "position is not finite"),
            (|c| c.velocity.z = f64::INFINITY, "velocity is not finite"),
            (|c| c.range_m = -1.0, "range_m"),
            (|c| c.range_m = f64::NAN, "range_m"),
            (|c| c.force = text("Blue"), "force"),
        ];
        for (edit, needle) in cases {
            let mut c = raw_contact();
            edit(&mut c);
            assert!(
                contact_of(&c).is_err_and(|e| e.contains(needle)),
                "{needle}"
            );
        }
        let mut newer = raw_contact();
        newer.abi_version = written_version(3);
        assert!(contact_of(&newer).is_err_and(|e| e.contains("abi_version")));
    }
}
