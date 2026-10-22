//! One capability as seen by one host, and its batch (REQ-B-4, ADR 0020, ADR 0021, ASM-118).
//!
//! [`PluginCapability`] is created once per (host, loaded capability) and shared by `Arc` by the
//! registered factory, every behavior it builds and every instance of those: it is the capability's
//! [`BehaviorBatch`] (one `Arc`, so the kernel groups all of them into one `tick_batch` call per
//! tick) and the place calls are made and faults recorded. It is generic over the copied vtable of
//! its kind: `PluginCapability` (the default) is a behavior, `PluginCapability<NetworkVTable>` a
//! network adapter (`bridge::network`, ADR 0033); calls, faults and the call verdict are shared.
//!
//! A batch call: items whose instance is not one of this capability's (per host) are ticked through
//! their own `tick` first, outside any plugin call; then, unless the capability is faulted for this
//! host or the host is a validation host (both: every item Failure, no plugin call), all others go
//! to the plugin in **one** `tick_batch` call, under the image's blocking call lock, with views
//! written in the plugin's minor, statuses pre-filled `UNSET`, one RNG token per item and the
//! intents buffered. The call is clean when the plugin returned `UMBRA_OK`, no callback recorded a
//! breach, every status is valid and the capability was not faulted during the call (re-entry from
//! the same thread faults it); only then are statuses and intents committed. Otherwise the whole
//! batch is discarded: every item is Failure with no intents, and RNG draws made before the fault
//! stay consumed (ADR 0020).

#![expect(
    unsafe_code,
    reason = "calls the plugin's tick_batch with host-built batch structs"
)]

use std::sync::Arc;

use umbra_core::{BatchItem, BehaviorBatch, ComponentId, PluginId, Status};
use umbra_plugin_api::{
    UMBRA_ABI_VERSION, UMBRA_OK, UMBRA_STATUS_UNSET, UmbraBehaviorBatch, UmbraBehaviorItem,
};

use super::instance::PluginInstance;
use super::marshal::{self, Scratch, written_size};
use super::sinks;
use crate::api::HostMode;
use crate::descriptor::{BehaviorVTable, Capability};
use crate::fault::{CapabilityKey, HostState};
use crate::lifecycle::{CallError, CallScope, LoadedImage, describe};
use crate::tokens::{self, Finished, Sink};

/// One capability of one host (see the module docs), with the copied vtable `V` of its kind.
pub(crate) struct PluginCapability<V = BehaviorVTable> {
    image: Arc<LoadedImage>,
    index: usize,
    key: CapabilityKey,
    host: Arc<HostState>,
    vtable: V,
}

/// The fault message of a call that returned `code` with what it left in its record; `None` when
/// the call was clean.
pub(crate) fn problem(code: i32, finished: &Finished) -> Option<String> {
    match (&finished.breach, code) {
        (Some(breach), UMBRA_OK) => Some(breach.clone()),
        (Some(breach), code) => Some(format!("{breach} (the call {})", describe(code))),
        (None, UMBRA_OK) => None,
        (None, code) => Some(match &finished.error {
            Some(message) => format!("{}: {message}", describe(code)),
            None => describe(code),
        }),
    }
}

/// How a batch call ended.
enum Verdict {
    /// Statuses and intents committed.
    Clean,
    /// Discarded because the capability was faulted during the call (already recorded).
    Faulted,
    /// Discarded; record this fault.
    Fault(String),
}

impl<V> PluginCapability<V> {
    /// The capability `index` of `image` with its copied `vtable`, as host `host` (fault key `key`)
    /// sees it.
    pub(crate) fn new(
        image: Arc<LoadedImage>,
        index: usize,
        key: CapabilityKey,
        host: Arc<HostState>,
        vtable: V,
    ) -> Self {
        Self {
            image,
            index,
            key,
            host,
            vtable,
        }
    }

