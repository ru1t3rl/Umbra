//! A plugin behavior instance as a core [`BehaviorInstance`] (REQ-B-5, REQ-B-10, ADR 0013
//! amendment, ADR 0020, ASM-096).
//!
//! `tick` is a batch of one through the capability's batch (the path of a plugin behavior used as a
//! `subgraph` child); `halt` and `halt_running` (the vtable entry when present, else `halt`, the
//! full reset) and `set_input` / `output` (through `UmbraValue`) are single calls whose non-OK
//! result, panic or contract breach faults the capability. On a faulted capability they do nothing
//! (`output` is `None`). The instance holds its behavior, so instances are destroyed first;
//! `destroy_instance` runs even when the capability is faulted.

#![expect(
    unsafe_code,
    reason = "plugin instance handle and calls through the vtable"
)]

use std::any::Any;
use std::sync::Arc;

use umbra_core::{
    BatchItem, BbValue, BehaviorBatch, BehaviorContext, BehaviorInstance, BlackboardError,
    EntityView, Intent, IntentBuffer, Status,
};
use umbra_plugin_api::{UMBRA_OK, UmbraBehavior, UmbraInstance};

use super::batch::PluginCapability;
use super::behavior::BehaviorHandle;
use super::marshal;
use super::sinks;
use crate::log::LogLevel;
use crate::tokens::{self, OutputCall, Sink};

/// One instance of a plugin behavior (see the module docs).
pub(crate) struct PluginInstance {
    behavior: Arc<BehaviorHandle>,
    handle: *mut UmbraInstance,
    /// Reused intent buffer of the single-item `tick` path.
    single: Vec<Intent>,
}

// SAFETY: the instance handle is an opaque plugin pointer that the host never dereferences; it is
// only passed to calls into the plugin, which take the image's blocking call lock, so the plugin
// never sees two calls at once even when instances move between threads (ADR 0019 "Threads and
// re-entry": calls may come from different threads over time, plugins must not rely on thread-local
// state).
unsafe impl Send for PluginInstance {}

impl PluginInstance {
    /// An instance of `behavior` owning the plugin handle `handle` (non-null).
    pub(crate) fn new(behavior: Arc<BehaviorHandle>, handle: *mut UmbraInstance) -> Self {
        Self {
            behavior,
            handle,
            single: Vec::new(),
        }
    }

    /// The capability this instance belongs to.
    pub(crate) fn capability(&self) -> &Arc<PluginCapability> {
        self.behavior.capability()
    }

    /// The (behavior, instance) handles for a batch item.
    pub(crate) fn handles(&self) -> (*mut UmbraBehavior, *mut UmbraInstance) {
        (self.behavior.handle(), self.handle)
    }

    /// A halt-like call (`halt`, `halt_running`).
    fn halt_call(
        &self,
        call: &'static str,
        entry: unsafe extern "C" fn(*mut UmbraInstance) -> i32,
    ) {
        let cap = self.capability();
        if cap.faulted() {
            return;
        }
        let handle = self.handle;
        // SAFETY: `entry` is a non-null vtable entry of this capability's image (open while the
        // capability lives); `handle` is a live instance of it; the call lock serialises calls.
        let outcome = cap.call(|_, _| unsafe { entry(handle) });
        match outcome {
            Ok((code, finished)) => {
                if let Some(problem) = PluginCapability::problem(code, &finished) {
                    cap.fault(call, problem);
                }
            }
            Err(e) => cap.fault(call, e.to_string()),
        }
    }
}

impl BehaviorInstance for PluginInstance {
    fn tick(
        &mut self,
        ctx: &mut BehaviorContext<'_>,
        entity: &EntityView<'_>,
        out: &mut IntentBuffer<'_>,
    ) -> Status {
        let cap = Arc::clone(self.capability());
        let mut buffer = std::mem::take(&mut self.single);
        buffer.clear();
        let status = {
            let mut items = [BatchItem::new(
                BehaviorContext::new(ctx.now, &mut *ctx.rng),
                *entity,
                self,
                IntentBuffer::new(&mut buffer),
            )];
            cap.tick_batch(&mut items);
            items[0].status
        };
        for intent in buffer.drain(..) {
            out.push(intent);
        }
        self.single = buffer;
        status
    }

    fn halt(&mut self) {
        let halt = self.capability().vtable().halt;
        self.halt_call("halt", halt);
    }

