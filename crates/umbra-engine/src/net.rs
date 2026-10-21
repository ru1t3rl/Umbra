//! Engine wiring of the network layer (M6, ADR 0030): the [`NetLink`] an engine owns when its
//! scenario has `[[network_adapters]]`.
//!
//! Signature stub of M6 step 4. `Engine::network` is real (no link until WI-9 attaches one); the
//! ghost builder and `Engine::attach_network` have their bodies in WI-9.
#![expect(
    clippy::todo,
    dead_code,
    clippy::needless_pass_by_value,
    unused_variables,
    reason = "M6 step-4 stubs, bodies from WI-9"
)]

use std::collections::BTreeMap;
use std::sync::Arc;

use umbra_core::{EntityId, MotionModel, TemplateId};
use umbra_kernel::Kernel;
use umbra_net::{
    AdapterStatus, GhostError, GhostSpec, GhostWorld, ManagerError, NetworkManager, OwnedMeta,
};

use crate::engine::Engine;

/// The pre-built, never stepped motion models of the mappable templates (ASM-241). WI-9 builds
/// them at load; during ingest the engine pairs them with the kernel in [`KernelGhosts`].
pub(crate) struct GhostBuilder {
    models: BTreeMap<TemplateId, Arc<dyn MotionModel>>,
}

/// The [`GhostWorld`] the engine hands to `NetworkManager::ingest`: ghosts live in the kernel
/// (`spawn_remote`, `update_remote`, `remove`), and the models come from the builder. A borrowing
/// pair, so neither the kernel nor the builder is moved into the manager (ASM-281, WI-9 design).
pub(crate) struct KernelGhosts<'a> {
    kernel: &'a mut Kernel,
    models: &'a GhostBuilder,
}

impl GhostWorld for KernelGhosts<'_> {
    fn create_ghost(&mut self, spec: GhostSpec<'_>) -> Result<EntityId, GhostError> {
        todo!("WI-9: KernelGhosts::create_ghost")
    }

    fn update_ghost(
        &mut self,
        id: EntityId,
        remote: umbra_core::dead_reckoning::RemoteState,
    ) -> Result<(), GhostError> {
        todo!("WI-9: KernelGhosts::update_ghost")
    }

    fn remove_ghost(&mut self, id: EntityId) -> Result<(), GhostError> {
        todo!("WI-9: KernelGhosts::remove_ghost")
    }
}

/// The network state of an engine: the manager, the ghost models, the per-owned-entity metadata
/// and the per-adapter `entity_type` of each template. The manager stays private; the facade is
/// [`NetLink::status`] and [`NetLink::set_enabled`] (ASM-281).
pub struct NetLink {
    manager: NetworkManager,
    ghosts: GhostBuilder,
    meta: BTreeMap<EntityId, OwnedMeta>,
}

impl NetLink {
    /// One status per adapter, in scenario order.
    #[must_use]
    pub fn status(&self) -> Vec<AdapterStatus> {
        self.manager.status()
    }

    /// Enables or disables the adapter called `name` (the control API, AT-19).
    ///
    /// # Errors
    /// [`ManagerError::UnknownAdapter`].
    pub fn set_enabled(
        &mut self,
        name: &str,
        enabled: bool,
    ) -> Result<AdapterStatus, ManagerError> {
        todo!("WI-9: NetLink::set_enabled")
    }
}

impl Engine {
    /// The network link, `None` when the scenario has no `[[network_adapters]]`.
    #[must_use]
    pub fn network(&self) -> Option<&NetLink> {
        self.network.as_ref()
    }

    /// Attaches the link; from then on every tick ingests before and publishes after the kernel
    /// tick (ASM-224).
    pub fn attach_network(&mut self, link: NetLink) {
        todo!("WI-9: Engine::attach_network")
    }
}