    /// The copied capability.
    pub(crate) fn capability(&self) -> &Capability {
        &self.image.descriptor().capabilities[self.index]
    }

    /// The copied vtable.
    pub(crate) fn vtable(&self) -> &V {
        &self.vtable
    }

    /// The plugin id.
    pub(crate) fn plugin(&self) -> &PluginId {
        &self.image.descriptor().id
    }

    /// The component id.
    pub(crate) fn id(&self) -> &ComponentId {
        &self.capability().id
    }

    /// The plugin's declared minor (host-written structs use its layout, R-B3).
    pub(crate) fn minor(&self) -> u32 {
        self.image.descriptor().abi.1
    }

    /// The host this capability belongs to.
    pub(crate) fn host(&self) -> &Arc<HostState> {
        &self.host
    }

    /// Whether this host has faulted the capability.
    pub(crate) fn faulted(&self) -> bool {
        self.host.is_faulted(self.key)
    }

    /// Records a fault of this capability for this host.
    pub(crate) fn fault(&self, call: &'static str, message: String) {
        self.host
            .fault(self.key, self.plugin(), self.id(), call, message);
    }

    /// One call into the plugin (the image's call lock, call record, error sink; see `lifecycle`).
    pub(crate) fn call<R>(
        &self,
        f: impl FnOnce(&CallScope, &mut Scratch) -> R,
    ) -> Result<(R, Finished), CallError> {
        self.image.call(&self.host.log, f)
    }
}

impl PluginCapability {
    /// [`problem`], for the behavior bridge.
    pub(crate) fn problem(code: i32, finished: &Finished) -> Option<String> {
        problem(code, finished)
    }

    /// Whether `item`'s instance is one of this capability's (for this host).
    fn owns(&self, item: &mut BatchItem<'_>) -> bool {
        self.plugin_instance(item).is_some()
    }

    fn plugin_instance<'i>(&self, item: &'i mut BatchItem<'_>) -> Option<&'i mut PluginInstance> {
        item.instance
            .as_any_mut()?
            .downcast_mut::<PluginInstance>()
            .filter(|i| std::ptr::eq(Arc::as_ptr(i.capability()), self))
    }

