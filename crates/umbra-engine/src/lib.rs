//! `umbra-engine`: the embeddable, synchronous facade over kernel and motion models.
//!
//! Responsibility: `Engine` construction, spawning with placement by ECEF or lat/lon/alt
//! (ASM-044), running and stepping the kernel by its clock, and loading and instantiating scenario
//! files through the component registry (REQ-S-2, REQ-T-1, ASM-026).
//!
//! This crate re-exports the core, kernel and motion types that `umbra-cli` needs, because the
//! dependency rules let the CLI depend on `umbra-engine` only (ASM-026).
//!
//! Dependency rule (REQ-A-2): depends on `umbra-core`, `umbra-kernel`, `umbra-motion` and (M4)
//! `umbra-plugin-host`. It contains no `unsafe` code.

#![forbid(unsafe_code)]

pub mod builtins;
pub mod confine;
pub mod display;
pub mod engine;
pub mod error;
pub mod plugins;
pub mod scenario;
pub mod session;
pub mod validate;
mod visual;

pub use confine::{ConfineError, Confinement};
pub use display::EntityDisplay;
pub use engine::{
    AgentInit, Engine, EngineConfig, Pumped, RouteSpec, RunSummary, RunUntil, SpawnSpec, TickView,
};
pub use error::{EngineError, SpawnParam};
pub use plugins::{
    CapabilityInfo, HostOptions, LoadedScenario, PluginError, PluginFault, PluginFaultPolicy,
    PluginHost, PluginInfo, PluginLog, instantiate_with_plugins, load_plugins,
    load_scenario_with_plugins, load_scenario_with_plugins_and_settings, resolve_plugin_paths,
    validate_path_with_plugins, validate_path_with_plugins_and_settings,
};
pub use scenario::LoadSettings;
pub use session::{Session, SessionError, SessionOptions, TemplateSpawn};
pub use umbra_core::{
    Catalog, CatalogEntry, CatalogKind, ComponentId, ComponentKind, EntityId, FlatTerrain,
    Geodetic, Meters, ParamSchema, ParamValue, Position, Provenance, Radians, Registry, SimTime,
    StateHash, TerrainProvider, TickRate, TimeError,
};
pub use umbra_kernel::{
    ClockError, ClockMode, EntitySnapshot, ManualWallClock, Snapshot, StdWallClock, TickReport,
    TimeScale, WallClock,
};
pub use umbra_templates::config::{
    Config, ConfigError, LogLevel, ServerConfig, StreamConfig, load_config,
};
pub use umbra_templates::{
    BEHAVIOR_FILE_SCHEMA, CONFIG_FILE_SCHEMA, SCENARIO_FILE_SCHEMA, ScenarioInputs,
    TEMPLATE_FILE_SCHEMA, scenario_inputs,
};
pub use validate::{GraphValidation, validate_graph_document};
