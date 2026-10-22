//! Engine wiring of the network layer (M6, ADR 0030): the [`NetLink`] an engine owns when its
//! scenario has `[[network_adapters]]`.
//!
//! The link is built before the entities are spawned ([`NetLink::connect`]); each tick the engine
//! calls `NetLink::ingest` before and `NetLink::publish` after `Kernel::tick` (ASM-224), and
//! nothing while the clock is paused (ASM-240).

use std::collections::BTreeMap;
use std::fmt::{self, Write as _};
use std::sync::Arc;

use umbra_core::net::OwnedView;
use umbra_core::net::{NetEntityType, configure_input};
use umbra_core::{
    ComponentKind, ControlAction, EntityId, MotionModel, PluginFaultPolicy, Registry, SimEvent,
    SimTime, TemplateId,
};
use umbra_kernel::{Kernel, RemoteInit};
use umbra_net::{
    AdapterSettings, AdapterSlot, AdapterStatus, GhostError, GhostPolicy, GhostSpec, GhostWorld,
    ManagerError, NetworkManager, OwnedMeta, TypeMap,
};
use umbra_templates::{NetworkAdapterSpec, Scenario};

use crate::engine::Engine;
use crate::error::EngineError;

/// A network adapter fault that ends the run under the `abort` policy (ASM-252).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct NetFault {
    /// The adapter's scenario name.
    pub adapter: String,
    /// Why it faulted.
    pub reason: String,
}

impl NetFault {
    /// A fault of the adapter named `adapter`.
    #[must_use]
    pub fn new(adapter: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            adapter: adapter.into(),
            reason: reason.into(),
        }
    }
}

impl fmt::Display for NetFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "network adapter {} faulted: {}",
            self.adapter, self.reason
        )
    }
}

/// The pre-built, never stepped motion models of the mappable templates (ASM-241). During ingest
/// the engine pairs them with the kernel in [`KernelGhosts`].
pub(crate) struct GhostBuilder {
    models: BTreeMap<TemplateId, Arc<dyn MotionModel>>,
}

impl GhostBuilder {
    /// Builds the model of every template some adapter can map an inbound entity to.
    fn build(
        registry: &Registry,
        scenario: &Scenario,
        targets: &[TemplateId],
    ) -> Result<Self, EngineError> {
        let mut models = BTreeMap::new();
        for id in targets {
            // Targets come from a validated scenario, so a missing template is a bug upstream.
            let template = scenario
                .templates
                .get(id)
                .ok_or_else(|| EngineError::UnknownTemplate(id.clone()))?;
            let factory = registry.motion_model(&template.motion.id).ok_or_else(|| {
                EngineError::UnknownComponent {
                    kind: ComponentKind::MotionModel,
                    id: template.motion.id.clone(),
                }
            })?;
            let model = factory.build(&template.motion.params).map_err(|source| {
                EngineError::GhostModelBuild {
                    template: id.clone(),
                    source,
                }
            })?;
            models.insert(id.clone(), model);
        }
        Ok(Self { models })
    }
}

/// The [`GhostWorld`] the engine hands to `NetworkManager::ingest`: ghosts live in the kernel
/// (`spawn_remote`, `update_remote`, `remove`), and the models come from the builder. A borrowing
/// pair, so neither the kernel nor the builder is moved into the manager (ASM-281).
pub(crate) struct KernelGhosts<'a> {
    kernel: &'a mut Kernel,
    models: &'a GhostBuilder,
    table: &'a mut GhostTable,
}

/// What the engine knows about a ghost besides the kernel's entity: its template and the peer's
/// display name (ASM-400). Shown in entity listings and stream frames.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GhostInfo {
    /// The template the ghost was built from.
    pub template: TemplateId,
    /// The peer's display name, if it sent one.
    pub name: Option<Arc<str>>,
}

/// The ghosts of a link by entity id, with a generation that changes whenever the set does.
#[derive(Debug, Default)]
pub(crate) struct GhostTable {
    info: BTreeMap<EntityId, GhostInfo>,
    generation: u64,
}

fn refused(e: &impl fmt::Display) -> GhostError {
    GhostError::World(e.to_string())
}

