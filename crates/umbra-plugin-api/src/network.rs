//! The network adapter capability (ABI 0.1, ADR 0033, REQ-N-1, REQ-P-4): the opaque adapter handle,
//! the vtable, the message, the publish batch and the poll sink.
//!
//! Lifecycle (REQ-P-8): `configure -> key_space -> start -> (publish | poll | counters)* -> stop ->
//! destroy`. `start` and `stop` are optional steps of a lifecycle: `configure -> destroy` (a dry-run
//! build) and `configure -> key_space -> destroy` are complete lifecycles too. `destroy` is valid after
//! `configure` in every state (started or not, stopped or not, faulted or not) and joins every thread
//! the adapter started. The adapter's I/O threads belong to the plugin; they never call the host API,
//! and `stop` and `destroy` join every one of them before returning.
//!
//! `publish` and `poll` never block on I/O (ASM-288): they move data to or from bounded queues.
//! Transient I/O problems (a full queue, a dropped or undecodable datagram, a send error) are counted
//! (see `counters`) and the call still returns `UMBRA_OK`. A non-`UMBRA_OK` result of any call faults
//! the capability; after a fault the host calls only `stop` and `destroy`.
//!
//! Every struct the host passes into a call is written by the host in the plugin's declared minor and
//! is valid only during that call (REQ-P-11), like every pointer inside it. Messages from the plugin
//! travel through the host-provided [`UmbraNetSink`], which copies them; a message the plugin writes
//! carries at most the plugin's declared minor (ADR 0033, clarified 2026-10-21).

use core::marker::{PhantomData, PhantomPinned};

use crate::consts::{UmbraCode, UmbraDeadReckoning, UmbraNetControl, UmbraNetKind, UmbraToken};
use crate::types::{UmbraErrorSink, UmbraStr, UmbraVec3};

/// Opaque handle to one configured network adapter. Created by the plugin in `configure`, owned by
/// the plugin, destroyed only through `destroy`. The host never dereferences it. In C an incomplete
/// type.
#[repr(C)]
pub struct UmbraNetAdapter {
    _p: [u8; 0],
    _m: PhantomData<(*mut u8, PhantomPinned)>,
}

/// One network message (the canonical `umbra_core::net::NetMessage`, ADR 0030). Written by the host
/// (in a `publish` batch, in the plugin's declared minor) or by the plugin (passed to
/// [`UmbraNetSink::push`], which copies it); borrowed by the reader during the call. Writers zero the
/// fields unused by `kind`; readers ignore them, even when non-zero, and check only the fields the kind
/// uses: known `kind`, `dead_reckoning` and `control` where used, finite numbers, UTF-8 strings of at
/// most `UMBRA_MAX_ID_LEN` bytes.
#[repr(C)]
pub struct UmbraNetMessage {
    /// ABI version the writer was compiled against.
    pub abi_version: u32,
    /// `sizeof` this struct as the writer knows it (host-written: the plugin's declared minor).
    pub struct_size: u32,
    /// Message kind (`UMBRA_NET_*`).
    pub kind: UmbraNetKind,
    /// `ENTITY_STATE`: the dead-reckoning model (`UMBRA_DR_*`).
    pub dead_reckoning: UmbraDeadReckoning,
    /// The subject's network key: the entity (`ENTITY_STATE`, `ENTITY_REMOVED`), the shooter (`FIRE`;
    /// `DETONATION`, 0 = none). Keys are never 0 otherwise.
    pub key: u64,
    /// The target's network key (`FIRE`, `DETONATION`); 0 = none.
    pub other_key: u64,
    /// ECEF position (m): entity (`ENTITY_STATE`), launch point (`FIRE`), burst point (`DETONATION`).
    pub position: UmbraVec3,
    /// `ENTITY_STATE`: ECEF velocity (m/s).
    pub velocity: UmbraVec3,
    /// `ENTITY_STATE`: ECEF acceleration (m/s^2).
    pub acceleration: UmbraVec3,
    /// `ENTITY_STATE`: body-axis angular velocity (rad/s).
    pub angular_velocity: UmbraVec3,
    /// `ENTITY_STATE`: world-frame orientation as `x` = yaw, `y` = pitch, `z` = roll (rad, ADR 0032).
    pub orientation: UmbraVec3,
    /// `ENTITY_STATE`: the adapter-normalised entity type token (UTF-8); empty = none. Opaque: the
    /// host passes it through unchanged and never parses it.
    pub entity_type: UmbraStr,
    /// `ENTITY_STATE`: force ID (UTF-8); empty = none.
    pub force: UmbraStr,
    /// `ENTITY_STATE`: display name (UTF-8); empty = none.
    pub name: UmbraStr,
    /// `FIRE`: weapon ID (UTF-8, ADR 0002); required (non-empty) for `FIRE`.
    pub weapon: UmbraStr,
    /// `CONTROL`: the action (`UMBRA_NET_CONTROL_*`).
    pub control: UmbraNetControl,
    /// Reserved: written as 0, ignored by readers.
    pub reserved_0: u32,
}

