//! The behavior capability: opaque handles, the vtable, the batch call and the data it carries
//! (ADR 0020, REQ-B-4, REQ-B-5, REQ-P-6).
//!
//! Every struct the host passes into a call is written by the host in the plugin's declared minor and
//! is valid only during that call (REQ-P-11); every pointer inside it likewise. Out-values of the
//! plugin travel through host-provided sinks (copied by the host) and the plain `statuses` array (R-B2).

use core::marker::{PhantomData, PhantomPinned};

use crate::consts::{UmbraCode, UmbraIntentKind, UmbraStatus, UmbraToken, UmbraValueType};
use crate::types::{UmbraErrorSink, UmbraQuat, UmbraStr, UmbraVec3};

/// Opaque handle to a configured behavior (`Factory::build` result). Created by the plugin in
/// `configure`, owned by the plugin, destroyed only through `destroy_behavior` after every instance made
/// from it was destroyed. The host never dereferences it. In C an incomplete type.
#[repr(C)]
pub struct UmbraBehavior {
    _p: [u8; 0],
    _m: PhantomData<(*mut u8, PhantomPinned)>,
}

/// Opaque handle to one behavior instance (per entity, REQ-B-5). Created by the plugin in `instantiate`,
/// owned by the plugin, destroyed only through `destroy_instance`. The host never dereferences it. In C
/// an incomplete type.
#[repr(C)]
pub struct UmbraInstance {
    _p: [u8; 0],
    _m: PhantomData<(*mut u8, PhantomPinned)>,
}

/// Function table of a behavior capability (ADR 0020). Written by the plugin as a static with the
/// descriptor's lifetime; the host copies the prefix it knows (`min(struct_size, own size)`) at load.
/// Calls into one plugin are never concurrent. Every entry is a nullable pointer; the required ones are
/// checked by the host at load.
#[repr(C)]
pub struct UmbraBehaviorVTable {
    /// ABI version the plugin was compiled against.
    pub abi_version: u32,
    /// `sizeof` this struct as the plugin knows it.
    pub struct_size: u32,
    /// Builds a configured behavior from `params` (JSON text, borrowed during the call) into `*out`.
    /// Required. `UMBRA_OK` with a null `*out` is a fault.
    pub configure: Option<
        unsafe extern "C" fn(
            params: UmbraStr,
            out: *mut *mut UmbraBehavior,
            err: *const UmbraErrorSink,
        ) -> UmbraCode,
    >,
    /// Destroys a configured behavior. Required; cannot report errors.
    pub destroy_behavior: Option<unsafe extern "C" fn(behavior: *mut UmbraBehavior)>,
    /// Creates an instance of `behavior` ticked at `rate_hz` into `*out`. Required. `UMBRA_OK` with a
    /// null `*out` is a fault.
    pub instantiate: Option<
        unsafe extern "C" fn(
            behavior: *mut UmbraBehavior,
            rate_hz: u32,
            out: *mut *mut UmbraInstance,
            err: *const UmbraErrorSink,
        ) -> UmbraCode,
    >,
    /// Destroys an instance. Required; cannot report errors.
    pub destroy_instance: Option<unsafe extern "C" fn(instance: *mut UmbraInstance)>,
    /// Ticks every item of `batch` once: writes one status per item and pushes intents to the batch's
    /// sink. Required. A non-`UMBRA_OK` result faults the capability and discards the whole batch.
    pub tick_batch: Option<
        unsafe extern "C" fn(
            batch: *const UmbraBehaviorBatch,
            err: *const UmbraErrorSink,
        ) -> UmbraCode,
    >,
    /// Instance halt (full reset). Required; must be idempotent and callable when the instance is not
    /// running (ASM-096 c). A non-`UMBRA_OK` result is a fault.
    pub halt: Option<unsafe extern "C" fn(instance: *mut UmbraInstance) -> UmbraCode>,
    /// Halt requested by a parent while running (ADR 0013 amendment). Optional: null means the host calls
    /// `halt`.
    pub halt_running: Option<unsafe extern "C" fn(instance: *mut UmbraInstance) -> UmbraCode>,
    /// Sets the bound input `param` (UTF-8) to `*value`, or unsets it when `value` is null; both borrowed
    /// during the call. Optional; present if and only if the metadata declares `param_types`.
    pub set_input: Option<
        unsafe extern "C" fn(
            instance: *mut UmbraInstance,
            param: UmbraStr,
            value: *const UmbraValue,
            err: *const UmbraErrorSink,
        ) -> UmbraCode,
    >,
    /// Reports the current value of output `port` (UTF-8) through `sink`, at most once per call.
    /// Optional; present if and only if the metadata declares `outputs`.
    pub output: Option<
        unsafe extern "C" fn(
            instance: *mut UmbraInstance,
            port: UmbraStr,
            sink: *const UmbraValueSink,
        ) -> UmbraCode,
    >,
}

