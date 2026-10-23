//! Applies the `Spawn` and `Despawn` intents of a tick after `Kernel::tick` (REQ-B-2, REQ-C-2,
//! REQ-K-5, D-46, ADR 0052, ASM-600..606, ASM-644, ASM-645, ASM-681..693).
//!
//! The engine keeps the spawner in an `Option` and takes it out for the duration of the call, so
//! `Spawner::apply` can borrow the whole `Engine` mutably (it builds entities with the same
//! `spawn_placement` as placements and API spawns, and removes through `Engine::remove`).

use std::collections::BTreeSet;

use umbra_core::{EntityId, Force, Intent, Position, Radians, Registry, SimEvent, TemplateId};
use umbra_templates::TemplateSet;

use crate::engine::Engine;
use crate::error::EngineError;
use crate::scenario::{placement_from_template, spawn_placement};
use crate::session::TemplateSpawn;

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

/// The caps a [`Spawner`] enforces (ASM-603). Only tests lower them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Limits {
    /// Most `Spawn` intents handled per tick (applied, rejected or capped alike).
    pub(crate) per_tick: usize,
    /// Most Owned entities the engine holds; a spawn that would exceed it is dropped.
    pub(crate) total: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            per_tick: MAX_SPAWNS_PER_TICK,
            total: MAX_ENTITIES,
        }
    }
}

/// Applies lifecycle intents against the scenario's templates and graphs.
pub(crate) struct Spawner {
    /// The registry plus the scenario's file graphs (what placements build from).
    registry: Registry,
    templates: TemplateSet,
    counters: LifecycleCounters,
    limits: Limits,
    /// Templates already warned about (one warning per template id, ASM-604).
    warned: BTreeSet<TemplateId>,
}

impl Spawner {
    /// A spawner for a scenario, with the documented caps.
    pub(crate) fn new(registry: Registry, templates: TemplateSet) -> Self {
        Self {
            registry,
            templates,
            counters: LifecycleCounters::default(),
            limits: Limits::default(),
            warned: BTreeSet::new(),
        }
    }

    /// The same spawner with other caps (unit tests of the caps).
    #[cfg(test)]
    pub(crate) fn with_limits(mut self, limits: Limits) -> Self {
        self.limits = limits;
        self
    }

