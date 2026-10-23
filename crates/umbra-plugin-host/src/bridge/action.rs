//! The action bridge (ABI 0.2; REQ-P-4, REQ-P-5, REQ-P-7, REQ-B-3, REQ-B-11, REQ-B-15, D-8, ADR 0020,
//! ADR 0021, ADR 0050, ASM-612, ASM-618, ASM-643, ASM-723): a plugin action capability as a core
//! `Factory<dyn Action>`, its configured action as a core [`Action`] and its instances as core
//! [`ActionInstance`]s, used by a behavior graph exactly like a built-in.
//!
//! **Lifecycle.** `build` resolves the leaf's grants from its params (`bridge::leaf`), then calls the
//! plugin's `configure` ([`configure_handle`]). The configured handle lives in an [`ActionHandle`]
//! shared with the instances, so `destroy_action` runs after the last instance's `destroy_instance`,
//! also for a faulted capability. `instantiate` calls the vtable's `instantiate`; a non-OK result (a
//! panic included) or an OK with a null handle is an [`InstantiateError`] naming the capability, not
//! a fault (R-M9); on a faulted capability it fails without a call.
//!
//! **A tick** is one leaf call (`bridge::leaf`) with the status pre-filled `UMBRA_STATUS_UNSET` and
//! an intent sink of item 0. A non-OK result (a panic included), a refused service call or intent (a
//! breach) or a status left unset or out of range faults the capability for this host under the
//! scenario's policy: the tick is Failure, its intents are discarded and its blackboard writes undone
//! (ADR 0020, ASM-723). A faulted action ticks Failure from then on without a call. **`halt`** is a
//! single call (none on a faulted capability) whose non-OK result or breach faults the capability. A
//! validation host never calls a plugin action's `tick` or `halt` (Failure).

#![expect(
    unsafe_code,
    reason = "plugin action handles and calls through the vtable"
)]

use std::sync::Arc;

use umbra_core::{
    Action, ActionInstance, BehaviorContext, Blackboard, BuildError, ComponentId, EntityView,
    Factory, InstantiateError, IntentBuffer, ParamSchema, ParamValue, Status, TickRate,
};
use umbra_plugin_api::{UMBRA_STATUS_UNSET, UmbraAction, UmbraActionInstance};

use super::batch::{PluginCapability, problem};
use super::factory::configure_handle;
use super::leaf::{self, Inputs, Refused};
use super::marshal;
use crate::api::HostMode;
use crate::descriptor::ActionVTable;
use crate::log::LogLevel;
use crate::tokens::Grant;

/// An action capability of one host.
pub(crate) type ActionCapability = PluginCapability<ActionVTable>;

/// The registry factory of one action capability of one host.
pub(crate) struct PluginActionFactory {
    capability: Arc<ActionCapability>,
}

impl PluginActionFactory {
    /// The factory of `capability`.
    pub(crate) fn new(capability: Arc<ActionCapability>) -> Self {
        Self { capability }
    }
}

impl Factory<dyn Action> for PluginActionFactory {
    fn id(&self) -> &ComponentId {
        self.capability.id()
    }

    fn param_schema(&self) -> &ParamSchema {
        &self.capability.capability().schema
    }

    /// The grants from `params`, then `configure` with `params` as JSON text; in a run-mode host only
    /// inside the host's build scope (RR-m2), like behaviors.
    fn build(&self, params: &ParamValue) -> Result<Arc<dyn Action>, BuildError> {
        let cap = &self.capability;
        let grants = leaf::grants(cap.capability(), params)
            .map_err(|e| BuildError::new(format!("plugin action {}: {e}", cap.id())))?;
        let raw = configure_handle(cap, "action", params, cap.vtable().configure)?;
        Ok(Arc::new(PluginAction {
            inner: Arc::new(ActionHandle {
                capability: Arc::clone(cap),
                raw,
                grants,
            }),
        }))
    }
}

/// The plugin's configured action handle and the leaf's grants; destroyed when the last holder (the
/// action, an instance) drops.
pub(crate) struct ActionHandle {
    capability: Arc<ActionCapability>,
    raw: *mut UmbraAction,
    grants: Box<[Grant]>,
}

// SAFETY: the handle is an opaque plugin pointer the host never dereferences; it is only passed to
// calls into the plugin, which are serialised by the image's blocking call lock (ADR 0019 "Threads
// and re-entry"), so sharing and sending it between threads never lets the plugin see concurrent
// calls. Exercised by `leaf_bridge.rs`
// `given_a_plugin_action_shared_with_another_thread_then_its_instances_tick_there_and_drop_last_there`.
unsafe impl Send for ActionHandle {}
// SAFETY: as above; `&ActionHandle` only reads the pointer value and the grants (the same test
// instantiates from one action on two threads).
unsafe impl Sync for ActionHandle {}

impl Drop for ActionHandle {
    /// `destroy_action`, after every instance (they hold this handle), also for a faulted capability
    /// (ADR 0021); a re-entrant drop leaks the handle (logged) rather than deadlock.
    fn drop(&mut self) {
        let cap = &self.capability;
        let destroy = cap.vtable().destroy_action;
        let raw = self.raw;
        // SAFETY: `destroy_action` is a required entry of the open image's vtable (the capability
        // holds the image); `raw` is a live configured action, destroyed exactly once, here, after
        // every instance; the call lock serialises calls.
        if let Err(e) = cap.call(|_, _| unsafe { destroy(raw) }) {
            cap.host().log.log(
                cap.plugin(),
                LogLevel::Error,
                &format!("capability {}: destroy_action skipped: {e}", cap.id()),
            );
        }
    }
}