/// One batched tick of one capability (ADR 0020, REQ-B-4). Written by the host; valid only during the
/// `tick_batch` call, like everything it points to.
#[repr(C)]
pub struct UmbraBehaviorBatch {
    /// ABI version the host was compiled against.
    pub abi_version: u32,
    /// `sizeof` this struct in the layout of the plugin's declared minor.
    pub struct_size: u32,
    /// Current tick number (simulation time = `tick / rate_hz`, REQ-K-4).
    pub tick: u64,
    /// Tick rate in Hz.
    pub rate_hz: u32,
    /// Reserved: written as 0, ignored by readers.
    pub reserved_0: u32,
    /// Array of `count` items (stride: the plugin's own `sizeof(UmbraBehaviorItem)`), read-only.
    pub items: *const UmbraBehaviorItem,
    /// Number of items.
    pub count: usize,
    /// Host-owned array of `count` statuses, pre-filled with `UMBRA_STATUS_UNSET`; the plugin writes one
    /// status per item (the only out-values of the call, R-B2).
    pub statuses: *mut UmbraStatus,
    /// Sink for the intents of this call (item index plus intent).
    pub intents: *const UmbraIntentSink,
}

/// One entity in a batch. Written by the host, read-only for the plugin, valid only during the call.
#[repr(C)]
pub struct UmbraBehaviorItem {
    /// ABI version the host was compiled against.
    pub abi_version: u32,
    /// `sizeof` this struct in the layout of the plugin's declared minor.
    pub struct_size: u32,
    /// The configured behavior the instance was made from.
    pub behavior: *mut UmbraBehavior,
    /// The instance to tick.
    pub instance: *mut UmbraInstance,
    /// Read-only view of the entity's pre-tick state.
    pub entity: *const UmbraEntityView,
    /// Opaque RNG token of this item's entity stream, valid only during this call (REQ-K-4).
    pub rng: UmbraToken,
}

/// Read-only view of one entity's pre-tick state. Written by the host; valid only during the call, like
/// the arrays it points to.
#[repr(C)]
pub struct UmbraEntityView {
    /// ABI version the host was compiled against.
    pub abi_version: u32,
    /// `sizeof` this struct in the layout of the plugin's declared minor.
    pub struct_size: u32,
    /// Entity ID (never 0).
    pub id: u64,
    /// ECEF position (m).
    pub position: UmbraVec3,
    /// ECEF velocity (m/s).
    pub velocity: UmbraVec3,
    /// Orientation.
    pub orientation: UmbraQuat,
    /// Force ID (UTF-8); empty = none.
    pub force: UmbraStr,
    /// 1 if the entity has a route, else 0.
    pub has_route: u8,
    /// 1 if the route is complete, else 0.
    pub route_complete: u8,
    /// Reserved: written as 0, ignored by readers.
    pub reserved_0: [u8; 6],
    /// Index of the next route waypoint.
    pub route_next: usize,
    /// Array of `route_len` ECEF waypoints; null only when `route_len` is 0.
    pub route: *const UmbraVec3,
    /// Number of route waypoints.
    pub route_len: usize,
    /// Array of `contact_count` contacts sorted by entity ID; null only when the count is 0.
    pub contacts: *const UmbraContact,
    /// Number of contacts.
    pub contact_count: usize,
}

/// One perceived contact of an entity (ADR 0015). Written by the host; valid only during the call.
#[repr(C)]
pub struct UmbraContact {
    /// ABI version the host was compiled against.
    pub abi_version: u32,
    /// `sizeof` this struct in the layout of the plugin's declared minor; also the array stride.
    pub struct_size: u32,
    /// Perceived entity ID (never 0).
    pub entity: u64,
    /// Perceived ECEF position (m).
    pub position: UmbraVec3,
    /// Perceived ECEF velocity (m/s).
    pub velocity: UmbraVec3,
    /// Perceived force ID (UTF-8); empty = none.
    pub force: UmbraStr,
    /// Range from the perceiving entity (m).
    pub range_m: f64,
}

