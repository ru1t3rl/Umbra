//! The network adapter queries and commands (REQ-X-5, ADR 0035): list the adapters of the session
//! and enable or disable one by its scenario name. Each adapter is its own runner in the manager
//! (ASM-410), so a command touches the named adapter only.

use umbra_core::{ComponentId, ComponentKind};
use umbra_net::{AdapterStatus, ManagerError};

use super::thread::{Run, Worker};
use super::{AdapterReport, HandleError};
use crate::commands::CommandApplier;

impl Run {
    /// The report of one adapter: the component the scenario named and, for a plugin adapter, the
    /// plugin that provides it.
    fn adapter_report(&self, adapter: ComponentId, status: AdapterStatus) -> AdapterReport {
        let plugin = self
            .session
            .plugins()
            .plugins()
            .iter()
            .find(|p| {
                p.capabilities
                    .iter()
                    .any(|c| c.kind == ComponentKind::NetworkAdapter && c.id == adapter)
            })
            .map(|p| p.id.to_string());
        AdapterReport {
            adapter,
            plugin,
            status,
        }
    }
}

impl Worker {
    /// Every adapter of the session in scenario order; empty when the scenario has none.
    pub(super) fn network_adapters(&self) -> Result<Vec<AdapterReport>, HandleError> {
        let run = self.run.as_ref().ok_or(HandleError::NoSession)?;
        let Some(link) = run.session.engine().network() else {
            return Ok(Vec::new());
        };
        let specs = &run.session.scenario().network_adapters;
        // Paired by name: every status of the link has its scenario entry.
        Ok(link
            .status()
            .into_iter()
            .filter_map(|status| {
                let spec = specs.iter().find(|s| s.name == status.name)?;
                Some(run.adapter_report(spec.adapter.clone(), status))
            })
            .collect())
    }

    /// Enables or disables the adapter called `name`. The manager is touched between ticks (this
    /// runs on the engine thread), idempotently; a faulted adapter cannot be enabled.
    pub(super) fn set_adapter_enabled(
        &mut self,
        name: &str,
        enabled: bool,
    ) -> Result<AdapterReport, HandleError> {
        let run = self.run.as_mut().ok_or(HandleError::NoSession)?;
        let not_found = || HandleError::AdapterNotFound(name.to_owned());
        let adapter = run
            .session
            .scenario()
            .network_adapters
            .iter()
            .find(|s| s.name == name)
            .map(|s| s.adapter.clone())
            .ok_or_else(not_found)?;
        let status = CommandApplier::set_adapter_enabled(run.session.engine_mut(), name, enabled)
            .map_err(|e| match e {
            ManagerError::Faulted(name) => HandleError::AdapterFaulted(name),
            // `ManagerError` is non-exhaustive; the name was found in the scenario above.
            _ => not_found(),
        })?;
        Ok(run.adapter_report(adapter, status))
    }
}
