//! Loading and instantiating scenarios (REQ-S-2, REQ-T-1, ASM-059, ASM-062).
//!
//! Components are built through the registry by ID only; this module never names a concrete
//! motion model or terrain type.

use std::path::Path;

use cgf_core::{ComponentKind, EntityId, Ownership, Registry, TemplateId};
use cgf_kernel::{ClockMode, TimeScale, WallClock};
use cgf_templates::{ClockSetting, Loader, LoaderError};
pub use cgf_templates::{
    Diagnostic, DiagnosticCode, Diagnostics, Location, Note, Scenario, ValidationReport,
};

use crate::engine::{Engine, EngineConfig, RouteSpec, SpawnSpec};
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

/// Builds terrain and motion models through the registry and spawns in file order.
///
/// # Errors
/// [`EngineError`]; after a successful validation only the spawn checks can fail.
pub fn instantiate(
    registry: &Registry,
    scenario: &Scenario,
    options: RunOptions,
) -> Result<ScenarioRun, EngineError> {
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
        let id = engine.spawn(SpawnSpec {
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
        })?;
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
