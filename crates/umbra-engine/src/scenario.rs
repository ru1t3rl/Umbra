//! Loading and instantiating scenarios (REQ-S-2, REQ-T-1, ASM-059, ASM-062).
//!
//! Components are built through the registry by ID only; this module never names a concrete
//! motion model or terrain type.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use umbra_core::{ComponentKind, EntityId, Ownership, Registry, TemplateId, TickRate};
use umbra_kernel::{ClockMode, TimeScale, WallClock};
use umbra_templates::{ClockSetting, Loader, LoaderError, LoaderOptions, Placement};
pub use umbra_templates::{
    Diagnostic, DiagnosticCode, Diagnostics, Location, Note, Scenario, TickRateSource,
    ValidationReport,
};

use umbra_plugin_host::PluginHost;

use crate::confine::Confinement;
use crate::engine::{AgentInit, Engine, EngineConfig, RouteSpec, SpawnSpec};
use crate::error::EngineError;
use crate::net::NetLink;
use crate::spawner::Spawner;

/// Overrides and the wall clock for one run.
pub struct RunOptions {
    /// The wall clock pacing the engine.
    pub wall: Box<dyn WallClock>,
    /// Replaces the scenario's seed.
    pub seed: Option<u64>,
    /// Replaces the scenario's clock.
    pub clock: Option<ClockMode>,
}

/// Engine-level metadata of a spawned entity (not hashed, ASM-059).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityInfo {
    /// The entity.
    pub id: EntityId,
    /// Its template.
    pub template: TemplateId,
    /// Its name.
    pub name: Option<String>,
    /// Its group.
    pub group: Option<String>,
}

/// An engine with a scenario spawned in it.
pub struct ScenarioRun {
    /// The engine.
    pub engine: Engine,
    /// Spawned entities in id order.
    pub entities: Vec<EntityInfo>,
    /// The scenario's run length in ticks.
    pub duration_ticks: u64,
}

