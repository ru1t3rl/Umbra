//! One scenario load, usable without the engine handle (REQ-S-2, REQ-X-1, ADR 0024, review D-a).
//!
//! A [`Session`] owns the engine of a loaded scenario, the registry and plugin host it was built
//! from and a directory of its entities. It starts paused at tick 0. The engine thread of
//! `EngineHandle` (from WI-8) drives one; an embedder can drive one directly with [`Engine::pump`].

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use umbra_core::{
    EntityId, Force, MetersPerSecond, Position, Radians, Registry, SimEvent, TemplateId,
};
use umbra_kernel::{ClockMode, WallClock};
use umbra_plugin_host::{HostOptions, PluginHost, PluginLog};
use umbra_templates::{
    Placement, Scenario, SourceFile, graph_document_id, io_diagnostic, scenario_inputs,
};

use crate::confine::ConfineError;
use crate::engine::Engine;
use crate::error::EngineError;
use crate::plugins::{
    LoadedScenario, dedup_plugin_paths, load_with_host_options, resolve_plugin_paths,
};
use crate::scenario::{
    EntityInfo, LoadSettings, RunOptions, ScenarioError, instantiate_in, scenario_registry,
    spawn_placement,
};
use crate::spawner::DirectoryChange;
use crate::validate::{GraphValidation, validate_with_others};

/// How to load a session.
#[non_exhaustive]
pub struct SessionOptions {
    /// Replaces the scenario's seed.
    pub seed: Option<u64>,
    /// Replaces the scenario's clock.
    pub clock: Option<ClockMode>,
    /// The wall clock pacing the engine.
    pub wall: Box<dyn WallClock>,
    /// Tick rate default, extra plugin directories and confinement.
    pub load: LoadSettings,
    /// Receives plugin log messages; `None` logs to `tracing`.
    pub plugin_log: Option<Arc<dyn PluginLog>>,
}

impl SessionOptions {
    /// Options with no overrides and default [`LoadSettings`].
    #[must_use]
    pub fn new(wall: Box<dyn WallClock>) -> Self {
        Self {
            seed: None,
            clock: None,
            wall,
            load: LoadSettings::default(),
            plugin_log: None,
        }
    }
}

/// A spawn from a template of the session's scenario (ASM-153): built exactly like a scenario
/// placement without overrides or a route.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct TemplateSpawn {
    /// The template.
    pub template: TemplateId,
    /// Where the entity starts.
    pub position: Position,
    /// Initial heading.
    pub heading: Radians,
    /// Initial speed.
    pub speed: MetersPerSecond,
    /// Optional unique name.
    pub name: Option<String>,
    /// Optional group.
    pub group: Option<String>,
    /// Optional force.
    pub force: Option<Force>,
}

impl TemplateSpawn {
    /// A spawn at `position`, heading north, at rest, without name, group or force.
    #[must_use]
    pub fn new(template: TemplateId, position: Position) -> Self {
        Self {
            template,
            position,
            heading: Radians(0.0),
            speed: MetersPerSecond(0.0),
            name: None,
            group: None,
            force: None,
        }
    }
}