    /// Marshals the members, makes the call and commits or discards (runs under the call lock).
    fn run(
        &self,
        scope: &CallScope,
        scratch: &mut Scratch,
        items: &mut [BatchItem<'_>],
    ) -> Verdict {
        let minor = self.minor();
        scratch.clear();
        let mut now = None;
        for (i, item) in items.iter_mut().enumerate() {
            let Some(instance) = self.plugin_instance(item) else {
                continue;
            };
            let handles = instance.handles();
            scratch.members.push(i);
            scratch.handles.push(handles);
            let span = scratch.push_entity_data(&item.entity, minor);
            scratch.spans.push(span);
            now.get_or_insert(item.ctx.now);
            // The plugin draws through this pointer only during the call below; the item is not
            // touched by the host meanwhile (ADR 0020 "Marshalling").
            scratch
                .batch
                .rngs
                .push(std::ptr::from_mut(&mut *item.ctx.rng));
        }
        let Some(now) = now else {
            return Verdict::Clean;
        };
        for k in 0..scratch.members.len() {
            let view = scratch.view(&items[scratch.members[k]].entity, scratch.spans[k], minor);
            scratch.views.push(view);
        }
        let generation = scope.generation();
        let count = scratch.members.len();
        for k in 0..count {
            let (behavior, instance) = scratch.handles[k];
            let entity = &raw const scratch.views[k];
            scratch.items.push(UmbraBehaviorItem {
                abi_version: UMBRA_ABI_VERSION,
                struct_size: written_size::<UmbraBehaviorItem>(minor),
                behavior,
                instance,
                entity,
                rng: tokens::rng_token(generation, k),
            });
        }
        scratch.statuses.resize(count, UMBRA_STATUS_UNSET);
        scratch.batch.counts.resize(count, 0);
        let sink = sinks::intent_sink(scope.sink(Sink::Intent), minor);
        let batch = UmbraBehaviorBatch {
            abi_version: UMBRA_ABI_VERSION,
            struct_size: written_size::<UmbraBehaviorBatch>(minor),
            tick: now.tick(),
            rate_hz: now.rate().hz(),
            reserved_0: 0,
            items: scratch.items.as_ptr(),
            count,
            statuses: scratch.statuses.as_mut_ptr(),
            intents: &raw const sink,
        };
        let state = std::mem::take(&mut scratch.batch);
        let returned = tokens::with_top(|r| r.batch = Some(state));
        if returned.is_none() {
            return Verdict::Fault("no call record on this thread (thread-local storage)".into());
        }
        let tick_batch = self.vtable().tick_batch;
        // SAFETY: `tick_batch` is a required, non-null entry of this image's validated vtable and
        // the library is open (this capability holds the image). Every pointer in `batch` points
        // into the scratch or the items, which are neither moved nor touched until the call
        // returns; the sink and the error sink live on this frame; the instances and behaviors are
        // live handles of this capability; calls into the image are serialised by the call lock. A
        // plugin cannot unwind.
        let code = unsafe { tick_batch(&raw const batch, scope.error_sink()) };
        let (state, error, breach) =
            tokens::with_top(|r| (r.batch.take(), r.error.clone(), r.breach.take()))
                .unwrap_or_default();
        scratch.batch = state.unwrap_or_default();
        let verdict = self.judge(code, &Finished { error, breach }, scratch, items);
        if let Verdict::Clean = verdict {
            for (k, &i) in scratch.members.iter().enumerate() {
                if let Ok(status) = marshal::status(scratch.statuses[k]) {
                    items[i].status = status;
                }
            }
            for (k, intent) in scratch.batch.intents.drain(..) {
                items[scratch.members[k]].out.push(intent);
            }
        } else {
            for &i in &scratch.members {
                items[i].status = Status::Failure;
            }
        }
        scratch.clear();
        verdict
    }

    /// The verdict of a finished call.
    fn judge(
        &self,
        code: i32,
        finished: &Finished,
        scratch: &Scratch,
        items: &[BatchItem<'_>],
    ) -> Verdict {
        if let Some(problem) = Self::problem(code, finished) {
            return Verdict::Fault(problem);
        }
        for (k, &raw) in scratch.statuses.iter().enumerate() {
            if let Err(why) = marshal::status(raw) {
                let entity = items[scratch.members[k]].entity.id;
                return Verdict::Fault(format!("item {k} (entity {entity}): {why}"));
            }
        }
        if self.faulted() {
            return Verdict::Faulted;
        }
        Verdict::Clean
    }
}

impl BehaviorBatch for PluginCapability {
    fn tick_batch(&self, items: &mut [BatchItem<'_>]) {
        let mut ours = 0_usize;
        for item in items.iter_mut() {
            if self.owns(item) {
                ours += 1;
            } else {
                // ADR 0020 / ASM-123: not ours, ticked through its own `tick` (never inside our
                // call).
                item.tick_instance();
            }
        }
        if ours == 0 {
            return;
        }
        if self.host.mode == HostMode::Validation || self.faulted() {
            for item in items.iter_mut() {
                if self.owns(item) {
                    item.status = Status::Failure;
                }
            }
            return;
        }
        let outcome = self.call(|scope, scratch| self.run(scope, scratch, items));
        let problem = match outcome {
            Ok((Verdict::Fault(message), _)) => Some(message),
            Ok((Verdict::Clean | Verdict::Faulted, _)) => None,
            Err(e) => Some(e.to_string()),
        };
        if let Some(message) = problem {
            for item in items.iter_mut() {
                if self.owns(item) {
                    item.status = Status::Failure;
                }
            }
            self.fault("tick_batch", message);
        }
    }
}