impl GhostWorld for KernelGhosts<'_> {
    fn create_ghost(&mut self, spec: GhostSpec<'_>) -> Result<EntityId, GhostError> {
        let motion =
            self.models.models.get(spec.template).ok_or_else(|| {
                GhostError::World(format!("no model for template {}", spec.template))
            })?;
        let id = self
            .kernel
            .spawn_remote(RemoteInit {
                source: spec.source,
                motion: Arc::clone(motion),
                force: spec.force,
                remote: spec.remote,
            })
            .map_err(|e| refused(&e))?;
        self.table.info.insert(
            id,
            GhostInfo {
                template: spec.template.clone(),
                name: spec.name,
            },
        );
        self.table.generation += 1;
        Ok(id)
    }

    fn update_ghost(
        &mut self,
        id: EntityId,
        remote: umbra_core::dead_reckoning::RemoteState,
    ) -> Result<(), GhostError> {
        self.kernel
            .update_remote(id, remote)
            .map_err(|e| refused(&e))
    }

    fn remove_ghost(&mut self, id: EntityId) -> Result<(), GhostError> {
        let removed = self.kernel.remove(id).map_err(|e| refused(&e));
        if self.table.info.remove(&id).is_some() {
            self.table.generation += 1;
        }
        removed
    }
}

/// The network state of an engine: the manager, the ghost models, the per-owned-entity metadata.
/// The manager stays private; the facade is [`NetLink::status`] and [`NetLink::set_enabled`]
/// (ASM-281).
pub struct NetLink {
    manager: NetworkManager,
    ghosts: GhostBuilder,
    table: GhostTable,
    meta: BTreeMap<EntityId, OwnedMeta>,
    /// Entities removed between ticks, reported to the next publish as `EntityRemoved`.
    removed: Vec<EntityId>,
    /// Reused per tick.
    owned: Vec<OwnedView>,
    policy: PluginFaultPolicy,
    faulted_seen: usize,
    reported: Vec<String>,
    abort: Option<NetFault>,
}

/// The key of a template's `network.<adapter>` table that maps a remote entity type to the template.
const ENTITY_TYPE_KEY: &str = "entity_type";

/// Every key of a template's `network.<adapter>` table that the engine reads itself (ASM-228); all
/// other keys belong to the adapter. `umbra_sdk::MANAGER_TEMPLATE_KEYS` must equal this list
/// (`tests/manager_template_keys.rs`).
pub const MANAGER_TEMPLATE_KEYS: &[&str] = &[ENTITY_TYPE_KEY];

/// The scenario's `network.<adapter>.entity_type` of each template, as the adapter publishes it.
fn declared_types(scenario: &Scenario, adapter: &str) -> Vec<(TemplateId, NetEntityType)> {
    scenario
        .templates
        .iter()
        .filter_map(|t| {
            let token = t.network.get(adapter)?.get(ENTITY_TYPE_KEY)?.as_str()?;
            Some((t.id.clone(), NetEntityType::new(token)))
        })
        .collect()
}

/// Builds one adapter's configuration (a dry-run `configure`: nothing is opened or started,
/// ASM-251) and its type map.
fn adapter_slot(
    registry: &Registry,
    scenario: &Scenario,
    spec: &NetworkAdapterSpec,
    targets: &mut Vec<TemplateId>,
) -> Result<AdapterSlot, EngineError> {
    let factory =
        registry
            .network_adapter(&spec.adapter)
            .ok_or_else(|| EngineError::UnknownComponent {
                kind: ComponentKind::NetworkAdapter,
                id: spec.adapter.clone(),
            })?;
    let tables = scenario
        .templates
        .iter()
        .filter_map(|t| t.network.get(&spec.name).map(|table| (&t.id, table)));
    let input = configure_input(&spec.name, &spec.params, tables);
    let config = factory
        .build(&input)
        .map_err(|source| EngineError::AdapterBuild {
            adapter: spec.name.clone(),
            source,
        })?;
    let declared = declared_types(scenario, &spec.name);
    let (type_map, warnings) = TypeMap::new(
        &declared,
        &spec.ghosts.type_map,
        spec.ghosts.default_template.clone(),
    );
    for w in warnings {
        tracing::warn!("network adapter {}: {w}", spec.name);
    }
    targets.extend(declared.iter().map(|(t, _)| t.clone()));
    targets.extend(spec.ghosts.type_map.iter().map(|(_, t)| t.clone()));
    targets.extend(spec.ghosts.default_template.clone());
    let settings = AdapterSettings {
        name: spec.name.clone(),
        enabled: spec.enabled,
        publish: spec.publish,
        ghosts: GhostPolicy {
            timeout: spec.ghosts.timeout,
            max: spec.ghosts.max,
            max_inbound_per_tick: spec.ghosts.max_inbound_per_tick,
        },
    };
    Ok(AdapterSlot::new(
        settings,
        config,
        type_map,
        declared.into_iter().collect(),
    ))
}

