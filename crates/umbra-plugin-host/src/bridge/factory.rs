//! The registry factory of a plugin behavior capability, and the host's build scope (ADR 0021
//! RR-m2, ASM-118, ASM-138).
//!
//! `build` is the plugin's `configure` (params as JSON text). A non-OK result (a panic included) or
//! an OK with a null handle is a [`BuildError`] with the plugin's message, not a fault (R-M9); on a
//! faulted capability `build` fails without a call.
//!
//! **Build scope (RR-m2).** A run-mode host's factories build only while that host's build scope is
//! open on the calling thread ([`crate::PluginHost::build_scope`]); `umbra-engine` opens it around
//! loading a scenario (the loader dry-runs `configure`) and, for the engine the host is attached to,
//! around building and spawning (`instantiate_with_plugins`). Outside it, `build` fails with "plugin host not attached to this engine", so a cloned registry cannot build
//! plugin behaviors for an engine that does not own the host's fault state. A validation-mode
//! host's factories build anywhere (dry runs); its instances refuse to tick. The open scopes are a
//! thread-local stack of host ids (process-unique, never reused, ASM-138), pushed by the guard and
//! popped when it drops: call-scoped, like the call records, not persistent global state. A guard
//! leaked with `mem::forget` leaves its id on the stack, but no other host ever has that id.

#![expect(
    unsafe_code,
    reason = "calls the plugin's configure through the vtable"
)]

use std::cell::RefCell;
use std::marker::PhantomData;
use std::sync::Arc;

use umbra_core::{Behavior, BehaviorId, BuildError, ComponentId, Factory, ParamSchema, ParamValue};
use umbra_plugin_api::{UMBRA_OK, UmbraBehavior};

use super::batch::PluginCapability;
use super::behavior::PluginBehavior;
use super::marshal;
use crate::api::HostMode;
use crate::fault::HostState;

thread_local! {
    /// Hosts whose build scope is open on this thread, innermost last.
    static SCOPES: RefCell<Vec<u64>> = const { RefCell::new(Vec::new()) };
}

/// The identity of a host in the scope stack: its process-unique id (never reused, unlike an
/// address).
fn identity(host: &HostState) -> u64 {
    host.id()
}

/// While alive, the factories of the host that opened it build on this thread (RR-m2). Not `Send`:
/// dropped on the thread that opened it.
#[must_use = "the scope closes when the guard drops"]
pub struct BuildScope<'host> {
    host: u64,
    _host: PhantomData<&'host HostState>,
    _thread_bound: PhantomData<*const ()>,
}

impl std::fmt::Debug for BuildScope<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BuildScope").finish_non_exhaustive()
    }
}

impl<'host> BuildScope<'host> {
    /// Opens the scope of `host` on this thread.
    pub(crate) fn open(host: &'host Arc<HostState>) -> Self {
        let id = identity(host);
        // If thread-local storage is unusable the scope simply stays closed (builds are refused).
        let _ = SCOPES.try_with(|s| s.borrow_mut().push(id));
        Self {
            host: id,
            _host: PhantomData,
            _thread_bound: PhantomData,
        }
    }
}

impl Drop for BuildScope<'_> {
    fn drop(&mut self) {
        let id = self.host;
        let _ = SCOPES.try_with(|s| {
            let mut s = s.borrow_mut();
            if let Some(at) = s.iter().rposition(|&h| h == id) {
                s.remove(at);
            }
        });
    }
}

/// Whether `host`'s build scope is open on this thread.
fn in_scope(host: &Arc<HostState>) -> bool {
    let id = identity(host);
    SCOPES
        .try_with(|s| s.borrow().contains(&id))
        .unwrap_or(false)
}

/// The factory registered for one capability of one host.
pub(crate) struct PluginFactory {
    capability: Arc<PluginCapability>,
}

impl PluginFactory {
    /// The factory of `capability`.
    pub(crate) fn new(capability: Arc<PluginCapability>) -> Self {
        Self { capability }
    }
}

impl Factory<dyn Behavior> for PluginFactory {
    fn id(&self) -> &ComponentId {
        self.capability.id()
    }

    fn param_schema(&self) -> &ParamSchema {
        &self.capability.capability().schema
    }

    fn build(&self, params: &ParamValue) -> Result<Arc<dyn Behavior>, BuildError> {
        let cap = &self.capability;
        let fail =
            |message: String| BuildError::new(format!("plugin behavior {}: {message}", cap.id()));
        if cap.host().mode == HostMode::Run && !in_scope(cap.host()) {
            return Err(fail("plugin host not attached to this engine".to_owned()));
        }
        if cap.faulted() {
            return Err(fail("the capability is faulted for this host".to_owned()));
        }
        let id = BehaviorId::new(cap.id().as_str()).map_err(|e| fail(e.to_string()))?;
        let params = serde_json::to_string(params).map_err(|e| fail(e.to_string()))?;
        let configure = cap.vtable().configure;
        let mut out: *mut UmbraBehavior = std::ptr::null_mut();
        // SAFETY: `configure` is a required entry of the open image's vtable; `params`, `out` and
        // the error sink outlive the call; the call lock serialises calls.
        let outcome = cap.call(|scope, _| unsafe {
            configure(marshal::text(&params), &raw mut out, scope.error_sink())
        });
        match outcome {
            Err(e) => Err(fail(format!("configure: {e}"))),
            Ok((code, finished)) => {
                if let Some(problem) = PluginCapability::problem(code, &finished) {
                    return Err(fail(format!("configure {problem}")));
                }
                if code == UMBRA_OK && out.is_null() {
                    return Err(fail(
                        "configure returned UMBRA_OK with a null handle".to_owned(),
                    ));
                }
                Ok(Arc::new(PluginBehavior::new(Arc::clone(cap), out, id)))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log::TracingLog;

    #[test]
    fn a_scope_is_open_only_while_its_guard_lives() {
        let host = Arc::new(HostState::new(Arc::new(TracingLog), HostMode::Run));
        assert!(!in_scope(&host));
        {
            let _scope = BuildScope::open(&host);
            assert!(in_scope(&host));
        }
        assert!(!in_scope(&host));
    }

    #[test]
    fn a_forgotten_scope_never_opens_a_later_host() {
        let first = Arc::new(HostState::new(Arc::new(TracingLog), HostMode::Run));
        std::mem::forget(BuildScope::open(&first));
        assert!(
            in_scope(&first),
            "the leaked guard keeps the first host's scope open"
        );
        drop(first);
        for _ in 0..64 {
            // A later host may well get the freed allocation; its id is new.
            let later = Arc::new(HostState::new(Arc::new(TracingLog), HostMode::Run));
            assert!(!in_scope(&later));
        }
    }
}
