//! A loaded scenario and its reader (ADR 0010, REQ-T-2, REQ-T-5, REQ-C-3, ASM-044, ASM-058,
//! ASM-060, ASM-066).

use std::path::PathBuf;

use umbra_core::net::NetEntityType;
pub use umbra_core::net::PublishSettings;
use umbra_core::{
    ComponentId, Force, Meters, MetersPerSecond, ParamValue, Position, Radians, Seconds,
    TemplateId, TickRate,
};

use crate::graph_resolve::GraphSet;
use crate::resolve::{ComponentRef, TemplateSet};

/// The clock a scenario asks for; mirrors `umbra_kernel::TimeScale` bounds (ASM-060).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ClockSetting {
    /// As fast as possible.
    AsFastAsPossible,
    /// Real time.
    RealTime,
    /// Stepped.
    Stepped,
    /// Scaled by a factor in `[1e-3, 1e6]`.
    Scaled(f64),
}

/// An entity's initial route.
#[derive(Debug, Clone, PartialEq)]
pub struct RoutePlan {
    /// Waypoints in visiting order; at least one.
    pub waypoints: Vec<Position>,
    /// Arrival radius.
    pub arrival_radius: Meters,
    /// Cruise speed.
    pub cruise_speed: MetersPerSecond,
}

/// One entity to spawn.
#[derive(Debug, Clone, PartialEq)]
pub struct Placement {
    /// The template.
    pub template: TemplateId,
    /// Optional unique name.
    pub name: Option<String>,
    /// Optional group.
    pub group: Option<String>,
    /// Where it starts.
    pub position: Position,
    /// Initial heading.
    pub heading: Radians,
    /// Initial speed.
    pub speed: MetersPerSecond,
    /// The template's motion parameters merged with the overrides.
    pub motion: ComponentRef,
    /// The initial route.
    pub route: Option<RoutePlan>,
    /// The template's behavior with its merged parameters.
    pub behavior: Option<ComponentRef>,
    /// The template's sensors in order.
    pub sensors: Vec<ComponentRef>,
    /// The entity's force (`None` is neutral, ASM-079).
    pub force: Option<Force>,
}

/// Where a scenario's tick rate came from (ADR 0010 note of 2026-10-19).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum TickRateSource {
    /// `[scenario] tick_rate_hz` in the file.
    Scenario,
    /// The key was absent; the loader's default applied (`LoaderOptions::default_tick_rate`).
    Default,
}

/// How an adapter's ghosts are kept (`[network_adapters.ghosts]`).
#[derive(Debug, Clone, PartialEq)]
pub struct GhostSettings {
    /// A ghost silent for longer than this is removed.
    pub timeout: Seconds,
    /// At most this many ghosts; `>= 1`.
    pub max: usize,
    /// At most this many inbound messages applied per tick; `>= 1`.
    pub max_inbound_per_tick: usize,
    /// Template for types that map to none.
    pub default_template: Option<TemplateId>,
    /// Token to template overrides, ascending by token.
    pub type_map: Vec<(NetEntityType, TemplateId)>,
}

impl Default for GhostSettings {
    fn default() -> Self {
        Self {
            timeout: Seconds(12.0),
            max: 10_000,
            max_inbound_per_tick: 4096,
            default_template: None,
            type_map: Vec::new(),
        }
    }
}

/// One `[[network_adapters]]` entry (ADR 0030, ASM-227).
#[derive(Debug, Clone, PartialEq)]
pub struct NetworkAdapterSpec {
    /// The registered network adapter component.
    pub adapter: ComponentId,
    /// Unique name, `[a-z][a-z0-9_]*`; selects the templates' `[network.<name>]` table.
    pub name: String,
    /// Whether the adapter starts enabled.
    pub enabled: bool,
    /// Adapter parameters, as given (validated against the adapter's schema).
    pub params: ParamValue,
    /// Publish settings.
    pub publish: PublishSettings,
    /// Ghost settings.
    pub ghosts: GhostSettings,
}

/// A scenario with its templates resolved.
#[derive(Debug, Clone, PartialEq)]
pub struct Scenario {
    /// Scenario name.
    pub name: String,
    /// The scenario file.
    pub source: PathBuf,
    /// Master seed.
    pub seed: u64,
    /// Ticks per second (the effective rate: the file's, or the default when it was absent).
    pub tick_rate: TickRate,
    /// Whether `tick_rate` came from the file or from the default.
    pub tick_rate_source: TickRateSource,
    /// Requested clock.
    pub clock: ClockSetting,
    /// `ceil(duration_s * hz)`.
    pub duration_ticks: u64,
    /// The terrain provider and parameters.
    pub terrain: ComponentRef,
    /// All templates found.
    pub templates: TemplateSet,
    /// Entities in file order (spawn order).
    pub entities: Vec<Placement>,
    /// The behavior graphs of the load (file graphs, ADR 0012).
    pub graphs: GraphSet,
    /// Plugin directories, resolved against the scenario file's directory like `template_paths`
    /// (`[scenario] plugin_paths`, ADR 0022).
    pub plugin_paths: Vec<PathBuf>,
    /// What to do when a plugin faults (`[scenario] on_plugin_fault`, ADR 0021); default `abort`.
    pub on_plugin_fault: umbra_core::PluginFaultPolicy,
    /// The `[[network_adapters]]` entries in file order; empty when the key is absent (M6, ADR 0030).
    pub network_adapters: Vec<NetworkAdapterSpec>,
}
