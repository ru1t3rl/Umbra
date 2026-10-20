//! `umbra-templates`: template and scenario files for Umbra.
//!
//! Responsibility (M2): read TOML templates and scenarios with source locations, resolve
//! inheritance, validate component parameters against the schemas registered in
//! `umbra_core::Registry`, and report every error with `path:line:col` (REQ-T-1..5).
//! Components are named by ID only; this crate never sees a concrete motion model or
//! terrain type.
//!
//! Dependency rule (spec section 4, REQ-A-1): depends on `umbra-core` only among workspace crates; never on
//! `umbra-kernel` or `umbra-motion`. It contains no `unsafe` code.

#![forbid(unsafe_code)]

pub(crate) mod components;
pub mod config;
pub mod diagnostic;
pub(crate) mod discover;
pub(crate) mod graph_read;
pub mod graph_resolve;
pub mod loader;
pub(crate) mod overlay;
pub(crate) mod position;
pub(crate) mod reader;
pub mod resolve;
pub mod scenario;
pub(crate) mod scenario_read;
pub mod schema;
pub mod source;
pub mod template;
pub mod value;

pub use diagnostic::{Diagnostic, DiagnosticCode, Diagnostics, Location, Note};
pub use discover::{ScenarioInputs, io_diagnostic, scenario_inputs, scenario_plugin_paths};
pub use graph_resolve::GraphSet;
pub use loader::{Loader, LoaderError, LoaderOptions, ValidationReport};
pub use resolve::{ComponentRef, ResolvedTemplate, TemplateSet};
pub use scenario::{ClockSetting, Placement, RoutePlan, Scenario, TickRateSource};
pub use scenario_read::graph_document_id;
pub use schema::{CompiledSchema, SchemaError};
pub use source::SourceFile;
pub use value::{LTable, LValue};

/// Schema version of template and scenario files (ADR 0010, ASM-057).
pub const SCHEMA_VERSION: u32 = 1;

/// The JSON Schema of template files (format v1, ADR 0010), in the ADR 0009 subset.
pub const TEMPLATE_FILE_SCHEMA: &str = include_str!("../schemas/template.v1.schema.json");

/// The JSON Schema of scenario files (format v1, ADR 0010), in the ADR 0009 subset.
pub const SCENARIO_FILE_SCHEMA: &str = include_str!("../schemas/scenario.v1.schema.json");

/// The JSON Schema of behavior graph files (`*.behavior.toml`, format v1, ADR 0012), in the
/// ADR 0009 subset. It checks structure only; the reader is authoritative.
pub const BEHAVIOR_FILE_SCHEMA: &str = include_str!("../schemas/behavior.v1.schema.json");

/// The JSON Schema of the `umbra.toml` configuration file (format v1, ADR 0029), in the ADR 0009
/// subset. The reader is authoritative.
pub const CONFIG_FILE_SCHEMA: &str = include_str!("../schemas/config.v1.schema.json");