    fn halt_running(&mut self) {
        match self.capability().vtable().halt_running {
            Some(entry) => self.halt_call("halt_running", entry),
            None => self.halt(),
        }
    }

    fn output(&self, port: &str) -> Option<BbValue> {
        let cap = self.capability();
        let declared = *cap.capability().metadata.outputs.get(port)?;
        let entry = cap.vtable().output?;
        if cap.faulted() {
            return None;
        }
        let (handle, minor) = (self.handle, cap.minor());
        let outcome = cap.call(|scope, _| {
            if tokens::with_top(|r| r.output = Some(OutputCall::default())).is_none() {
                return (UMBRA_OK, None, false);
            }
            let sink = sinks::value_sink(scope.sink(Sink::Value), minor);
            // SAFETY: `entry` is the vtable's `output` of this capability's open image; `handle` is
            // a live instance; `port` and `sink` outlive the call; the call lock serialises calls.
            let code = unsafe { entry(handle, marshal::text(port), &raw const sink) };
            let output = tokens::with_top(|r| r.output.take()).flatten();
            let reported = output.as_ref().is_some_and(|o| o.calls > 0);
            (code, output.and_then(|o| o.value), reported)
        });
        let problem = match outcome {
            Err(e) => Some(e.to_string()),
            Ok(((code, value, reported), finished)) => {
                match PluginCapability::problem(code, &finished) {
                    Some(problem) => Some(problem),
                    None if !reported => Some(format!(
                        "output {port}: returned UMBRA_OK without reporting a value"
                    )),
                    None => match value {
                        Some(v) if v.ty() != declared => Some(format!(
                            "output {port}: reported {} for an output declared {declared}",
                            v.ty()
                        )),
                        value => return value,
                    },
                }
            }
        };
        if let Some(problem) = problem {
            cap.fault("output", problem);
        }
        None
    }

    fn set_input(&mut self, param: &str, value: Option<BbValue>) -> Result<(), BlackboardError> {
        let cap = self.capability();
        let key = || param.to_owned();
        let Some(&declared) = cap.capability().metadata.param_types.get(param) else {
            return Err(BlackboardError::Undeclared { key: key() });
        };
        if let Some(v) = &value {
            if v.ty() != declared || !marshal::has_abi_form(v.ty()) {
                return Err(BlackboardError::WrongType {
                    key: key(),
                    expected: declared,
                    found: v.ty(),
                });
            }
            if !marshal::finite(v) {
                return Err(BlackboardError::NonFinite { key: key() });
            }
        }
        let Some(entry) = cap.vtable().set_input else {
            return Err(BlackboardError::Undeclared { key: key() });
        };
        if cap.faulted() {
            return Ok(());
        }
        let raw = value.as_ref().map(|v| marshal::value_out(v, cap.minor()));
        let handle = self.handle;
        let outcome = cap.call(|scope, _| {
            let ptr = raw.as_ref().map_or(std::ptr::null(), std::ptr::from_ref);
            // SAFETY: `entry` is the vtable's `set_input` of this capability's open image; `handle`
            // is a live instance; `param`, `raw` (and the string it borrows from `value`) and the
            // error sink outlive the call; the call lock serialises calls.
            unsafe { entry(handle, marshal::text(param), ptr, scope.error_sink()) }
        });
        match outcome {
            Ok((code, finished)) => {
                if let Some(problem) = PluginCapability::problem(code, &finished) {
                    cap.fault("set_input", problem);
                }
            }
            Err(e) => cap.fault("set_input", e.to_string()),
        }
        Ok(())
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn Any> {
        Some(self)
    }
}

impl Drop for PluginInstance {
    /// `destroy_instance`, also for a faulted capability (ADR 0021); a re-entrant drop leaks the
    /// handle (logged) rather than deadlock.
    fn drop(&mut self) {
        let cap = self.capability();
        let destroy = cap.vtable().destroy_instance;
        let handle = self.handle;
        // SAFETY: `destroy_instance` is a required vtable entry of the open image; `handle` is a
        // live instance destroyed exactly once, here; the call lock serialises calls.
        if let Err(e) = cap.call(|_, _| unsafe { destroy(handle) }) {
            cap.host().log.log(
                cap.plugin(),
                LogLevel::Error,
                &format!("capability {}: destroy_instance skipped: {e}", cap.id()),
            );
        }
    }
}