/// One `publish` call's outbound messages (ADR 0033). Written by the host in the plugin's declared
/// minor; valid only during the call, like the array it points to.
#[repr(C)]
pub struct UmbraNetBatch {
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
    /// Array of `count` messages (stride: the plugin's own `sizeof(UmbraNetMessage)`), read-only; null
    /// only when `count` is 0.
    pub messages: *const UmbraNetMessage,
    /// Number of messages.
    pub count: usize,
}

/// Host-provided sink for the inbound messages of one `poll` call. Written by the host in the plugin's
/// declared minor; valid only during the call. The host copies every pushed message.
#[repr(C)]
pub struct UmbraNetSink {
    /// ABI version the host was compiled against.
    pub abi_version: u32,
    /// `sizeof` this struct in the layout of the plugin's declared minor.
    pub struct_size: u32,
    /// Opaque host token of this sink, passed back to `push`.
    pub ctx: UmbraToken,
    /// Appends `*message` (borrowed during the call). Required. At most `max` (and
    /// `UMBRA_MAX_NET_POLL`) pushes per call; a refused message returns an error code and faults the
    /// capability.
    pub push:
        Option<unsafe extern "C" fn(ctx: UmbraToken, message: *const UmbraNetMessage) -> UmbraCode>,
}

/// Function table of a network adapter capability (ABI 0.1, ADR 0033). Written by the plugin as a
/// static with the descriptor's lifetime; the host copies the prefix it knows (`min(struct_size, own
/// size)`) at load. Its header carries the network kind's minor (1), never above the descriptor's.
/// Calls into one
/// plugin are never concurrent. Every entry is a nullable pointer; the required ones are checked by the
/// host at load.
#[repr(C)]
pub struct UmbraNetworkVTable {
    /// ABI version the plugin was compiled against.
    pub abi_version: u32,
    /// `sizeof` this struct as the plugin knows it.
    pub struct_size: u32,
    /// Builds a configured adapter into `*out` from `params`: JSON text, borrowed during the call, the
    /// wrapper `{"name", "params", "templates"}` (ADR 0030; the capability's param schema applies to
    /// `params`, errors point at `/params/<key>`). No I/O. Required. `UMBRA_OK` with a null `*out` is a
    /// fault.
    pub configure: Option<
        unsafe extern "C" fn(
            params: UmbraStr,
            out: *mut *mut UmbraNetAdapter,
            err: *const UmbraErrorSink,
        ) -> UmbraCode,
    >,
    /// Destroys an adapter, after joining every thread it started. Required; cannot report errors.
    /// Valid after `configure` in every state (also without `start` or `stop`, and after a fault).
    pub destroy: Option<unsafe extern "C" fn(adapter: *mut UmbraNetAdapter)>,
    /// Writes the adapter's own key space (`base < key <= base + count`) into `*base` and `*count`.
    /// Required. `count >= 1` and `base + count` must not overflow `u64` (the host checks both).
    pub key_space: Option<
        unsafe extern "C" fn(
            adapter: *mut UmbraNetAdapter,
            base: *mut u64,
            count: *mut u64,
        ) -> UmbraCode,
    >,
    /// Starts the adapter's I/O at tick `tick` of a run at `rate_hz`; at most once. Required entry; an
    /// optional lifecycle step (a dry-run build never starts).
    pub start: Option<
        unsafe extern "C" fn(
            adapter: *mut UmbraNetAdapter,
            tick: u64,
            rate_hz: u32,
            err: *const UmbraErrorSink,
        ) -> UmbraCode,
    >,
    /// Hands `batch`'s outbound messages to the adapter without blocking. Required. Transient I/O
    /// problems are counted and return `UMBRA_OK`; a non-`UMBRA_OK` result faults the capability.
    pub publish: Option<
        unsafe extern "C" fn(
            adapter: *mut UmbraNetAdapter,
            batch: *const UmbraNetBatch,
            err: *const UmbraErrorSink,
        ) -> UmbraCode,
    >,
    /// Pushes at most `max` received messages to `sink` without blocking. Required. Transient I/O
    /// problems (an undecodable datagram, a full queue) are counted and return `UMBRA_OK`; a
    /// non-`UMBRA_OK` result faults the capability and discards the call's messages whole.
    pub poll: Option<
        unsafe extern "C" fn(
            adapter: *mut UmbraNetAdapter,
            tick: u64,
            max: usize,
            sink: *const UmbraNetSink,
            err: *const UmbraErrorSink,
        ) -> UmbraCode,
    >,
    /// Stops the adapter's I/O and joins its threads; idempotent, also valid when never started.
    /// Required entry; an optional lifecycle step (`destroy` also joins).
    pub stop: Option<unsafe extern "C" fn(adapter: *mut UmbraNetAdapter) -> UmbraCode>,
    /// Writes counters into `out[0..n]` (`n` at most `UMBRA_MAX_NET_COUNTERS`), in the order tx
    /// dropped, rx dropped, decode errors, I/O errors. The host zero-fills `out[0..n]` before the call;
    /// the plugin writes at most `min(n, the counters it knows)` entries. Optional: null means all
    /// counters are 0. A non-`UMBRA_OK` result faults the capability.
    pub counters: Option<
        unsafe extern "C" fn(adapter: *mut UmbraNetAdapter, out: *mut u64, n: usize) -> UmbraCode,
    >,
}