    /// The counters so far.
    pub(crate) fn counters(&self) -> LifecycleCounters {
        self.counters
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
            // A duplicate `Despawn` finds the entity gone and is ignored (ASM-685).
            if matches!(intent, Intent::Despawn) && engine.remove(*entity).is_ok() {
                self.counters.despawned += 1;
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
        let mut seen = 0_usize;
        let mut capped = 0_u64;
        let mut owned = engine.owned_count();
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
            if seen > self.limits.per_tick || owned >= self.limits.total {
                capped += 1;
                continue;
            }
            let force = engine.kernel.force_of(*spawner);
            if let Some(created) =
                self.spawn_one(engine, *spawner, template, *position, *heading, force)
            {
                owned += 1;
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
        spawner: EntityId,
        template: &TemplateId,
        position: umbra_core::Ecef,
        heading: Radians,
        force: Option<Force>,
    ) -> Option<SimEvent> {
        let Some(resolved) = self.templates.get(template) else {
            self.reject(spawner, template, "unknown template");
            return None;
        };
        let mut spec = TemplateSpawn::new(template.clone(), Position::Ecef(position));
        spec.heading = heading;
        spec.force = force;
        let placement = placement_from_template(resolved, spec);
        // The index only names the placement in a build error, which `reject` reports with the
        // spawner instead; runtime spawns have no placement index.
        match spawn_placement(engine, &self.registry, 0, &placement) {
            Ok(id) => {
                self.counters.spawned += 1;
                Some(SimEvent::EntityCreated {
                    entity: id,
                    template: Some(template.clone()),
                })
            }
            Err(error) => {
                self.reject(spawner, template, &cause(&error));
                None
            }
        }
    }

    fn reject(&mut self, spawner: EntityId, template: &TemplateId, why: &str) {
        self.counters.rejected += 1;
        if self.warned.insert(template.clone()) {
            tracing::warn!(
                spawner = spawner.get(),
                template = template.as_str(),
                "spawn dropped: {why}"
            );
        }
    }
}

/// The error of a failed spawn without the placement index the build variants print (runtime
/// spawns have none): the factory's own message.
fn cause(error: &EngineError) -> String {
    match error {
        EngineError::Build { source, .. }
        | EngineError::BehaviorBuild { source, .. }
        | EngineError::SensorBuild { source, .. } => source.to_string(),
        other => other.to_string(),
    }
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

    /// The Owned entities of this engine: every entity but the ghosts of the network link. The
    /// total cap counts these (ASM-689).
    pub(crate) fn owned_count(&self) -> usize {
        let ghosts = self
            .network
            .as_ref()
            .map_or(0, crate::net::NetLink::ghost_count);
        self.kernel.entity_count().saturating_sub(ghosts)
    }

    /// Attaches the spawner that applies `Spawn` and `Despawn` intents after every tick.
    pub(crate) fn attach_spawner(&mut self, spawner: Spawner) {
        self.spawner = Some(spawner);
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use umbra_core::Ecef;
    use umbra_kernel::ManualWallClock;

    use super::*;
    use crate::builtins::standard_registry;
    use crate::scenario::{RunOptions, instantiate, load_scenario};

    /// The walking skeleton: three Owned entities (ids 1..=3), templates `tank.mbt_generic` etc.
    fn skeleton() -> (Engine, Spawner) {
        let registry = standard_registry().expect("registry");
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/scenarios/walking_skeleton/scenario.toml");
        let scenario = load_scenario(&registry, &path).expect("scenario");
        let mut run = instantiate(
            &registry,
            &scenario,
            RunOptions {
                wall: Box::new(ManualWallClock::default()),
                seed: None,
                clock: None,
            },
        )
        .expect("instantiate");
        let spawner = run.engine.spawner.take().expect("the scenario's spawner");
        (run.engine, spawner)
    }

    fn id(n: u64) -> EntityId {
        EntityId::new(n).expect("non-zero")
    }

    fn spawn(template: &str) -> (EntityId, Intent) {
        (
            id(1),
            Intent::Spawn {
                template: TemplateId::new(template).expect("template id"),
                position: Ecef(umbra_core::Vec3::new(6_378_137.0, 0.0, 0.0)),
                heading: Radians(0.0),
            },
        )
    }

    /// ASM-603, ASM-689: the documented caps, and `Spawner::new` uses them.
    #[test]
    fn the_default_caps_are_the_documented_ones() {
        assert_eq!(MAX_SPAWNS_PER_TICK, 256);
        assert_eq!(MAX_ENTITIES, 100_000);
        assert_eq!(
            Limits::default(),
            Limits {
                per_tick: 256,
                total: 100_000
            }
        );
        let (_, spawner) = skeleton();
        assert_eq!(spawner.limits, Limits::default());
    }

    /// ASM-603: with three Owned entities and a total cap of five, four spawns in one slice apply
    /// two and drop two, in order, counted.
    #[test]
    fn spawns_beyond_the_total_cap_are_dropped_in_order_and_counted() {
        let (mut engine, spawner) = skeleton();
        let mut spawner = spawner.with_limits(Limits {
            per_tick: 256,
            total: 5,
        });
        let lifecycle = vec![spawn("tank.mbt_generic"); 4];
        let mut events = Vec::new();

        spawner.apply(&mut engine, &lifecycle, &mut events);

        let counters = spawner.counters();
        assert_eq!((counters.spawned, counters.capped), (2, 2));
        assert_eq!(engine.kernel.entity_count(), 5);
        let created: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                SimEvent::EntityCreated { entity, .. } => Some(entity.get()),
                _ => None,
            })
            .collect();
        assert_eq!(created, [4, 5]);
    }

    /// ASM-690: a build error is logged without the placement index (`entity 0:`) that runtime
    /// spawns do not have.
    #[test]
    fn a_build_error_is_reported_without_a_placement_index() {
        let source = umbra_core::BuildError::new("bad param");
        for error in [
            EngineError::Build {
                entity: 0,
                source: source.clone(),
            },
            EngineError::BehaviorBuild {
                entity: 0,
                source: source.clone(),
            },
            EngineError::SensorBuild {
                entity: 0,
                index: 1,
                source,
            },
        ] {
            let shown = cause(&error);
            assert!(
                shown.contains("bad param") && !shown.contains("entity"),
                "{shown}"
            );
        }
    }

    /// ASM-685: the same entity despawned twice in one slice is removed once: one event, one count.
    #[test]
    fn a_duplicate_despawn_removes_once_and_is_not_an_error() {
        let (mut engine, mut spawner) = skeleton();
        let lifecycle = vec![(id(2), Intent::Despawn), (id(2), Intent::Despawn)];
        let mut events = Vec::new();

        spawner.apply(&mut engine, &lifecycle, &mut events);

        assert_eq!(events, [SimEvent::EntityRemoved { entity: id(2) }]);
        assert_eq!(spawner.counters().despawned, 1);
        assert_eq!(engine.kernel.entity_count(), 2);
        assert!(!engine.directory().contains_key(&id(2)));
    }

    /// ASM-688: the engine's directory follows lifecycle spawns and despawns without any caller.
    #[test]
    fn the_engine_directory_follows_lifecycle_spawns_and_despawns() {
        let (mut engine, mut spawner) = skeleton();
        let before = engine.directory_version();
        let mut events = Vec::new();

        spawner.apply(&mut engine, &[spawn("truck.cargo")], &mut events);
        let child = engine.directory().get(&id(4)).cloned().expect("the child");

        assert_eq!(child.template.as_str(), "truck.cargo");
        assert_eq!((child.name, child.group), (None, None));
        assert!(engine.directory_version() > before);
        spawner.apply(&mut engine, &[(id(4), Intent::Despawn)], &mut events);
        assert!(!engine.directory().contains_key(&id(4)));
    }

    /// ASM-689: a ghost of a network peer is in the kernel but not in `owned_count`, so it does not
    /// use up the cap that protects local spawning.
    #[test]
    fn a_ghost_does_not_count_towards_the_owned_total() {
        use umbra_core::dead_reckoning::DeadReckoning;
        use umbra_core::net::{AdapterStart, KeySpace, NetEntityState, NetKey, NetMessage};
        use umbra_core::{ComponentId, Force, SimTime, TickRate, Vec3, WorldEuler};

        let mut registry = crate::builtins::builtin_registry().expect("registry");
        let hub = umbra_net::LoopbackHub::new(64);
        let adapter = ComponentId::new("test.net.left").expect("id");
        let other_hub = umbra_net::LoopbackHub::new(64);
        for (id, hub, base) in [
            (adapter.clone(), &hub, 1000),
            (
                ComponentId::new("test.net.right").expect("id"),
                &other_hub,
                2000,
            ),
        ] {
            registry
                .register_network_adapter(hub.factory(id, KeySpace { base, count: 10 }))
                .expect("register");
        }
        let input = serde_json::json!({"name": "peer", "params": {}, "templates": {}});
        let rate = TickRate::new(30).expect("rate");
        let mut peer = hub
            .factory(
                adapter,
                KeySpace {
                    base: 1500,
                    count: 10,
                },
            )
            .build(&input)
            .expect("peer config")
            .start(&AdapterStart {
                now: SimTime::at_tick(rate, 0),
            })
            .expect("peer start");
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/net_pair/c/scenario.toml");
        let scenario = load_scenario(&registry, &path).expect("scenario");
        let mut run = instantiate(
            &registry,
            &scenario,
            RunOptions {
                wall: Box::new(ManualWallClock::with_auto_advance(
                    core::time::Duration::from_micros(500),
                )),
                seed: None,
                clock: None,
            },
        )
        .expect("instantiate");
        let owned = run.engine.owned_count();
        let ghost = NetMessage::EntityState(NetEntityState {
            key: NetKey(5000),
            entity_type: Some(umbra_core::net::NetEntityType::new("tank")),
            force: Force::new("red").ok(),
            name: None,
            position: Ecef(Vec3::new(6_378_137.0, 0.0, 0.0)),
            velocity: Vec3::ZERO,
            acceleration: Vec3::ZERO,
            angular_velocity: Vec3::ZERO,
            orientation: WorldEuler {
                yaw: Radians(0.0),
                pitch: Radians(0.0),
                roll: Radians(0.0),
            },
            dead_reckoning: DeadReckoning::Static,
        });
        peer.publish(SimTime::at_tick(rate, 0), &[ghost])
            .expect("send");

        run.engine
            .step(1, |_| core::ops::ControlFlow::Continue(()))
            .expect("step");

        assert_eq!(
            run.engine.kernel.entity_count(),
            owned + 1,
            "the ghost exists"
        );
        assert_eq!(run.engine.owned_count(), owned);
        // With the ghost present, a total cap one above the owned count still admits one spawn.
        let template = run.entities[0].template.clone();
        let mut spawner = run
            .engine
            .spawner
            .take()
            .expect("spawner")
            .with_limits(Limits {
                per_tick: 256,
                total: owned + 1,
            });
        let lifecycle = [spawn(template.as_str()), spawn(template.as_str())];

        spawner.apply(&mut run.engine, &lifecycle, &mut Vec::new());

        let counters = spawner.counters();
        assert_eq!((counters.spawned, counters.capped), (1, 1));
    }
}