/// One intent (ADR 0016, set v1), written by the plugin and passed to `UmbraIntentSink::push`, which
/// copies it during the call. Fields unused by the kind are zero or empty and ignored.
#[repr(C)]
pub struct UmbraIntent {
    /// ABI version the plugin was compiled against.
    pub abi_version: u32,
    /// `sizeof` this struct as the plugin knows it.
    pub struct_size: u32,
    /// Intent kind (`UMBRA_INTENT_*`).
    pub kind: UmbraIntentKind,
    /// Reserved: written as 0, ignored by readers.
    pub reserved_0: u32,
    /// ECEF position (`MOVE_TO`, `SPAWN`).
    pub position: UmbraVec3,
    /// Scalar (`SET_SPEED` m/s, `SET_HEADING` rad, `SPAWN` heading rad).
    pub value: f64,
    /// Target entity ID (`FIRE`, non-zero).
    pub entity: u64,
    /// ID text (`FIRE` weapon, `SPAWN` template, `CUSTOM` component; ADR 0002).
    pub id: UmbraStr,
    /// JSON payload (`CUSTOM`).
    pub payload: UmbraStr,
}

/// Host-provided sink for the intents of one `tick_batch` call. Written by the host; valid only during
/// the call. The host copies every pushed intent.
#[repr(C)]
pub struct UmbraIntentSink {
    /// ABI version the host was compiled against.
    pub abi_version: u32,
    /// `sizeof` this struct in the layout of the plugin's declared minor.
    pub struct_size: u32,
    /// Opaque host token of this sink, passed back to `push`.
    pub ctx: UmbraToken,
    /// Appends `*intent` (borrowed during the call) to batch item `item` (`< count`). Required. A refused
    /// intent returns an error code and faults the capability.
    pub push: Option<
        unsafe extern "C" fn(ctx: UmbraToken, item: usize, intent: *const UmbraIntent) -> UmbraCode,
    >,
}

/// A typed blackboard value (`BbValue`, ADR 0014) for `set_input` and `output`. Written by its producer
/// (host for `set_input`, plugin for `output`) and borrowed by the reader during the call. Only the
/// field selected by `ty` is meaningful; the others are zero. Readers check the `BbValue` invariants:
/// known `ty`, `boolean` 0 or 1, finite `number` and `position`, non-zero `entity`, UTF-8 `string`.
#[repr(C)]
pub struct UmbraValue {
    /// ABI version the writer was compiled against.
    pub abi_version: u32,
    /// `sizeof` this struct as the writer knows it.
    pub struct_size: u32,
    /// Value type (`UMBRA_VALUE_*`).
    pub ty: UmbraValueType,
    /// `UMBRA_VALUE_BOOL`: 0 or 1.
    pub boolean: u8,
    /// Reserved: written as 0, ignored by readers.
    pub reserved_0: [u8; 3],
    /// `UMBRA_VALUE_NUMBER`: a finite number.
    pub number: f64,
    /// `UMBRA_VALUE_INTEGER`.
    pub integer: i64,
    /// `UMBRA_VALUE_STRING`: UTF-8.
    pub string: UmbraStr,
    /// `UMBRA_VALUE_ENTITY`: a non-zero entity ID.
    pub entity: u64,
    /// `UMBRA_VALUE_POSITION`: a finite ECEF position (m).
    pub position: UmbraVec3,
}

/// Host-provided sink for one `output` call. Written by the host; valid only during the call. The host
/// copies the value.
#[repr(C)]
pub struct UmbraValueSink {
    /// ABI version the host was compiled against.
    pub abi_version: u32,
    /// `sizeof` this struct in the layout of the plugin's declared minor.
    pub struct_size: u32,
    /// Opaque host token of this sink, passed back to `set`.
    pub ctx: UmbraToken,
    /// Records `*value` (borrowed during the call), or "unset" when `value` is null; at most once per
    /// `output` call. Required.
    pub set: Option<unsafe extern "C" fn(ctx: UmbraToken, value: *const UmbraValue) -> UmbraCode>,
}