/// A configured plugin action (see the module docs).
pub(crate) struct PluginAction {
    inner: Arc<ActionHandle>,
}

impl Action for PluginAction {
    fn id(&self) -> &ComponentId {
        self.inner.capability.id()
    }

    fn instantiate(&self, rate: TickRate) -> Result<Box<dyn ActionInstance>, InstantiateError> {
        let cap = &self.inner.capability;
        let fail = |message: String| InstantiateError::Other {
            message: format!("plugin action {}: {message}", cap.id()),
        };
        if cap.faulted() {
            return Err(fail("the capability is faulted for this host".to_owned()));
        }
        let entry = cap.vtable().instantiate;
        let action = self.inner.raw;
        let mut out: *mut UmbraActionInstance = std::ptr::null_mut();
        // SAFETY: `instantiate` is a required entry of the open image's vtable; `action` is a live
        // configured action; `out` and the error sink outlive the call; the call lock serialises
        // calls.
        let outcome = cap
            .call(|scope, _| unsafe { entry(action, rate.hz(), &raw mut out, scope.error_sink()) });
        let (code, finished) = outcome.map_err(|e| fail(format!("instantiate: {e}")))?;
        if let Some(problem) = problem(code, &finished) {
            return Err(fail(format!("instantiate {problem}")));
        }
        if out.is_null() {
            return Err(fail(
                "instantiate returned UMBRA_OK with a null handle".to_owned(),
            ));
        }
        Ok(Box::new(PluginActionInstance {
            action: Arc::clone(&self.inner),
            raw: out,
        }))
    }
}

/// One instance of a plugin action (see the module docs).
pub(crate) struct PluginActionInstance {
    action: Arc<ActionHandle>,
    raw: *mut UmbraActionInstance,
}

// SAFETY: the instance handle is an opaque plugin pointer that the host never dereferences; it is
// only passed to calls into the plugin, which take the image's blocking call lock, so the plugin never
// sees two calls at once even when instances move between threads (ADR 0019 "Threads and re-entry").
// Exercised by the same `leaf_bridge.rs` test (an instance ticked and dropped on another thread).
unsafe impl Send for PluginActionInstance {}

impl ActionInstance for PluginActionInstance {
    fn tick(
        &mut self,
        ctx: &mut BehaviorContext<'_>,
        entity: &EntityView<'_>,
        bb: &mut Blackboard,
        out: &mut IntentBuffer<'_>,
    ) -> Status {
        let cap = &self.action.capability;
        if cap.host().mode == HostMode::Validation || cap.faulted() {
            return Status::Failure;
        }
        let entry = cap.vtable().tick;
        let raw = self.raw;
        let input = Inputs {
            ctx,
            entity,
            bb,
            grants: &self.action.grants,
        };
        let outcome = leaf::call(
            cap,
            input,
            Some(out),
            |call, sink, err| {
                let mut status = UMBRA_STATUS_UNSET;
                // SAFETY: `tick` is a required, non-null entry of this image's validated vtable and
                // the library is open (this instance holds the action, which holds the capability and
                // so the image). `raw` is a live instance; the call record, the intent sink, `status`
                // and the error sink live on the caller's frames until the call returns
                // (`leaf::call`); calls into the image are serialised by the call lock. A plugin
                // cannot unwind.
                let code = unsafe { entry(raw, call, sink, &raw mut status, err) };
                (code, status)
            },
            marshal::status,
        );
        match outcome {
            Ok(status) => status,
            Err(Refused::Faulted) => Status::Failure,
            Err(Refused::Fault(message)) => {
                cap.fault("tick", message);
                Status::Failure
            }
        }
    }

    fn halt(&mut self) {
        let cap = &self.action.capability;
        if cap.host().mode == HostMode::Validation || cap.faulted() {
            return;
        }
        let entry = cap.vtable().halt;
        let raw = self.raw;
        // SAFETY: `halt` is a required entry of this capability's open image; `raw` is a live instance
        // of it; the call lock serialises calls.
        let outcome = cap.call(|_, _| unsafe { entry(raw) });
        let problem = match outcome {
            Ok((code, finished)) => problem(code, &finished),
            Err(e) => Some(e.to_string()),
        };
        if let Some(message) = problem {
            cap.fault("halt", message);
        }
    }
}

impl Drop for PluginActionInstance {
    /// `destroy_instance`, also for a faulted capability (ADR 0021); a re-entrant drop leaks the
    /// handle (logged) rather than deadlock.
    fn drop(&mut self) {
        let cap = &self.action.capability;
        let destroy = cap.vtable().destroy_instance;
        let raw = self.raw;
        // SAFETY: `destroy_instance` is a required vtable entry of the open image; `raw` is a live
        // instance destroyed exactly once, here; the call lock serialises calls.
        if let Err(e) = cap.call(|_, _| unsafe { destroy(raw) }) {
            cap.host().log.log(
                cap.plugin(),
                LogLevel::Error,
                &format!("capability {}: destroy_instance skipped: {e}", cap.id()),
            );
        }
    }
}
