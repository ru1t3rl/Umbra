//! Loading and instantiating scenarios (REQ-S-2, REQ-T-1, ASM-059, ASM-062).
//!
//! Components are built through the registry by ID only; this module never names a concrete
//! motion model or terrain type.

use std::path::Path;

use cgf_core::{ComponentKind, EntityId, Ownership, Registry, TemplateId};
use cgf_kernel::{ClockMode, TimeScale, WallClock};
use cgf_templates::{ClockSetting, Loader, LoaderError, Placement};
pub use cgf_templates::{
    Diagnostic, DiagnosticCode, Diagnostics, Location, Note, Scenario, ValidationReport,
};

use crate::engine::{AgentInit, Engine, EngineConfig, RouteSpec, SpawnSpec};
use crate::error::EngineError;

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
}

/// Loads a scenario file and its templates.
///
/// # Errors
/// [`ScenarioError::Invalid`] with every diagnostic; [`ScenarioError::Loader`] if a registered
/// schema is broken.
pub fn load_scenario(registry: &Registry, path: &Path) -> Result<Scenario, ScenarioError> {
    Ok(Loader::new(registry)?.load_scenario(path)?)
}

/// Validates a scenario file, a template file or a directory without running anything.
///
/// # Errors
/// [`ScenarioError::Loader`] if a registered schema is broken.
pub fn validate_path(registry: &Registry, path: &Path) -> Result<ValidationReport, ScenarioError> {
    Ok(Loader::new(registry)?.validate_path(path))
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
    cgf_behavior::register_graphs(&mut registry, scenario.graphs.iter())?;
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

/// Builds terrain, motion models, behaviors and sensors through the registry and spawns in file
/// order. The scenario's file graphs are registered first ([`scenario_registry`]). Placements
/// without behavior, sensors and force are spawned with [`Engine::spawn`] exactly as in M2.
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
    let registry = &scenario_registry(registry, scenario)?;
    let unknown = |kind, id: &cgf_core::ComponentId| EngineError::UnknownComponent {
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
    let mut entities = Vec::with_capacity(scenario.entities.len());
    for (index, placement) in scenario.entities.iter().enumerate() {
        let motion = registry
            .motion_model(&placement.motion.id)
            .ok_or_else(|| unknown(ComponentKind::MotionModel, &placement.motion.id))?
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