impl NetLink {
    /// Builds every adapter of `scenario` through `registry` and starts them at `now`; `None` when
    /// the scenario has no `[[network_adapters]]`. Call it before the entities are spawned, so
    /// their metadata is recorded as they appear.
    ///
    /// # Errors
    /// [`EngineError::UnknownComponent`], [`EngineError::AdapterBuild`],
    /// [`EngineError::GhostModelBuild`], [`EngineError::NetworkStart`].
    pub fn connect(
        registry: &Registry,
        scenario: &Scenario,
        now: SimTime,
    ) -> Result<Option<Self>, EngineError> {
        if scenario.network_adapters.is_empty() {
            return Ok(None);
        }
        let mut targets = Vec::new();
        let slots = scenario
            .network_adapters
            .iter()
            .map(|spec| adapter_slot(registry, scenario, spec, &mut targets))
            .collect::<Result<Vec<_>, _>>()?;
        targets.sort();
        targets.dedup();
        let ghosts = GhostBuilder::build(registry, scenario, &targets)?;
        let manager = NetworkManager::new(slots, now).map_err(EngineError::NetworkStart)?;
        Ok(Some(Self {
            manager,
            ghosts,
            table: GhostTable::default(),
            meta: BTreeMap::new(),
            removed: Vec::new(),
            owned: Vec::new(),
            policy: scenario.on_plugin_fault,
            faulted_seen: 0,
            reported: Vec::new(),
            abort: None,
        }))
    }

    /// One status per adapter, in scenario order.
    #[must_use]
    pub fn status(&self) -> Vec<AdapterStatus> {
        self.manager.status()
    }

    /// Enables or disables the adapter called `name` (the control API, AT-19).
    ///
    /// # Errors
    /// [`ManagerError::UnknownAdapter`], and [`ManagerError::Faulted`] when enabling a faulted one.
    pub fn set_enabled(
        &mut self,
        name: &str,
        enabled: bool,
    ) -> Result<AdapterStatus, ManagerError> {
        self.manager.set_enabled(name, enabled)
    }

    /// The live ghosts with their template and the peer's name, in entity id order.
    pub fn ghosts(&self) -> impl Iterator<Item = (EntityId, &GhostInfo)> {
        self.table.info.iter().map(|(id, info)| (*id, info))
    }

    /// Whether `id` is a ghost of this link.
    #[must_use]
    pub fn is_ghost(&self, id: EntityId) -> bool {
        self.table.info.contains_key(&id)
    }

    /// Changes whenever a ghost appears or goes, so a reader can tell its copy of the entity
    /// directory is stale.
    #[must_use]
    pub fn ghost_generation(&self) -> u64 {
        self.table.generation
    }

    /// Records what publishing needs to know about a new owned entity.
    pub(crate) fn note_owned(
        &mut self,
        id: EntityId,
        template: Option<TemplateId>,
        name: Option<&str>,
    ) {
        self.meta.insert(
            id,
            OwnedMeta {
                template,
                name: name.map(Arc::from),
            },
        );
    }

    /// An entity left the kernel between ticks; the next publish sends its removal.
    pub(crate) fn entity_removed(&mut self, id: EntityId) {
        self.meta.remove(&id);
        if self.table.info.remove(&id).is_some() {
            self.table.generation += 1;
        }
        self.removed.push(id);
    }

