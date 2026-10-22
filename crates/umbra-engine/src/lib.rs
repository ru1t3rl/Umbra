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
pub mod bus;
pub mod confine;
pub mod display;
pub mod engine;
pub mod error;
pub mod handle;
pub mod net;
pub mod plugins;
pub mod record;
pub mod replay;
pub mod scenario;
pub mod session;
mod spawner;
pub mod validate;
mod visual;

pub use confine::{ConfineError, Confinement};
pub use display::{EntityDisplay, PositionDisplay};
pub use engine::{
    AgentInit, Engine, EngineConfig, Pumped, RouteSpec, RunSummary, RunUntil, SpawnSpec, TickView,
};
pub use error::{EngineError, SpawnParam};
pub use net::{GhostInfo, MANAGER_TEMPLATE_KEYS, NetFault, NetLink, net_summary};
pub use plugins::{
    CapabilityInfo, HostOptions, LoadedScenario, PluginError, PluginFault, PluginFaultPolicy,
    PluginHost, PluginInfo, PluginLog, instantiate_with_plugins, load_plugins,
    load_scenario_with_plugins, load_scenario_with_plugins_and_settings, resolve_plugin_paths,
    validate_path_with_plugins, validate_path_with_plugins_and_settings,
};
pub use record::{RecordOptions, Recorder, RecordingState, RecordingStatus};
pub use replay::{ReplayError, ReplayOptions, ReplaySummary, replay};
pub use scenario::{EntityInfo, LoadSettings, ScenarioError};
pub use session::{Session, SessionError, SessionOptions, TemplateSpawn};
pub use spawner::{LifecycleCounters, MAX_ENTITIES, MAX_SPAWNS_PER_TICK};
/// The result of ticking a behavior (REQ-B-8), as an entity snapshot reports it.
pub use umbra_core::Status as BehaviorStatus;
pub use umbra_core::net::KeySpace;
pub use umbra_core::{
    Catalog, CatalogEntry, CatalogKind, ComponentId, ComponentKind, ControlAction, EntityId,
    FlatTerrain, Force, Geodetic, Meters, MetersPerSecond, Ownership, ParamSchema, ParamValue,
    Position, Provenance, Radians, Registry, SimEvent, SimTime, StateHash, StateHashVersion,
    TemplateId, TerrainProvider, TickRate, TimeError, Version,
};
pub use umbra_kernel::{
    ClockError, ClockMode, EntitySnapshot, ManualWallClock, Snapshot, StdWallClock, TickReport,
    TimeScale, WallClock,
};
pub use umbra_net::{AdapterStatus, LoopbackHub};
pub use umbra_templates::config::{
    Config, ConfigError, LogLevel, ServerConfig, StreamConfig, load_config,
};
pub use umbra_templates::{
    BEHAVIOR_FILE_SCHEMA, CONFIG_FILE_SCHEMA, ClockSetting, Diagnostic, Location, Note,
    ResolvedTemplate, SCENARIO_FILE_SCHEMA, ScenarioInputs, Severity, TEMPLATE_FILE_SCHEMA,
    TickRateSource, scenario_inputs,
};
pub use validate::{GraphValidation, validate_graph_document};
