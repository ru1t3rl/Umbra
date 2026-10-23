//! The condition and action capabilities of `export_plugin!` (ABI 0.2, ADR 0050): the two vtables and
//! their `extern "C"` trampolines over [`Condition`], [`Action`] and [`ActionInstance`].
//!
//! A configured condition, a configured action and an action instance are each one plugin-owned
//! handle tagged per capability (an action passed as an instance is refused). `check` and `tick`
//! validate the whole leaf call (entity view, blackboard service) and their out-pointers before the
//! trait runs. A result (the condition's 0 or 1, the action's status) is written only when the trait
//! succeeded and the host refused none of the call's service, RNG or intent calls; otherwise the call
//! returns the error's code (or the host's) and leaves `out` as the host pre-filled it (unset), so the
//! host faults the capability (ADR 0050). An action's intents carry the action kind's minor, item 0.
//! `halt` has no error sink: a panic in it is logged at error level and is `UMBRA_ERR_PANIC`.
#![expect(
    unsafe_code,
    reason = "the condition and action vtables of the plugin ABI (ADR 0050)"
)]

use std::marker::PhantomData;

use super::{
    Cap, VTable, configure_into, destroy, handle, host_refusal, put_contacts, take_contacts,
    without_sink, write_out,
};
use crate::abi::{
    UMBRA_ERR_INVALID_ARGUMENT, UMBRA_OK, UmbraAction, UmbraActionInstance, UmbraActionVTable,
    UmbraCode, UmbraCondition, UmbraConditionVTable, UmbraErrorSink, UmbraIntentSink,
    UmbraLeafCall, UmbraStatus, UmbraStr,
};
use crate::behavior::{EntityView, Fault, Intents};
use crate::convert::{IntentSink, leaf_in, rate, size_of_u32, status_code};
use crate::error::Error;
use crate::guard::with_error_sink;
use crate::leaf::{Action, ActionInstance, Condition, LeafCtx};
use crate::minor::{ACTION_MINOR, CONDITION_MINOR, abi_version};
use crate::services::BlackboardAccess;

/// The tag of condition `C`'s handles.
struct ConditionTag<C>(PhantomData<C>);
/// The tag of action `A`'s configured actions.
struct ActionTag<A>(PhantomData<A>);
/// The tag of action `A`'s instances.
struct ActionInstanceTag<A>(PhantomData<A>);

/// The capability of condition `C`: every vtable entry is filled; the metadata is `C::METADATA`.
pub(crate) fn condition_cap<C: Condition>() -> Cap {
    let vtable = UmbraConditionVTable {
        abi_version: abi_version(CONDITION_MINOR),
        struct_size: size_of_u32::<UmbraConditionVTable>(),
        configure: Some(configure_condition::<C>),
        destroy: Some(destroy_condition::<C>),
        check: Some(check::<C>),
    };
    Cap {
        minor: CONDITION_MINOR,
        id: C::ID,
        version: C::VERSION.unwrap_or(""),
        schema: C::SCHEMA,
        metadata: C::METADATA.to_owned(),
        vtable: VTable::Condition(vtable),
    }
}

/// The capability of action `A`: every vtable entry is filled; the metadata is `A::METADATA`.
pub(crate) fn action_cap<A: Action>() -> Cap {
    let vtable = UmbraActionVTable {
        abi_version: abi_version(ACTION_MINOR),
        struct_size: size_of_u32::<UmbraActionVTable>(),
        configure: Some(configure_action::<A>),
        destroy_action: Some(destroy_action::<A>),
        instantiate: Some(instantiate::<A>),
        destroy_instance: Some(destroy_instance::<A>),
        tick: Some(tick::<A>),
        halt: Some(halt::<A>),
    };
    Cap {
        minor: ACTION_MINOR,
        id: A::ID,
        version: A::VERSION.unwrap_or(""),
        schema: A::SCHEMA,
        metadata: A::METADATA.to_owned(),
        vtable: VTable::Action(vtable),
    }
}

/// Runs `body` with the safe pieces of a validated leaf call: its context, entity view and
/// blackboard; returns the trait's result unless the host refused a call during it.
///
/// # Safety
/// `call` is null or a host-written leaf call valid for this call (the vtable contract).
unsafe fn with_leaf<R>(
    call: *const UmbraLeafCall,
    minor: u32,
    body: impl FnOnce(
        &mut LeafCtx<'_>,
        &EntityView<'_>,
        &mut BlackboardAccess<'_>,
        &Fault,
    ) -> Result<R, Error>,
) -> Result<R, Error> {
    // SAFETY: forwarded from this function's contract.
    let c = unsafe { leaf_in(call, minor) }?;
    let mut contacts = take_contacts();
    c.view.contacts_into(&mut contacts);
    let fault = Fault::default();
    let result = {
        let entity = EntityView::new(&c.view, &contacts);
        let mut ctx = LeafCtx::new(c.tick, c.rate_hz, crate::host::current(), c.rng, &fault);
        let mut bb = BlackboardAccess::new(c.blackboard, &fault);
        body(&mut ctx, &entity, &mut bb, &fault)
    };
    put_contacts(contacts);
    host_refusal(&fault).map_or(result, Err)
}

// ---------------------------------------------------------------- conditions

unsafe extern "C" fn configure_condition<C: Condition>(
    params: UmbraStr,
    out: *mut *mut UmbraCondition,
    err: *const UmbraErrorSink,
) -> UmbraCode {
    // SAFETY: the vtable contract (ADR 0050): `params`, `out` and `err` are valid for this call.
    unsafe { configure_into::<ConditionTag<C>, _, _>(params, out, err, C::configure) }
}