    /// Polls the adapters and mirrors their entities into `kernel`; returns the ghost events
    /// (ASM-248).
    pub(crate) fn ingest(&mut self, kernel: &mut Kernel) -> Vec<SimEvent> {
        let now = kernel.now();
        let mut events = Vec::new();
        let mut world = KernelGhosts {
            kernel,
            models: &self.ghosts,
            table: &mut self.table,
        };
        self.manager.ingest(now, &mut world, &mut events);
        self.note_faults();
        events
    }

    /// Publishes the owned entities and the tick's events after the kernel tick.
    pub(crate) fn publish(&mut self, kernel: &Kernel, events: &[SimEvent]) {
        kernel.owned_views(&mut self.owned);
        let now = kernel.now();
        if self.removed.is_empty() {
            self.manager.publish(now, &self.owned, &self.meta, events);
        } else {
            let mut all: Vec<SimEvent> = self
                .removed
                .drain(..)
                .map(|entity| SimEvent::EntityRemoved { entity })
                .collect();
            all.extend_from_slice(events);
            self.manager.publish(now, &self.owned, &self.meta, &all);
        }
        self.note_faults();
    }

    /// Sends a run-control action on the adapters that publish control (ASM-244).
    pub(crate) fn publish_control(&mut self, now: SimTime, action: ControlAction) {
        self.manager.publish_control(now, action);
        self.note_faults();
    }

    /// The fault that ends the run under the `abort` policy, once.
    pub(crate) fn take_abort(&mut self) -> Option<NetFault> {
        self.abort.take()
    }

    /// Compares the faulted adapters with those seen before (ASM-252).
    fn note_faults(&mut self) {
        let count = self.manager.faulted_count();
        if count <= self.faulted_seen {
            return;
        }
        self.faulted_seen = count;
        for s in self.manager.status().into_iter().filter(|s| s.faulted) {
            if self.reported.contains(&s.name) {
                continue;
            }
            tracing::warn!("network adapter {} faulted: {:?}", s.name, s.fault_reason);
            if self.policy == PluginFaultPolicy::Abort && self.abort.is_none() {
                self.abort = Some(NetFault {
                    adapter: s.name.clone(),
                    reason: s.fault_reason.clone().unwrap_or_default(),
                });
            }
            self.reported.push(s.name);
        }
    }
}

/// The end-of-run stderr line of one network adapter (ASM-250): `info: net <name>: published=..`
/// with the counters in a fixed order; refused entity types follow when there are any.
pub fn net_summary(s: &AdapterStatus) -> String {
    let mut line = format!(
        "info: net {}: published={} received={} ghosts={} echo_dropped={} unmapped={} \
         tx_dropped={} rx_dropped={} decode_errors={} invalid_state={} world_errors={}",
        s.name,
        s.published,
        s.received,
        s.ghosts,
        s.echo_dropped,
        s.unmapped,
        s.counters.tx_dropped,
        s.counters.rx_dropped,
        s.counters.decode_errors,
        s.invalid_state,
        s.world_errors,
    );
    if s.faulted {
        let _ = write!(
            line,
            " faulted={:?}",
            s.fault_reason.as_deref().unwrap_or_default()
        );
    }
    if !s.unmapped_types.is_empty() {
        let types: Vec<&str> = s.unmapped_types.iter().map(NetEntityType::as_str).collect();
        let _ = write!(line, " unmapped_types={}", types.join(","));
    }
    line
}

impl Engine {
    /// The network link, `None` when the scenario has no `[[network_adapters]]`.
    #[must_use]
    pub fn network(&self) -> Option<&NetLink> {
        self.network.as_ref()
    }

    /// The network link, mutably (enable or disable an adapter).
    pub fn network_mut(&mut self) -> Option<&mut NetLink> {
        self.network.as_mut()
    }

    /// Attaches the link; from then on every tick ingests before and publishes after the kernel
    /// tick (ASM-224).
    pub fn attach_network(&mut self, link: NetLink) {
        self.network = Some(link);
    }

    /// Publishes a run-control action on the adapters that have `publish.control` set; nothing
    /// without a link (ASM-244).
    ///
    /// Returns the fault when an adapter faulted in it under the `abort` policy (ASM-252); the
    /// caller stops the run.
    pub fn publish_control(&mut self, action: ControlAction) -> Option<NetFault> {
        let now = self.now();
        let link = self.network.as_mut()?;
        link.publish_control(now, action);
        link.take_abort()
    }
}
