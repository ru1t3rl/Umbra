//! Applies the `Spawn` and `Despawn` intents of a tick after `Kernel::tick` (REQ-B-2, REQ-C-2,
//! REQ-K-5, D-46, ADR 0052, ASM-600..606, ASM-644, ASM-645, ASM-681..).
//!
//! The engine keeps the spawner in an `Option` and takes it out for the duration of the call, so
//! `Spawner::apply` can borrow the whole `Engine` mutably (it builds entities with the same
//! `spawn_placement` as placements and API spawns, and removes through `Engine::remove`).

use std::collections::BTreeSet;

use umbra_core::net::OwnedView;
use umbra_core::{
    EntityId, Force, Intent, MetersPerSecond, Position, Radians, Registry, SimEvent, TemplateId,
};
use umbra_templates::{Placement, TemplateSet};

use crate::engine::Engine;
use crate::scenario::{EntityInfo, spawn_placement};

/// Most spawns applied per tick per engine; the excess is dropped in order (ASM-603).
pub const MAX_SPAWNS_PER_TICK: usize = 256;
/// Most entities an engine holds; spawns beyond it are dropped (ASM-603).
pub const MAX_ENTITIES: usize = 100_000;

/// Counters of the lifecycle intents an engine has handled (ASM-603, ASM-604).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct LifecycleCounters {
    /// Spawns applied.
    pub spawned: u64,
    /// Despawns applied.
    pub despawned: u64,
    /// Spawns rejected (unknown template or a failed build).
    pub rejected: u64,
    /// Spawns dropped by the per-tick or total caps.
    pub capped: u64,
}

/// A change to the entity directory that the spawner made (ASM-681).
pub(crate) enum DirectoryChange {
    /// An entity was created from a template.
    Created(EntityInfo),
    /// An entity was removed.
    Removed(EntityId),
}

/// Applies lifecycle intents against the scenario's templates and graphs.
pub(crate) struct Spawner {
    /// The registry plus the scenario's file graphs (what placements build from).
    registry: Registry,
    templates: TemplateSet,
    counters: LifecycleCounters,
    /// Build errors are numbered after the scenario's placements.
    next_index: usize,
    /// Templates already warned about (one warning per template id, ASM-604).
    warned: BTreeSet<TemplateId>,
    changes: Vec<DirectoryChange>,
}

impl Spawner {
    /// A spawner for a scenario with `placements` entities.
    pub(crate) fn new(registry: Registry, templates: TemplateSet, placements: usize) -> Self {
        Self {
            registry,
            templates,
            counters: LifecycleCounters::default(),
            next_index: placements,
            warned: BTreeSet::new(),
            changes: Vec::new(),
        }
    }

    /// The counters so far.
    pub(crate) fn counters(&self) -> LifecycleCounters {
        self.counters
    }

    /// Takes the directory changes made since the last call.
    pub(crate) fn take_changes(&mut self) -> Vec<DirectoryChange> {
        std::mem::take(&mut self.changes)
    }

    /// Applies all spawns of `lifecycle` in order, then all despawns, and appends the resulting
    /// `EntityCreated` and `EntityRemoved` events to `events` (ADR 0052).
    pub(crate) fn apply(
        &mut self,
        engine: &mut Engine,
        lifecycle: &[(EntityId, Intent)],
        events: &mut Vec<SimEvent>,
    ) {
        if lifecycle.is_empty() {
            return;
        }
        // Entering the build scope makes plugin behaviors build as they did at load (ASM-605).
        let host = engine.plugins.clone();
        let _scope = host.as_ref().map(|h| h.build_scope());
        self.apply_spawns(engine, lifecycle, events);
        for (entity, intent) in lifecycle {
            if matches!(intent, Intent::Despawn) && engine.remove(*entity).is_ok() {
                self.counters.despawned += 1;
                self.changes.push(DirectoryChange::Removed(*entity));
                events.push(SimEvent::EntityRemoved { entity: *entity });
            }
        }
    }

    fn apply_spawns(
        &mut self,
        engine: &mut Engine,
        lifecycle: &[(EntityId, Intent)],
        events: &mut Vec<SimEvent>,
    ) {
        let mut owners = Vec::new();
        let mut seen = 0_usize;
        let mut capped = 0_u64;
        for (spawner, intent) in lifecycle {
            let Intent::Spawn {
                template,
                position,
                heading,
            } = intent
            else {
                continue;
            };
            seen += 1;
            if seen > MAX_SPAWNS_PER_TICK || engine.kernel.entity_count() >= MAX_ENTITIES {
                capped += 1;
                continue;
            }
            if owners.is_empty() {
                engine.kernel.owned_views(&mut owners);
            }
            let force = force_of(&owners, *spawner);
            if let Some(created) = self.spawn_one(engine, template, *position, *heading, force) {
                events.push(created);
            }
        }
        if capped > 0 {
            self.counters.capped += capped;
            tracing::warn!(capped, "spawn intents dropped by the lifecycle caps");
        }
    }

    /// Builds one entity; a failure is counted, warned about once per template, and not a fault.
    fn spawn_one(
        &mut self,
        engine: &mut Engine,
        template: &TemplateId,
        position: umbra_core::Ecef,
        heading: Radians,
        force: Option<Force>,
    ) -> Option<SimEvent> {
        let Some(resolved) = self.templates.get(template) else {
            self.reject(template, "unknown template");
            return None;
        };
        let placement = Placement {
            template: template.clone(),
            name: None,
            group: None,
            position: Position::Ecef(position),
            heading,
            speed: MetersPerSecond(0.0),
            motion: resolved.motion.clone(),
            route: None,
            behavior: resolved.behavior.clone(),
            sensors: resolved.sensors.clone(),
            force,
        };
        let index = self.next_index;
        self.next_index += 1;
        match spawn_placement(engine, &self.registry, index, &placement) {
            Ok(id) => {
                self.counters.spawned += 1;
                self.changes.push(DirectoryChange::Created(EntityInfo {
                    id,
                    template: template.clone(),
                    name: None,
                    group: None,
                }));
                Some(SimEvent::EntityCreated {
                    entity: id,
                    template: Some(template.clone()),
                })
            }
            Err(error) => {
                self.reject(template, &error.to_string());
                None
            }
        }
    }

    fn reject(&mut self, template: &TemplateId, why: &str) {
        self.counters.rejected += 1;
        if self.warned.insert(template.clone()) {
            tracing::warn!(template = template.as_str(), "spawn dropped: {why}");
        }
    }
}

/// The force of Owned entity `id` in `owners` (sorted by id).
fn force_of(owners: &[OwnedView], id: EntityId) -> Option<Force> {
    owners
        .binary_search_by_key(&id, |o| o.id)
        .ok()
        .and_then(|i| owners[i].force.clone())
}

impl Engine {
    /// The lifecycle counters of this engine since it was built (ASM-603, ASM-604).
    #[must_use]
    pub fn lifecycle_counters(&self) -> LifecycleCounters {
        self.spawner
            .as_ref()
            .map(Spawner::counters)
            .unwrap_or_default()
    }

    /// Attaches the spawner that applies `Spawn` and `Despawn` intents after every tick.
    pub(crate) fn attach_spawner(&mut self, spawner: Spawner) {
        self.spawner = Some(spawner);
    }

    /// Takes the directory changes the spawner made since the last call (ASM-606).
    pub(crate) fn take_directory_changes(&mut self) -> Vec<DirectoryChange> {
        self.spawner
            .as_mut()
            .map(Spawner::take_changes)
            .unwrap_or_default()
    }
}