unsafe extern "C" fn destroy_condition<C: Condition>(condition: *mut UmbraCondition) {
    // SAFETY: the vtable contract: a handle from `configure`, destroyed once.
    unsafe { destroy::<ConditionTag<C>, C>(condition.cast(), "destroy") };
}

unsafe extern "C" fn check<C: Condition>(
    condition: *mut UmbraCondition,
    call: *const UmbraLeafCall,
    out: *mut u8,
    err: *const UmbraErrorSink,
) -> UmbraCode {
    let body = || {
        if out.is_null() {
            return Err(Error::invalid("out: null"));
        }
        // SAFETY: the vtable contract: a live handle from `configure`; the reference ends with the call.
        let condition = unsafe { handle::<ConditionTag<C>, C>(condition.cast()) }
            .ok_or_else(|| Error::invalid("check: not a condition of this capability"))?;
        // SAFETY: the vtable contract: `call` and everything it points to are valid for this call.
        let answer = unsafe {
            with_leaf(call, CONDITION_MINOR, |ctx, entity, bb, _| {
                condition.value.check(ctx, entity, bb)
            })
        }?;
        // SAFETY: `out` is non-null (checked; a `u8` is always aligned) and host-owned, writable for
        // this call (the vtable contract).
        unsafe { out.write(u8::from(answer)) };
        Ok(())
    };
    // SAFETY: the vtable contract: `err` is null or a sink valid for this call.
    unsafe { with_error_sink(err, body) }
}

// ---------------------------------------------------------------- actions

type InstanceOf<A> = <A as Action>::Instance;

unsafe extern "C" fn configure_action<A: Action>(
    params: UmbraStr,
    out: *mut *mut UmbraAction,
    err: *const UmbraErrorSink,
) -> UmbraCode {
    // SAFETY: the vtable contract (ADR 0050): `params`, `out` and `err` are valid for this call.
    unsafe { configure_into::<ActionTag<A>, _, _>(params, out, err, A::configure) }
}

unsafe extern "C" fn destroy_action<A: Action>(action: *mut UmbraAction) {
    // SAFETY: the vtable contract: a handle from `configure`, destroyed once, after its instances.
    unsafe { destroy::<ActionTag<A>, A>(action.cast(), "destroy_action") };
}

unsafe extern "C" fn instantiate<A: Action>(
    action: *mut UmbraAction,
    rate_hz: u32,
    out: *mut *mut UmbraActionInstance,
    err: *const UmbraErrorSink,
) -> UmbraCode {
    let body = || {
        // SAFETY: `out` is the call's out-pointer.
        unsafe { write_out(out, std::ptr::null_mut()) }?;
        // As in a call record (ASM-715(c)); the behavior kind's `instantiate` is unchanged.
        rate(rate_hz, "instantiate")?;
        // SAFETY: the vtable contract: a live handle from `configure`.
        let action = unsafe { handle::<ActionTag<A>, A>(action.cast()) }
            .ok_or_else(|| Error::invalid("instantiate: not an action of this capability"))?;
        let instance = action.value.instantiate(rate_hz)?;
        let handle =
            super::new_handle::<ActionInstanceTag<A>, _>(instance).cast::<UmbraActionInstance>();
        // SAFETY: as above (checked by the first write).
        unsafe { write_out(out, handle) }
    };
    // SAFETY: the vtable contract: `err` is null or a sink valid for this call.
    unsafe { with_error_sink(err, body) }
}

unsafe extern "C" fn destroy_instance<A: Action>(instance: *mut UmbraActionInstance) {
    // SAFETY: the vtable contract: a handle from `instantiate`, destroyed once.
    unsafe {
        destroy::<ActionInstanceTag<A>, InstanceOf<A>>(instance.cast(), "destroy_instance");
    };
}

unsafe extern "C" fn tick<A: Action>(
    instance: *mut UmbraActionInstance,
    call: *const UmbraLeafCall,
    intents: *const UmbraIntentSink,
    out: *mut UmbraStatus,
    err: *const UmbraErrorSink,
) -> UmbraCode {
    let body = || {
        if out.is_null() || !out.is_aligned() {
            return Err(Error::invalid("out: null or misaligned"));
        }
        // SAFETY: the vtable contract: `intents` is the host's sink for this call.
        let sink = unsafe { IntentSink::from_raw(intents, ACTION_MINOR) }?;
        // SAFETY: the vtable contract: a live handle from `instantiate`; the reference ends with the
        // call.
        let instance = unsafe { handle::<ActionInstanceTag<A>, InstanceOf<A>>(instance.cast()) }
            .ok_or_else(|| Error::invalid("tick: not an action instance of this capability"))?;
        // SAFETY: the vtable contract: `call` and everything it points to are valid for this call.
        let status = unsafe {
            with_leaf(call, ACTION_MINOR, |ctx, entity, bb, fault| {
                let mut intents = Intents::new(sink, 0, fault);
                instance.value.tick(ctx, entity, bb, &mut intents)
            })
        }?;
        // SAFETY: `out` is non-null and aligned (checked) and host-owned, writable for this call (the
        // vtable contract).
        unsafe { out.write(status_code(status)) };
        Ok(())
    };
    // SAFETY: the vtable contract: `err` is null or a sink valid for this call.
    unsafe { with_error_sink(err, body) }
}

unsafe extern "C" fn halt<A: Action>(instance: *mut UmbraActionInstance) -> UmbraCode {
    without_sink("halt", || {
        // SAFETY: the vtable contract: a live handle from `instantiate`.
        match unsafe { handle::<ActionInstanceTag<A>, InstanceOf<A>>(instance.cast()) } {
            Some(h) => {
                h.value.halt();
                UMBRA_OK
            }
            None => UMBRA_ERR_INVALID_ARGUMENT,
        }
    })
}
