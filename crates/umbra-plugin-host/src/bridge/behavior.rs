//! A configured plugin behavior as a core [`Behavior`] (REQ-B-4, REQ-B-5, ADR 0020).
//!
//! `instantiate` calls the vtable's `instantiate`; a non-OK result (a panic included) or an OK with a
//! null handle is an [`InstantiateError`] with the plugin's message, not a fault (R-M9). On a faulted
//! capability `instantiate` fails without a call. `batch()` returns the capability's one shared batch,
//! so the kernel ticks all instances of the capability in one call per tick. The configured handle
//! lives in a [`BehaviorHandle`] shared with the instances, so `destroy_behavior` runs after the last
//! instance's `destroy_instance`, also for a faulted capability.

#![expect(
    unsafe_code,
    reason = "plugin behavior handle and calls through the vtable"
)]

use std::sync::Arc;

use umbra_core::{
    Behavior, BehaviorBatch, BehaviorId, BehaviorInstance, InstantiateError, TickRate,
};
use umbra_plugin_api::{UMBRA_OK, UmbraBehavior, UmbraInstance};

use super::batch::PluginCapability;
use super::instance::PluginInstance;
use crate::log::LogLevel;

/// The plugin's configured behavior handle; destroyed when the last holder (the behavior, an
/// instance) drops.
pub(crate) struct BehaviorHandle {
    capability: Arc<PluginCapability>,
    handle: *mut UmbraBehavior,
}

// SAFETY: the handle is an opaque plugin pointer the host never dereferences; it is only passed to
// calls into the plugin, which are serialised by the image's blocking call lock (ADR 0019 "Threads and
// re-entry"), so sharing and sending it between threads never lets the plugin see concurrent calls.
unsafe impl Send for BehaviorHandle {}
// SAFETY: as above; `&BehaviorHandle` only reads the pointer value.
unsafe impl Sync for BehaviorHandle {}

impl BehaviorHandle {
    /// The capability.
    pub(crate) fn capability(&self) -> &Arc<PluginCapability> {
        &self.capability
    }

    /// The raw handle (for batch items).
    pub(crate) fn handle(&self) -> *mut UmbraBehavior {
        self.handle
    }
}

impl Drop for BehaviorHandle {
    fn drop(&mut self) {
        let cap = &self.capability;
        let destroy = cap.vtable().destroy_behavior;
        let handle = self.handle;
        // SAFETY: `destroy_behavior` is a required entry of the open image's vtable; `handle` is a live
        // configured behavior, destroyed exactly once, here, after every instance (they hold this
        // handle); the call lock serialises calls.
        if let Err(e) = cap.call(|_, _| unsafe { destroy(handle) }) {
            cap.host().log.log(
                cap.plugin(),
                LogLevel::Error,
                &format!("capability {}: destroy_behavior skipped: {e}", cap.id()),
            );
        }
    }
}

/// A configured plugin behavior (see the module docs).
pub(crate) struct PluginBehavior {
    inner: Arc<BehaviorHandle>,
    id: BehaviorId,
}

impl PluginBehavior {
    /// Wraps the configured `handle` (non-null) of `capability`.
    pub(crate) fn new(
        capability: Arc<PluginCapability>,
        handle: *mut UmbraBehavior,
        id: BehaviorId,
    ) -> Self {
        Self {
            inner: Arc::new(BehaviorHandle { capability, handle }),
            id,
        }
    }
}

impl Behavior for PluginBehavior {
    fn id(&self) -> &BehaviorId {
        &self.id
    }

    fn instantiate(&self, rate: TickRate) -> Result<Box<dyn BehaviorInstance>, InstantiateError> {
        let cap = self.inner.capability();
        let fail = |message: String| InstantiateError::Other {
            message: format!("plugin behavior {}: {message}", self.id),
        };
        if cap.faulted() {
            return Err(fail("the capability is faulted for this host".to_owned()));
        }
        let entry = cap.vtable().instantiate;
        let handle = self.inner.handle();
        let mut out: *mut UmbraInstance = std::ptr::null_mut();
        // SAFETY: `instantiate` is a required entry of the open image's vtable; `handle` is a live
        // configured behavior; `out` and the error sink outlive the call; the call lock serialises
        // calls.
        let outcome = cap
            .call(|scope, _| unsafe { entry(handle, rate.hz(), &raw mut out, scope.error_sink()) });
        match outcome {
            Err(e) => Err(fail(format!("instantiate: {e}"))),
            Ok((code, finished)) => {
                if let Some(problem) = PluginCapability::problem(code, &finished) {
                    return Err(fail(format!("instantiate {problem}")));
                }
                if code == UMBRA_OK && out.is_null() {
                    return Err(fail(
                        "instantiate returned UMBRA_OK with a null handle".to_owned(),
                    ));
                }
                Ok(Box::new(PluginInstance::new(Arc::clone(&self.inner), out)))
            }
        }
    }

    fn batch(&self) -> Option<Arc<dyn BehaviorBatch>> {
        let batch: Arc<dyn BehaviorBatch> = Arc::clone(self.inner.capability()) as _;
        Some(batch)
    }
}