/// Why a session operation failed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SessionError {
    /// The scenario or its plugins could not be loaded.
    #[error(transparent)]
    Scenario(#[from] ScenarioError),
    /// The engine refused a spawn or removal, or could not be built.
    #[error(transparent)]
    Engine(#[from] EngineError),
    /// An input of the load is outside the allowed directories.
    #[error(transparent)]
    Confine(#[from] ConfineError),
    /// The scenario has no such template.
    #[error("unknown template {0}")]
    TemplateNotFound(TemplateId),
    /// An entity already has this name (ASM-059).
    #[error("an entity named {0:?} exists")]
    DuplicateName(String),
    /// There is no such entity.
    #[error("no entity {0}")]
    EntityNotFound(EntityId),
    /// The entity is a ghost of a network peer; only its owner can remove it (M6).
    #[error("entity {0} belongs to a network peer and cannot be removed here")]
    EntityRemote(EntityId),
}

/// One loaded scenario.
///
/// Field order is drop order (review D-a): the engine goes first (entity instances, then their
/// behaviors, so plugin `destroy_instance` and `destroy_behavior` run), then the registries and the
/// scenario, and the host last, so the plugin's `shutdown` and library close come after everything
/// that was built from it. A reload with the same plugin therefore starts a new image lifecycle.
pub struct Session {
    engine: Engine,
    /// The registry plus the scenario's file graphs, which API spawns build from.
    build_registry: Registry,
    registry: Registry,
    scenario: Scenario,
    directory: BTreeMap<EntityId, EntityInfo>,
    duration_ticks: u64,
    /// API spawns so far; with the scenario's placements it numbers build errors.
    spawns: usize,
    plugins: Arc<PluginHost>,
    /// The scenario file as loaded: resolved against the scenario root when confined.
    path: PathBuf,
}

impl Session {
    /// Loads the scenario at `path` against `base` plus its plugins, builds the engine and leaves
    /// it paused at tick 0.
    ///
    /// Without a confinement the plugin directories are `options.load.extra_plugin_paths`, then
    /// those the scenario file (or, for a directory, every scenario under it) names. With one, `path`
    /// must be a file, every input of the scenario is checked first (D-b) and nothing is read or
    /// loaded if one is outside the allowed directories; the plugin directories are then the
    /// extra ones plus exactly the `plugin_paths` of the checked file.
    ///
    /// # Errors
    /// [`SessionError::Confine`], [`SessionError::Scenario`] (including plugin errors) and
    /// [`SessionError::Engine`].
    pub fn load(
        base: &Registry,
        path: &Path,
        options: SessionOptions,
    ) -> Result<Self, SessionError> {
        let SessionOptions {
            seed,
            clock,
            wall,
            load,
            plugin_log,
        } = options;
        // With a confinement the request is resolved first: a relative path is taken against the
        // scenario root, one outside it is refused without touching the disk, a directory is not a
        // file (ADR 0026, D-b). Everything after this uses the resolved path.
        let resolved;
        let (path, paths) = match &load.confinement {
            Some(confinement) => {
                resolved = confinement.resolve_request(path)?;
                let path = resolved.as_path();
                let inputs = scenario_inputs(path);
                confinement.check_inputs(&inputs)?;
                (
                    path,
                    dedup_plugin_paths(
                        load.extra_plugin_paths
                            .iter()
                            .cloned()
                            .chain(inputs.plugin_dirs),
                    ),
                )
            }
            None => (
                path,
                resolve_plugin_paths(&load.extra_plugin_paths, Some(path)),
            ),
        };
        let mut host_options = HostOptions::default();
        if let Some(log) = plugin_log {
            host_options.log = log;
        }
        let LoadedScenario {
            registry,
            scenario,
            plugins,
            ..
        } = load_with_host_options(base, path, &paths, &load, host_options)?;
        let options = RunOptions { wall, seed, clock };
        let mut run = instantiate_in(&registry, &scenario, options, Some(&plugins))?;
        run.engine.pause();
        let build_registry = scenario_registry(&registry, &scenario)?;
        let directory = run.entities.into_iter().map(|e| (e.id, e)).collect();
        Ok(Self {
            engine: run.engine,
            build_registry,
            registry,
            scenario,
            directory,
            duration_ticks: run.duration_ticks,
            spawns: 0,
            plugins,
            path: path.to_path_buf(),
        })
    }

    /// Brings the directory up to date with the entities the engine's spawner created and removed
    /// from `Spawn` and `Despawn` intents (ADR 0052, ASM-606). The driver calls it after every tick.
    pub fn sync_lifecycle(&mut self) {
        for change in self.engine.take_directory_changes() {
            match change {
                DirectoryChange::Created(info) => {
                    self.directory.insert(info.id, info);
                }
                DirectoryChange::Removed(id) => {
                    self.directory.remove(&id);
                }
            }
        }
    }

    /// The scenario file as loaded: with a confinement, the request resolved against the scenario
    /// root; without one, as given.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The engine.
    #[must_use]
    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    /// The engine, to drive it.
    pub fn engine_mut(&mut self) -> &mut Engine {
        &mut self.engine
    }

    /// The loaded scenario.
    #[must_use]
    pub fn scenario(&self) -> &Scenario {
        &self.scenario
    }

    /// The base registry plus the plugins' capabilities (without the scenario's file graphs).
    #[must_use]
    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    /// The registry plus the scenario's file graphs: what API spawns build from, and what the
    /// component and catalog queries describe.
    #[must_use]
    pub fn graph_registry(&self) -> &Registry {
        &self.build_registry
    }

    /// The plugin host of this session.
    #[must_use]
    pub fn plugins(&self) -> &PluginHost {
        &self.plugins
    }

    /// The entities this engine owns by id: those of the scenario, plus spawns, minus removals.
    /// Ghosts of network peers are not in it; see [`Session::directory`].
    #[must_use]
    pub fn entities(&self) -> &BTreeMap<EntityId, EntityInfo> {
        &self.directory
    }

    /// Every entity by id: [`Session::entities`] plus the ghosts of the network link, with their
    /// template and the peer's name (`group` is `None`). An owned map, so it reflects the ghosts
    /// of the moment (ASM-400).
    #[must_use]
    pub fn directory(&self) -> BTreeMap<EntityId, EntityInfo> {
        let mut all = self.directory.clone();
        if let Some(link) = self.engine.network() {
            all.extend(link.ghosts().map(|(id, ghost)| {
                (
                    id,
                    EntityInfo {
                        id,
                        template: ghost.template.clone(),
                        name: ghost.name.as_deref().map(str::to_owned),
                        group: None,
                    },
                )
            }));
        }
        all
    }

    /// The directory entry of `id`, owned entities and ghosts alike.
    #[must_use]
    pub fn entity_info(&self, id: EntityId) -> Option<EntityInfo> {
        self.directory.get(&id).cloned().or_else(|| {
            let ghost = self.engine.network()?.ghosts().find(|(g, _)| *g == id)?.1;
            Some(EntityInfo {
                id,
                template: ghost.template.clone(),
                name: ghost.name.as_deref().map(str::to_owned),
                group: None,
            })
        })
    }

    /// The scenario's run length in ticks.
    #[must_use]
    pub fn duration_ticks(&self) -> u64 {
        self.duration_ticks
    }

    /// Spawns an entity from a template of the scenario, between ticks. Building happens inside the
    /// plugin build scope, so plugin behaviors build like they did at load (review M6). Returns the
    /// new id and the `EntityCreated` event for the caller to publish.
    ///
    /// # Errors
    /// [`SessionError::TemplateNotFound`], [`SessionError::DuplicateName`] and
    /// [`SessionError::Engine`]; nothing is created then.
    pub fn spawn(&mut self, spec: TemplateSpawn) -> Result<(EntityId, SimEvent), SessionError> {
        let template = self
            .scenario
            .templates
            .get(&spec.template)
            .ok_or_else(|| SessionError::TemplateNotFound(spec.template.clone()))?;
        if let Some(name) = &spec.name
            && self
                .directory
                .values()
                .any(|e| e.name.as_ref() == Some(name))
        {
            return Err(SessionError::DuplicateName(name.clone()));
        }
        let placement = Placement {
            template: spec.template,
            name: spec.name,
            group: spec.group,
            position: spec.position,
            heading: spec.heading,
            speed: spec.speed,
            motion: template.motion.clone(),
            route: None,
            behavior: template.behavior.clone(),
            sensors: template.sensors.clone(),
            force: spec.force,
        };
        let _scope = self.plugins.build_scope();
        let index = self.scenario.entities.len() + self.spawns;
        self.spawns += 1;
        let id = spawn_placement(&mut self.engine, &self.build_registry, index, &placement)?;
        let event = SimEvent::EntityCreated {
            entity: id,
            template: Some(placement.template.clone()),
        };
        self.directory.insert(
            id,
            EntityInfo {
                id,
                template: placement.template,
                name: placement.name,
                group: placement.group,
            },
        );
        Ok((id, event))
    }

    /// Removes an entity, between ticks. Returns the `EntityRemoved` event for the caller to
    /// publish.
    ///
    /// # Errors
    /// [`SessionError::EntityNotFound`]; [`SessionError::EntityRemote`] for a ghost of a network
    /// peer. Nothing changes then.
    pub fn remove(&mut self, id: EntityId) -> Result<SimEvent, SessionError> {
        if self.engine.network().is_some_and(|n| n.is_ghost(id)) {
            return Err(SessionError::EntityRemote(id));
        }
        if !self.directory.contains_key(&id) {
            return Err(SessionError::EntityNotFound(id));
        }
        self.engine.remove(id)?;
        self.directory.remove(&id);
        Ok(SimEvent::EntityRemoved { entity: id })
    }

    /// REQ-X-11, ASM-154: validates `document` (named `name` in diagnostics) together with the
    /// scenario's other file graphs, which are read again from disk; the one with the document's ID
    /// is left out, since the document stands for it. Runs inside the plugin build scope, so plugin
    /// behaviors dry-run (review M6). Never changes the session.
    ///
    /// # Errors
    /// [`ScenarioError::Loader`] if a registered component schema is broken.
    pub fn validate_graph(
        &self,
        name: &str,
        document: &str,
    ) -> Result<GraphValidation, ScenarioError> {
        let replaced = graph_document_id(&SourceFile::new(name, document));
        let (mut files, mut unread) = (Vec::new(), Vec::new());
        for graph in self.scenario.graphs.iter() {
            let Some(origin) = &graph.origin else {
                continue;
            };
            if replaced.as_deref() == Some(graph.id.as_str()) {
                continue;
            }
            match std::fs::read_to_string(origin) {
                Ok(text) => files.push(SourceFile::new(origin, text)),
                Err(error) => unread.push(io_diagnostic(origin, &error)),
            }
        }
        let _scope = self.plugins.build_scope();
        validate_with_others(&self.registry, name, document, files, unread)
    }
}
