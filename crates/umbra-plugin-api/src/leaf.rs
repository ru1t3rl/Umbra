//! The behavior-graph leaf capabilities (ABI 0.2, ADR 0050, ADR 0020, REQ-B-3, REQ-B-15): conditions
//! and actions, their opaque handles, the leaf call and the two vtables.
//!
//! A condition is configured once per graph node and checked per node tick; an action is configured
//! once per graph node and instantiated per entity, like a behavior (ADR 0020). One call per node tick
//! (not batched, ASM-612). Every struct the host passes into a call is written by the host in the
//! plugin's declared minor and is valid only during that call (REQ-P-11), like every pointer inside
//! it. The capability's metadata declares its blackboard params:
//! `{"blackboard_params": [{"param", "type", "access"}], "result"}` (ASM-643); the call's
//! `UmbraBlackboard` grants exactly those keys.

use core::marker::{PhantomData, PhantomPinned};

use crate::behavior::{UmbraEntityView, UmbraIntentSink};
use crate::consts::{UmbraCode, UmbraStatus, UmbraToken};
use crate::services::UmbraBlackboard;
use crate::types::{UmbraErrorSink, UmbraStr};

/// Opaque handle to one configured condition. Created by the plugin in `configure`, owned by the
/// plugin, destroyed only through `destroy`. The host never dereferences it. In C an incomplete type.
#[repr(C)]
pub struct UmbraCondition {
    _p: [u8; 0],
    _m: PhantomData<(*mut u8, PhantomPinned)>,
}

/// Opaque handle to one configured action. Created by the plugin in `configure`, owned by the plugin,
/// destroyed only through `destroy_action` after every instance made from it was destroyed. The host
/// never dereferences it. In C an incomplete type.
#[repr(C)]
pub struct UmbraAction {
    _p: [u8; 0],
    _m: PhantomData<(*mut u8, PhantomPinned)>,
}

/// Opaque handle to one action instance (per entity). Created by the plugin in `instantiate`, owned by
/// the plugin, destroyed only through `destroy_instance`. The host never dereferences it. In C an
/// incomplete type.
#[repr(C)]
pub struct UmbraActionInstance {
    _p: [u8; 0],
    _m: PhantomData<(*mut u8, PhantomPinned)>,
}

/// One condition `check` or action `tick` call (ADR 0050). Written by the host; valid only during the
/// call, like everything it points to.
#[repr(C)]
pub struct UmbraLeafCall {
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
    /// Read-only view of the entity's pre-tick state.
    pub entity: *const UmbraEntityView,
    /// Opaque RNG token of the entity's stream, valid only during this call (REQ-K-4); used with
    /// `UmbraHostApi::rng_next_u64` and `rng_next_f64`.
    pub rng: UmbraToken,
    /// Blackboard access for this call (the declared keys only).
    pub blackboard: *const UmbraBlackboard,
}

/// Function table of a condition capability (ABI 0.2, ADR 0050). Written by the plugin as a static with
/// the descriptor's lifetime; the host copies the prefix it knows (`min(struct_size, own size)`) at
/// load. Its header carries the condition kind's minor (2), never above the descriptor's. Calls into
/// one plugin are never concurrent. Every entry is a nullable pointer; the required ones are checked by
/// the host at load.
#[repr(C)]
pub struct UmbraConditionVTable {
    /// ABI version the plugin was compiled against.
    pub abi_version: u32,
    /// `sizeof` this struct as the plugin knows it.
    pub struct_size: u32,
    /// Builds a configured condition from `params` (JSON text, borrowed during the call) into `*out`.
    /// Required. A non-`UMBRA_OK` result (a panic included) or `UMBRA_OK` with a null `*out` is a
    /// build error naming the capability, not a fault.
    pub configure: Option<
        unsafe extern "C" fn(
            params: UmbraStr,
            out: *mut *mut UmbraCondition,
            err: *const UmbraErrorSink,
        ) -> UmbraCode,
    >,
    /// Destroys a configured condition. Required; cannot report errors.
    pub destroy: Option<unsafe extern "C" fn(condition: *mut UmbraCondition)>,
    /// Evaluates the condition: writes 1 (true) or 0 (false) into `*out` (host-owned, pre-filled with
    /// `UMBRA_CONDITION_UNSET`). Required. A non-`UMBRA_OK` result or any other value in `*out` faults
    /// the capability.
    pub check: Option<
        unsafe extern "C" fn(
            condition: *mut UmbraCondition,
            call: *const UmbraLeafCall,
            out: *mut u8,
            err: *const UmbraErrorSink,
        ) -> UmbraCode,
    >,
}

/// Function table of an action capability (ABI 0.2, ADR 0050). Written by the plugin as a static with
/// the descriptor's lifetime; the host copies the prefix it knows (`min(struct_size, own size)`) at
/// load. Its header carries the action kind's minor (2), never above the descriptor's. Calls into one
/// plugin are never concurrent. Every entry is a nullable pointer; the required ones are checked by the
/// host at load.
#[repr(C)]
pub struct UmbraActionVTable {
    /// ABI version the plugin was compiled against.
    pub abi_version: u32,
    /// `sizeof` this struct as the plugin knows it.
    pub struct_size: u32,
    /// Builds a configured action from `params` (JSON text, borrowed during the call) into `*out`.
    /// Required. A non-`UMBRA_OK` result (a panic included) or `UMBRA_OK` with a null `*out` is a
    /// build error naming the capability, not a fault.
    pub configure: Option<
        unsafe extern "C" fn(
            params: UmbraStr,
            out: *mut *mut UmbraAction,
            err: *const UmbraErrorSink,
        ) -> UmbraCode,
    >,
    /// Destroys a configured action. Required; cannot report errors.
    pub destroy_action: Option<unsafe extern "C" fn(action: *mut UmbraAction)>,
    /// Creates an instance of `action` ticked at `rate_hz` into `*out`. Required. A non-`UMBRA_OK`
    /// result or `UMBRA_OK` with a null `*out` is a build error, not a fault.
    pub instantiate: Option<
        unsafe extern "C" fn(
            action: *mut UmbraAction,
            rate_hz: u32,
            out: *mut *mut UmbraActionInstance,
            err: *const UmbraErrorSink,
        ) -> UmbraCode,
    >,
    /// Destroys an instance. Required; cannot report errors.
    pub destroy_instance: Option<unsafe extern "C" fn(instance: *mut UmbraActionInstance)>,
    /// Ticks the instance once: writes its status into `*out` (host-owned, pre-filled with
    /// `UMBRA_STATUS_UNSET`) and pushes intents to `intents` (item index 0). Required. A non-`UMBRA_OK`
    /// result or a status left unset or out of range faults the capability and discards the call's
    /// intents (ADR 0020).
    pub tick: Option<
        unsafe extern "C" fn(
            instance: *mut UmbraActionInstance,
            call: *const UmbraLeafCall,
            intents: *const UmbraIntentSink,
            out: *mut UmbraStatus,
            err: *const UmbraErrorSink,
        ) -> UmbraCode,
    >,
    /// Halts the instance (full reset): idempotent, callable when it is not running. Required. A
    /// non-`UMBRA_OK` result is a fault.
    pub halt: Option<unsafe extern "C" fn(instance: *mut UmbraActionInstance) -> UmbraCode>,
}
