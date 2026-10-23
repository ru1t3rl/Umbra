//! The motion model capability (ABI 0.2, ADR 0050, REQ-M-1, REQ-M-4): the opaque model handle, the
//! kinematic state, the step call and the vtable.
//!
//! Lifecycle: `configure -> step* -> destroy`. One `step` call per entity and tick, in the kernel's
//! entity order (not batched, ASM-612). Every struct the host passes into a call is written by the
//! host in the plugin's declared minor and is valid only during that call (REQ-P-11), like every
//! pointer inside it. The plugin's result is the `UmbraKinematicState` the host passes as `out`,
//! pre-filled with the input state; it must be finite with a unit quaternion (within 1e-9, ASM-616),
//! otherwise the capability faults.

use core::marker::{PhantomData, PhantomPinned};

use crate::behavior::UmbraIntent;
use crate::consts::UmbraCode;
use crate::services::UmbraTerrain;
use crate::types::{UmbraErrorSink, UmbraQuat, UmbraStr, UmbraVec3};

/// Opaque handle to one configured motion model. Created by the plugin in `configure`, owned by the
/// plugin, destroyed only through `destroy`. The host never dereferences it. In C an incomplete type.
#[repr(C)]
pub struct UmbraMotionModel {
    _p: [u8; 0],
    _m: PhantomData<(*mut u8, PhantomPinned)>,
}

/// The kinematic state of one entity (`umbra_core::KinematicState`). Written by the host (the input
/// state, and the pre-filled `out` of `step`) in the plugin's declared minor; the plugin overwrites the
/// fields of `out`.
#[repr(C)]
pub struct UmbraKinematicState {
    /// ABI version of the writer (host-written: the plugin's declared minor).
    pub abi_version: u32,
    /// `sizeof` this struct in the layout of the plugin's declared minor.
    pub struct_size: u32,
    /// ECEF position (m).
    pub position: UmbraVec3,
    /// ECEF velocity (m/s).
    pub velocity: UmbraVec3,
    /// Orientation (unit quaternion, body to ECEF).
    pub orientation: UmbraQuat,
}

/// One `step` call (ADR 0050). Written by the host; valid only during the call, like everything it
/// points to.
#[repr(C)]
pub struct UmbraMotionStep {
    /// ABI version the host was compiled against.
    pub abi_version: u32,
    /// `sizeof` this struct in the layout of the plugin's declared minor.
    pub struct_size: u32,
    /// Current tick number when the host knows it, else 0. The core motion model is time-free (a step
    /// is a function of state, intents, `dt` and terrain only), so the ABI 0.2 host writes 0; a model
    /// must not depend on it (ASM-720).
    pub tick: u64,
    /// Tick rate in Hz: `1 / dt` rounded to the nearest whole hertz, at least 1 (ASM-720).
    pub rate_hz: u32,
    /// Reserved: written as 0, ignored by readers.
    pub reserved_0: u32,
    /// Step length in seconds (`1 / rate_hz`).
    pub dt: f64,
    /// The entity's state before the step, read-only.
    pub state: *const UmbraKinematicState,
    /// Array of `intent_count` intents of this tick, in emission order (stride: the plugin's own
    /// `sizeof(UmbraIntent)`, host-written in the plugin's declared minor), read-only; null only when
    /// the count is 0.
    pub intents: *const UmbraIntent,
    /// Number of intents.
    pub intent_count: usize,
    /// Terrain queries for this call.
    pub terrain: *const UmbraTerrain,
}

/// Function table of a motion model capability (ABI 0.2, ADR 0050). Written by the plugin as a static
/// with the descriptor's lifetime; the host copies the prefix it knows (`min(struct_size, own size)`)
/// at load. Its header carries the motion kind's minor (2), never above the descriptor's. Calls into
/// one plugin are never concurrent. Every entry is a nullable pointer; the required ones are checked
/// by the host at load.
#[repr(C)]
pub struct UmbraMotionVTable {
    /// ABI version the plugin was compiled against.
    pub abi_version: u32,
    /// `sizeof` this struct as the plugin knows it.
    pub struct_size: u32,
    /// Builds a configured model from `params` (JSON text, borrowed during the call) into `*out`.
    /// Required. A non-`UMBRA_OK` result (a panic included) or `UMBRA_OK` with a null `*out` is a
    /// build error naming the capability, not a fault.
    pub configure: Option<
        unsafe extern "C" fn(
            params: UmbraStr,
            out: *mut *mut UmbraMotionModel,
            err: *const UmbraErrorSink,
        ) -> UmbraCode,
    >,
    /// Destroys a configured model. Required; cannot report errors.
    pub destroy: Option<unsafe extern "C" fn(model: *mut UmbraMotionModel)>,
    /// Advances one entity by `step.dt`: writes the new state into `*out` (host-owned, pre-filled with
    /// `*step.state`). Required. A non-`UMBRA_OK` result or an invalid `*out` faults the capability.
    pub step: Option<
        unsafe extern "C" fn(
            model: *mut UmbraMotionModel,
            step: *const UmbraMotionStep,
            out: *mut UmbraKinematicState,
            err: *const UmbraErrorSink,
        ) -> UmbraCode,
    >,
}
