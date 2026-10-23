//! The sensor capability (ABI 0.2, ADR 0050, REQ-M-1, ADR 0015): the opaque sensor handle, the target
//! view, the sense call, the contact sink and the vtable.
//!
//! Lifecycle: `configure -> sense* -> destroy`. One `sense` call per observer and sensor and tick, in
//! the kernel's entity order (not batched, ASM-612). Every struct the host passes into a call is written
//! by the host in the plugin's declared minor and is valid only during that call (REQ-P-11), like every
//! pointer inside it. Contacts travel through the host-provided [`UmbraContactSink`], which copies
//! them; a contact must name a candidate's id (the kernel sorts and deduplicates them, as for built-in
//! sensors), otherwise the capability faults.

use core::marker::{PhantomData, PhantomPinned};

use crate::behavior::UmbraContact;
use crate::consts::{UmbraCode, UmbraToken};
use crate::services::UmbraTerrain;
use crate::types::{UmbraErrorSink, UmbraQuat, UmbraStr, UmbraVec3};

/// Opaque handle to one configured sensor. Created by the plugin in `configure`, owned by the plugin,
/// destroyed only through `destroy`. The host never dereferences it. In C an incomplete type.
#[repr(C)]
pub struct UmbraSensor {
    _p: [u8; 0],
    _m: PhantomData<(*mut u8, PhantomPinned)>,
}

/// Read-only view of one entity's pre-tick state for a sensor (`umbra_core::TargetView`). Written by
/// the host; valid only during the call, like the strings it points to.
#[repr(C)]
pub struct UmbraTarget {
    /// ABI version the host was compiled against.
    pub abi_version: u32,
    /// `sizeof` this struct in the layout of the plugin's declared minor; also the array stride.
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
    /// `UMBRA_OWNERSHIP_OWNED` or `UMBRA_OWNERSHIP_REMOTE`.
    pub ownership: u32,
    /// Reserved: written as 0, ignored by readers.
    pub reserved_0: u32,
}

/// One `sense` call (ADR 0050). Written by the host; valid only during the call, like everything it
/// points to.
#[repr(C)]
pub struct UmbraSenseCall {
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
    /// The observing entity, read-only.
    pub observer: *const UmbraTarget,
    /// Array of `candidate_count` candidates: every other entity, in ID order, pre-tick (stride: the
    /// plugin's own `sizeof(UmbraTarget)`), read-only; null only when the count is 0.
    pub candidates: *const UmbraTarget,
    /// Number of candidates.
    pub candidate_count: usize,
    /// Opaque RNG token of the observer's entity stream, valid only during this call (REQ-K-4); used
    /// with `UmbraHostApi::rng_next_u64` and `rng_next_f64`.
    pub rng: UmbraToken,
    /// Terrain queries for this call.
    pub terrain: *const UmbraTerrain,
}

/// Host-provided sink for the contacts of one `sense` call. Written by the host; valid only during the
/// call. The host copies every pushed contact.
#[repr(C)]
pub struct UmbraContactSink {
    /// ABI version the host was compiled against.
    pub abi_version: u32,
    /// `sizeof` this struct in the layout of the plugin's declared minor.
    pub struct_size: u32,
    /// Opaque host token of this sink, passed back to `push`.
    pub ctx: UmbraToken,
    /// Appends `*contact` (borrowed during the call). Required. At most
    /// `UMBRA_MAX_CONTACTS_PER_SENSE` pushes per call; a refused contact returns an error code and
    /// faults the capability.
    pub push:
        Option<unsafe extern "C" fn(ctx: UmbraToken, contact: *const UmbraContact) -> UmbraCode>,
}

/// Function table of a sensor capability (ABI 0.2, ADR 0050). Written by the plugin as a static with
/// the descriptor's lifetime; the host copies the prefix it knows (`min(struct_size, own size)`) at
/// load. Its header carries the sensor kind's minor (2), never above the descriptor's. Calls into one
/// plugin are never concurrent. Every entry is a nullable pointer; the required ones are checked by the
/// host at load.
#[repr(C)]
pub struct UmbraSensorVTable {
    /// ABI version the plugin was compiled against.
    pub abi_version: u32,
    /// `sizeof` this struct as the plugin knows it.
    pub struct_size: u32,
    /// Builds a configured sensor from `params` (JSON text, borrowed during the call) into `*out`.
    /// Required. A non-`UMBRA_OK` result (a panic included) or `UMBRA_OK` with a null `*out` is a
    /// build error naming the capability, not a fault.
    pub configure: Option<
        unsafe extern "C" fn(
            params: UmbraStr,
            out: *mut *mut UmbraSensor,
            err: *const UmbraErrorSink,
        ) -> UmbraCode,
    >,
    /// Destroys a configured sensor. Required; cannot report errors.
    pub destroy: Option<unsafe extern "C" fn(sensor: *mut UmbraSensor)>,
    /// Pushes the contacts the observer perceives to `sink`. Required. A non-`UMBRA_OK` result faults
    /// the capability and discards the call's contacts whole.
    pub sense: Option<
        unsafe extern "C" fn(
            sensor: *mut UmbraSensor,
            call: *const UmbraSenseCall,
            sink: *const UmbraContactSink,
            err: *const UmbraErrorSink,
        ) -> UmbraCode,
    >,
}
