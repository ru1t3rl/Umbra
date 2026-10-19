//! Per-host state: the fault state of the engine the host belongs to (ADR 0021 "Fault policy", R-M3,
//! ASM-118), the host's log and mode, and whether an engine took the host.
//!
//! A fault is a panic caught by the SDK, a non-OK code, an unset or invalid status, an invalid intent or
//! value, or a token or callback misuse. On the first fault of a capability the host records it (in
//! occurrence order), marks the capability faulted, logs it once at error level, and under `abort`
//! keeps it as the pending abort, handed out once by [`HostState::take_abort`]. A faulted capability is
//! skipped by this host from then on, under both policies: its items fail without a plugin call,
//! `configure` and `instantiate` fail, `halt`, `set_input` and `output` do nothing, `destroy_*` still
//! runs. Nothing here is shared with other hosts: two hosts over one image have independent fault state.
//!
//! Lock order (ADR 0019): the fault lock is a leaf taken after the per-image call lock; the log is
//! called after the fault lock is released.

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use umbra_core::{ComponentId, PluginFaultPolicy, PluginId};

use crate::api::{HostMode, PluginFault};
use crate::log::{LogLevel, PluginLog};

/// A capability of this host: (plugin index in load order, capability index in its descriptor).
pub(crate) type CapabilityKey = (usize, usize);

/// What a host shares with the bridge objects it creates.
pub(crate) struct HostState {
    /// The host's log.
    pub(crate) log: Arc<dyn PluginLog>,
    /// Run or validation mode.
    pub(crate) mode: HostMode,
    attached: AtomicBool,
    faults: Mutex<Faults>,
}

#[derive(Default)]
struct Faults {
    policy: PluginFaultPolicy,
    faulted: BTreeSet<CapabilityKey>,
    list: Vec<PluginFault>,
    abort: Option<PluginFault>,
}

impl HostState {
    /// A host state with no faults and the `abort` policy.
    pub(crate) fn new(log: Arc<dyn PluginLog>, mode: HostMode) -> Self {
        Self {
            log,
            mode,
            attached: AtomicBool::new(false),
            faults: Mutex::new(Faults::default()),
        }
    }

    /// Recovers a poisoned lock: every update below is a single push or insert, so the state is
    /// consistent at every point a panic could occur.
    fn lock(&self) -> MutexGuard<'_, Faults> {
        self.faults.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Marks the host taken by an engine; `false` if one already took it.
    pub(crate) fn try_attach(&self) -> bool {
        !self.attached.swap(true, Ordering::SeqCst)
    }

    /// Sets the policy applied to later faults.
    pub(crate) fn set_policy(&self, policy: PluginFaultPolicy) {
        self.lock().policy = policy;
    }

    /// Whether `key` is faulted for this host.
    pub(crate) fn is_faulted(&self, key: CapabilityKey) -> bool {
        self.lock().faulted.contains(&key)
    }

    /// The faulted capabilities (for the `plugins()` snapshot).
    pub(crate) fn faulted(&self) -> BTreeSet<CapabilityKey> {
        self.lock().faulted.clone()
    }

    /// Records a fault of `key` (the first one per capability; later ones are ignored, the capability is
    /// already skipped) and logs it at error level.
    pub(crate) fn fault(
        &self,
        key: CapabilityKey,
        plugin: &PluginId,
        capability: &ComponentId,
        call: &'static str,
        message: String,
    ) {
        let fault = PluginFault {
            plugin: plugin.clone(),
            capability: capability.clone(),
            call,
            message,
        };
        {
            let mut faults = self.lock();
            if !faults.faulted.insert(key) {
                return;
            }
            faults.list.push(fault.clone());
            if faults.policy == PluginFaultPolicy::Abort && faults.abort.is_none() {
                faults.abort = Some(fault.clone());
            }
        }
        self.log.log(
            plugin,
            LogLevel::Error,
            &format!(
                "capability {} faulted in {}: {}",
                fault.capability, fault.call, fault.message
            ),
        );
    }

    /// Faults in occurrence order.
    pub(crate) fn faults(&self) -> Vec<PluginFault> {
        self.lock().list.clone()
    }

    /// The pending abort, once.
    pub(crate) fn take_abort(&self) -> Option<PluginFault> {
        self.lock().abort.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log::TracingLog;

    fn ids() -> (PluginId, ComponentId) {
        (
            PluginId::new("test.plugin.fault").unwrap_or_else(|e| panic!("{e}")),
            ComponentId::new("test.behavior.fault").unwrap_or_else(|e| panic!("{e}")),
        )
    }

    #[test]
    fn first_fault_per_capability_counts_and_abort_is_handed_out_once() {
        let state = HostState::new(Arc::new(TracingLog), HostMode::Run);
        let (p, c) = ids();
        state.fault((0, 0), &p, &c, "tick_batch", "one".to_owned());
        state.fault((0, 0), &p, &c, "halt", "two".to_owned());
        assert_eq!(state.faults().len(), 1);
        assert!(state.is_faulted((0, 0)));
        assert!(!state.is_faulted((0, 1)));
        assert_eq!(state.take_abort().map(|f| f.call), Some("tick_batch"));
        assert!(state.take_abort().is_none());
    }

    #[test]
    fn disable_capability_records_without_an_abort_and_attach_is_once() {
        let state = HostState::new(Arc::new(TracingLog), HostMode::Run);
        state.set_policy(PluginFaultPolicy::DisableCapability);
        let (p, c) = ids();
        state.fault((1, 2), &p, &c, "output", "bad".to_owned());
        assert!(state.take_abort().is_none());
        assert_eq!(state.faulted().into_iter().collect::<Vec<_>>(), [(1, 2)]);
        assert!(state.try_attach());
        assert!(!state.try_attach());
    }
}
