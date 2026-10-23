//! The motion model capability of `export_plugin!` (ABI 0.2, ADR 0050): the vtable and its
//! `extern "C"` trampolines over [`MotionModel`].
//!
//! One `configure` makes one plugin-owned handle, a `Box<Handle<M>>` tagged per capability like the
//! behavior handles. `step` validates the whole step record (state, intents, terrain service) and the
//! host's `out` before the trait runs, and writes the trait's result into `out` only when no terrain
//! query was refused during the step (the host then faulted the capability). The result itself is
//! passed on unchanged: whether it is finite and unit is the host's check (ADR 0050).
#![expect(
    unsafe_code,
    reason = "the motion model vtable of the plugin ABI (ADR 0050)"
)]

use std::marker::PhantomData;

use super::{Cap, VTable, configure_into, destroy, handle, host_refusal};
use crate::abi::{
    UmbraCode, UmbraErrorSink, UmbraKinematicState, UmbraMotionModel, UmbraMotionStep,
    UmbraMotionVTable, UmbraStr,
};
use crate::behavior::Fault;
use crate::convert::{size_of_u32, state_out, step_in};
use crate::error::Error;
use crate::guard::with_error_sink;
use crate::minor::{MOTION_MODEL_MINOR, abi_version};
use crate::motion::{MotionCtx, MotionModel};

/// The tag of model `M`'s handles.
struct MotionTag<M>(PhantomData<M>);

/// The capability of motion model `M`: every vtable entry is filled; the metadata is empty (ADR 0050).
pub(crate) fn motion_cap<M: MotionModel>() -> Cap {
    let vtable = UmbraMotionVTable {
        abi_version: abi_version(MOTION_MODEL_MINOR),
        struct_size: size_of_u32::<UmbraMotionVTable>(),
        configure: Some(configure::<M>),
        destroy: Some(destroy_model::<M>),
        step: Some(step::<M>),
    };
    Cap {
        minor: MOTION_MODEL_MINOR,
        id: M::ID,
        version: M::VERSION.unwrap_or(""),
        schema: M::SCHEMA,
        metadata: String::new(),
        vtable: VTable::Motion(vtable),
    }
}

unsafe extern "C" fn configure<M: MotionModel>(
    params: UmbraStr,
    out: *mut *mut UmbraMotionModel,
    err: *const UmbraErrorSink,
) -> UmbraCode {
    // SAFETY: the vtable contract (ADR 0050): `params`, `out` and `err` are valid for this call.
    unsafe { configure_into::<MotionTag<M>, _, _>(params, out, err, M::configure) }
}

unsafe extern "C" fn destroy_model<M: MotionModel>(model: *mut UmbraMotionModel) {
    // SAFETY: the vtable contract: a handle from `configure`, destroyed once.
    unsafe { destroy::<MotionTag<M>, M>(model.cast(), "destroy") };
}

unsafe extern "C" fn step<M: MotionModel>(
    model: *mut UmbraMotionModel,
    step: *const UmbraMotionStep,
    out: *mut UmbraKinematicState,
    err: *const UmbraErrorSink,
) -> UmbraCode {
    let body = || {
        // SAFETY: the vtable contract: `step` and everything it points to are valid for this call.
        let call = unsafe { step_in(step) }?;
        // SAFETY: the vtable contract: `out` is host-owned and writable for this call.
        let out = unsafe { state_out(out) }?;
        // SAFETY: the vtable contract: a live handle from `configure`; the reference ends with the call.
        let model = unsafe { handle::<MotionTag<M>, M>(model.cast()) }
            .ok_or_else(|| Error::invalid("step: not a motion model of this capability"))?;
        let fault = Fault::default();
        let ctx = MotionCtx::new(call.tick, call.rate_hz, call.terrain, &fault);
        let next = model.value.step(&ctx, &call.state, &call.intents, call.dt);
        if let Some(refusal) = host_refusal(&fault) {
            return Err(refusal);
        }
        out.write(&next);
        Ok(())
    };
    // SAFETY: the vtable contract: `err` is null or a sink valid for this call.
    unsafe { with_error_sink(err, body) }
}