/// Why a scenario could not be loaded.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ScenarioError {
    /// The files have errors.
    #[error(transparent)]
    Invalid(#[from] Diagnostics),
    /// A registered component schema is broken.
    #[error(transparent)]
    Loader(#[from] LoaderError),
    /// A plugin could not be discovered, loaded or registered (M4, ADR 0022).
    #[error(transparent)]
    Plugin(#[from] umbra_plugin_host::PluginError),
}

/// Settings that apply to one scenario load or validation (ADR 0029). `Default` is the built-in
/// behavior of [`load_scenario`] and [`validate_path`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct LoadSettings {
    /// The tick rate of a scenario that omits `tick_rate_hz`; 60 Hz by default. An explicit value
    /// in the scenario always wins.
    pub default_tick_rate: TickRate,
    /// Plugin directories searched before those the scenario names, used by the plugin-aware
    /// loaders and `Session::load` (the configuration file's `plugin_paths`, ADR 0029). Never
    /// confined: they are trusted. `load_scenario_with_settings` and `validate_path_with_settings`
    /// load no plugins and ignore them.
    pub extra_plugin_paths: Vec<PathBuf>,
    /// Keeps the inputs of a load inside the scenario root (D-b). Applied only by `Session::load`;
    /// every other function taking `LoadSettings` ignores it. `None` outside the server (which sets
    /// it from WI-11).
    pub confinement: Option<Confinement>,
}

impl LoadSettings {
    fn loader_options(&self) -> LoaderOptions {
        let mut options = LoaderOptions::default();
        options.default_tick_rate = self.default_tick_rate;
        options
    }
}

/// Loads a scenario file and its templates.
///
/// # Errors
/// [`ScenarioError::Invalid`] with every diagnostic; [`ScenarioError::Loader`] if a registered
/// schema is broken.
pub fn load_scenario(registry: &Registry, path: &Path) -> Result<Scenario, ScenarioError> {
    load_scenario_with_settings(registry, path, &LoadSettings::default())
}

/// [`load_scenario`] with explicit [`LoadSettings`] (the tick rate default only; plugin paths and
/// confinement are not used here).
///
/// # Errors
/// As [`load_scenario`].
pub fn load_scenario_with_settings(
    registry: &Registry,
    path: &Path,
    settings: &LoadSettings,
) -> Result<Scenario, ScenarioError> {
    Ok(Loader::with_options(registry, settings.loader_options())?.load_scenario(path)?)
}

/// Validates a scenario file, a template file or a directory without running anything.
///
/// # Errors
/// [`ScenarioError::Loader`] if a registered schema is broken.
pub fn validate_path(registry: &Registry, path: &Path) -> Result<ValidationReport, ScenarioError> {
    validate_path_with_settings(registry, path, &LoadSettings::default())
}

/// [`validate_path`] with explicit [`LoadSettings`] (the tick rate default only; plugin paths and
/// confinement are not used here).
///
/// # Errors
/// As [`validate_path`].
pub fn validate_path_with_settings(
    registry: &Registry,
    path: &Path,
    settings: &LoadSettings,
) -> Result<ValidationReport, ScenarioError> {
    Ok(Loader::with_options(registry, settings.loader_options())?.validate_path(path))
}

fn clock_mode(setting: ClockSetting) -> Result<ClockMode, EngineError> {
    Ok(match setting {
        ClockSetting::AsFastAsPossible => ClockMode::AsFastAsPossible,
        ClockSetting::RealTime => ClockMode::RealTime,
        ClockSetting::Stepped => ClockMode::Stepped,
        ClockSetting::Scaled(factor) => ClockMode::Scaled(TimeScale::new(factor)?),
    })
}

/// A clone of `base` with the scenario's file graphs registered as behaviors (ADR 0011), so a
/// template can name them by ID. `base` is unchanged, and the result is a pure function of
/// `base` and the graphs (ordered by ID, REQ-K-4).
///
/// # Errors
/// [`EngineError::Graphs`] if a graph cannot be registered (not expected after validation).
pub fn scenario_registry(base: &Registry, scenario: &Scenario) -> Result<Registry, EngineError> {
    let mut registry = base.clone();
    umbra_behavior::register_graphs(&mut registry, scenario.graphs.iter())?;
    Ok(registry)
}

/// The behavior and sensors of a placement, built through the registry by ID (ADR 0011).
fn build_agent(
    registry: &Registry,
    index: usize,
    placement: &Placement,
) -> Result<AgentInit, EngineError> {
    let behavior = placement
        .behavior
        .as_ref()
        .map(|b| {
            registry
                .behavior(&b.id)
                .ok_or_else(|| EngineError::UnknownComponent {
                    kind: ComponentKind::Behavior,
                    id: b.id.clone(),
                })?
                .build(&b.params)
                .map_err(|source| EngineError::BehaviorBuild {
                    entity: index,
                    source,
                })
        })
        .transpose()?;
    let sensors = placement
        .sensors
        .iter()
        .enumerate()
        .map(|(i, s)| {
            registry
                .sensor(&s.id)
                .ok_or_else(|| EngineError::UnknownComponent {
                    kind: ComponentKind::Sensor,
                    id: s.id.clone(),
                })?
                .build(&s.params)
                .map_err(|source| EngineError::SensorBuild {
                    entity: index,
                    index: i,
                    source,
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(AgentInit {
        force: placement.force.clone(),
        behavior,
        sensors,
    })
}

/// Builds one placement through `registry` (which must hold the scenario's graphs, see
/// [`scenario_registry`]) and spawns it; `index` names the placement in build errors. Shared by
/// [`instantiate`] and `Session::spawn`, so an API spawn is built exactly like a placement
/// (ASM-153).
pub(crate) fn spawn_placement(
    engine: &mut Engine,
    registry: &Registry,
    index: usize,
    placement: &Placement,
) -> Result<EntityId, EngineError> {
    let motion = registry
        .motion_model(&placement.motion.id)
        .ok_or_else(|| EngineError::UnknownComponent {
            kind: ComponentKind::MotionModel,
            id: placement.motion.id.clone(),
        })?
        .build(&placement.motion.params)
        .map_err(|source| EngineError::Build {
            entity: index,
            source,
        })?;
    let agent = build_agent(registry, index, placement)?;
    let spec = SpawnSpec {
        position: placement.position,
        heading: placement.heading,
        speed: placement.speed,
        ownership: Ownership::Owned,
        motion,
        route: placement.route.as_ref().map(|r| RouteSpec {
            waypoints: r.waypoints.clone(),
            arrival_radius: r.arrival_radius,
            cruise_speed: r.cruise_speed,
        }),
    };
    let id = engine.spawn_with(spec, agent)?;
    if let Some(link) = engine.network.as_mut() {
        link.note_owned(
            id,
            Some(placement.template.clone()),
            placement.name.as_deref(),
        );
    }
    Ok(id)
}

/// Builds terrain, motion models, behaviors and sensors through the registry and spawns in file
/// order. The scenario's file graphs are registered first ([`scenario_registry`]). Every placement is
/// spawned with [`Engine::spawn_with`]; one without behavior, sensors and force gets
/// `AgentInit::default()`, which is bit-identical to [`Engine::spawn`] (ASM-109 a).
///
/// # Errors
/// [`EngineError`]; after a successful validation only the spawn checks can fail. A behavior that
/// cannot be instantiated at the tick rate arrives as
/// `EngineError::Kernel(KernelError::Instantiate(..))`.
pub fn instantiate(
    registry: &Registry,
    scenario: &Scenario,
    options: RunOptions,
) -> Result<ScenarioRun, EngineError> {
    instantiate_in(registry, scenario, options, None)
}

/// [`instantiate`], optionally with a plugin host: the host is attached to the new engine, and
/// building and spawning happen inside the host's build scope (RR-m2, ASM-138).
pub(crate) fn instantiate_in(
    registry: &Registry,
    scenario: &Scenario,
    options: RunOptions,
    host: Option<&Arc<PluginHost>>,
) -> Result<ScenarioRun, EngineError> {
    let registry = &scenario_registry(registry, scenario)?;
    let unknown = |kind, id: &umbra_core::ComponentId| EngineError::UnknownComponent {
        kind,
        id: id.clone(),
    };
    let terrain = registry
        .terrain(&scenario.terrain.id)
        .ok_or_else(|| unknown(ComponentKind::Terrain, &scenario.terrain.id))?
        .build(&scenario.terrain.params)
        .map_err(EngineError::TerrainBuild)?;
    let config = EngineConfig {
        seed: options.seed.unwrap_or(scenario.seed),
        tick_rate: scenario.tick_rate,
        clock: match options.clock {
            Some(mode) => mode,
            None => clock_mode(scenario.clock)?,
        },
    };
    let mut engine = Engine::with_wall_clock(config, terrain, options.wall);
    // Declared after the engine, so the scope closes before the engine moves out.
    let _scope = match host {
        Some(host) => {
            engine.attach_plugins(Arc::clone(host))?;
            Some(host.build_scope())
        }
        None => None,
    };
    // The link starts before the first spawn, so every placement records its metadata (M6).
    if let Some(link) = NetLink::connect(registry, scenario, engine.now())? {
        engine.attach_network(link);
    }
    engine.attach_spawner(Spawner::new(
        registry.clone(),
        scenario.templates.clone(),
        scenario.entities.len(),
    ));
    let mut entities = Vec::with_capacity(scenario.entities.len());
    for (index, placement) in scenario.entities.iter().enumerate() {
        let id = spawn_placement(&mut engine, registry, index, placement)?;
        entities.push(EntityInfo {
            id,
            template: placement.template.clone(),
            name: placement.name.clone(),
            group: placement.group.clone(),
        });
    }
    Ok(ScenarioRun {
        engine,
        entities,
        duration_ticks: scenario.duration_ticks,
    })
}
