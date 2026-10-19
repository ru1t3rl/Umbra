//! A loaded scenario and its reader (ADR 0010, REQ-T-2, REQ-T-5, REQ-C-3, ASM-044, ASM-058,
//! ASM-060, ASM-066).

use std::path::PathBuf;

use umbra_core::{Force, Meters, MetersPerSecond, Position, Radians, TemplateId, TickRate};

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

/// A scenario with its templates resolved.
#[derive(Debug, Clone, PartialEq)]
pub struct Scenario {
    /// Scenario name.
    pub name: String,
    /// The scenario file.
    pub source: PathBuf,
    /// Master seed.
    pub seed: u64,
    /// Ticks per second.
    pub tick_rate: TickRate,
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
    /// (`[scenario] plugin_paths`, ADR 0022). M4 WI-11 stub: always empty until the reader fills it.
    pub plugin_paths: Vec<PathBuf>,
    /// What to do when a plugin faults (`[scenario] on_plugin_fault`, ADR 0021). M4 WI-11 stub: always
    /// the default until the reader fills it.
    pub on_plugin_fault: umbra_core::PluginFaultPolicy,
}
