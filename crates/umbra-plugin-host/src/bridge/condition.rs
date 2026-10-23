//! The condition bridge (ABI 0.2; REQ-P-4, REQ-P-5, REQ-P-7, REQ-B-3, REQ-B-11, REQ-B-15, D-8, ADR
//! 0021, ADR 0050, ASM-612, ASM-618, ASM-643, ASM-723): a plugin condition capability as a core
//! `Factory<dyn Condition>`, and its configured condition as a core [`Condition`], used by a behavior
//! graph exactly like a built-in.
//!
//! **Lifecycle.** `build` resolves the leaf's grants from its params (`bridge::leaf`), then calls the
//! plugin's `configure` ([`configure_handle`]); every built condition owns one plugin handle,
//! destroyed (`destroy`) when the condition drops, also for a faulted capability. The condition holds
//! its capability and so its image.
//!
//! **A check** is one leaf call (`bridge::leaf`) with the result byte pre-filled
//! `UMBRA_CONDITION_UNSET`: 1 is true and 0 false; a non-OK result (a panic included), a refused
//! service call (a breach) or any other byte (unset included) faults the capability for this host
//! under the scenario's policy, undoes the call's blackboard writes and answers false; a faulted
//! condition answers false from then on without a call. A validation host never calls a plugin
//! condition (false).

#![expect(
    unsafe_code,
    reason = "plugin condition handle and calls through the vtable"
)]

use std::sync::Arc;

use umbra_core::{
    BehaviorContext, Blackboard, BuildError, ComponentId, Condition, EntityView, Factory,
    ParamSchema, ParamValue,
};
use umbra_plugin_api::{UMBRA_CONDITION_UNSET, UmbraCondition};

use super::batch::PluginCapability;
use super::factory::configure_handle;
use super::leaf::{self, Inputs, Refused};
use crate::api::HostMode;
use crate::descriptor::ConditionVTable;
use crate::log::LogLevel;
use crate::tokens::Grant;

/// A condition capability of one host.
pub(crate) type ConditionCapability = PluginCapability<ConditionVTable>;

/// The registry factory of one condition capability of one host.
pub(crate) struct PluginConditionFactory {
    capability: Arc<ConditionCapability>,
}

impl PluginConditionFactory {
    /// The factory of `capability`.
    pub(crate) fn new(capability: Arc<ConditionCapability>) -> Self {
        Self { capability }
    }
}

impl Factory<dyn Condition> for PluginConditionFactory {
    fn id(&self) -> &ComponentId {
        self.capability.id()
    }

    fn param_schema(&self) -> &ParamSchema {
        &self.capability.capability().schema
    }

    /// The grants from `params`, then `configure` with `params` as JSON text; in a run-mode host only
    /// inside the host's build scope (RR-m2), like behaviors.
    fn build(&self, params: &ParamValue) -> Result<Arc<dyn Condition>, BuildError> {
        let cap = &self.capability;
        let grants = leaf::grants(cap.capability(), params)
            .map_err(|e| BuildError::new(format!("plugin condition {}: {e}", cap.id())))?;
        let raw = configure_handle(cap, "condition", params, cap.vtable().configure)?;
        Ok(Arc::new(PluginCondition {
            capability: Arc::clone(cap),
            raw,
            grants,
        }))
    }
}

/// A configured plugin condition (see the module docs).
pub(crate) struct PluginCondition {
    capability: Arc<ConditionCapability>,
    raw: *mut UmbraCondition,
    grants: Box<[Grant]>,
}

// SAFETY: the handle is an opaque plugin pointer the host never dereferences; it is only passed to
// calls into the plugin, which take the image's blocking call lock (ADR 0019 "Threads and re-entry"),
// so sending the condition to another thread never lets the plugin see concurrent calls. Exercised by
// `leaf_bridge.rs`
// `given_a_plugin_condition_shared_with_another_thread_then_it_checks_there_and_drops_last_there`.
unsafe impl Send for PluginCondition {}
// SAFETY: as above; `&PluginCondition` only reads the pointer value and the grants and calls through
// the lock (the same test checks one condition from two threads).
unsafe impl Sync for PluginCondition {}

/// The answer of a check that wrote `byte`.
fn answer(byte: u8) -> Result<bool, String> {
    match byte {
        0 => Ok(false),
        1 => Ok(true),
        UMBRA_CONDITION_UNSET => {
            Err("check left its result unset (UMBRA_CONDITION_UNSET)".to_owned())
        }
        other => Err(format!("check result {other} is not 0 or 1")),
    }
}

impl Condition for PluginCondition {
    fn id(&self) -> &ComponentId {
        self.capability.id()
    }

    fn check(
        &self,
        ctx: &mut BehaviorContext<'_>,
        entity: &EntityView<'_>,
        bb: &mut Blackboard,
    ) -> bool {
        let cap = &self.capability;
        if cap.host().mode == HostMode::Validation || cap.faulted() {
            return false;
        }
        let entry = cap.vtable().check;
        let raw = self.raw;
        let input = Inputs {
            ctx,
            entity,
            bb,
            grants: &self.grants,
        };
        let outcome = leaf::call(
            cap,
            input,
            None,
            |call, _, err| {
                let mut out = UMBRA_CONDITION_UNSET;
                // SAFETY: `check` is a required, non-null entry of this image's validated vtable and
                // the library is open (this condition holds the capability, which holds the image).
                // `raw` is a live configured condition; the call record, `out` and the error sink
                // live on the caller's frame until the call returns (`leaf::call`); calls into the
                // image are serialised by the call lock. A plugin cannot unwind.
                let code = unsafe { entry(raw, call, &raw mut out, err) };
                (code, out)
            },
            answer,
        );
        match outcome {
            Ok(answer) => answer,
            Err(Refused::Faulted) => false,
            Err(Refused::Fault(message)) => {
                cap.fault("check", message);
                false
            }
        }
    }
}

impl Drop for PluginCondition {
    /// `destroy`, also for a faulted capability (ADR 0021); a re-entrant drop leaks the handle (logged)
    /// rather than deadlock.
    fn drop(&mut self) {
        let cap = &self.capability;
        let destroy = cap.vtable().destroy;
        let raw = self.raw;
        // SAFETY: `destroy` is a required entry of the open image's vtable (the capability holds the
        // image); `raw` is a live configured condition, destroyed exactly once, here; the call lock
        // serialises calls.
        if let Err(e) = cap.call(|_, _| unsafe { destroy(raw) }) {
            cap.host().log.log(
                cap.plugin(),
                LogLevel::Error,
                &format!("capability {}: destroy skipped: {e}", cap.id()),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_0_and_1_are_answers() {
        assert_eq!(answer(0), Ok(false));
        assert_eq!(answer(1), Ok(true));
        assert!(answer(UMBRA_CONDITION_UNSET).is_err_and(|e| e.contains("unset")));
        assert!(answer(2).is_err_and(|e| e.contains("not 0 or 1")));
    }
}
