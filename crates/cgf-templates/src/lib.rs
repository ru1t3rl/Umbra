//! `cgf-templates`: template and scenario files for the CGF Engine.
//!
//! Responsibility (M2): read TOML templates and scenarios with source locations, resolve
//! inheritance, validate component parameters against the schemas registered in
//! `cgf_core::Registry`, and report every error with `path:line:col` (REQ-T-1..5).
//! Components are named by ID only; this crate never sees a concrete motion model or
//! terrain type.
//!
//! Dependency rule (spec section 4, REQ-A-1): depends on `cgf-core` only among workspace crates; never on
//! `cgf-kernel` or `cgf-motion`. It contains no `unsafe` code.

#![forbid(unsafe_code)]

pub(crate) mod components;
pub mod diagnostic;
pub(crate) mod discover;
pub mod loader;
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
pub use loader::{Loader, LoaderError, ValidationReport};
pub use resolve::{ComponentRef, ResolvedTemplate, TemplateSet};
pub use scenario::{ClockSetting, Placement, RoutePlan, Scenario};
pub use schema::{CompiledSchema, SchemaError};
pub use source::SourceFile;
pub use value::{LTable, LValue};

/// Schema version of template and scenario files (ADR 0010, ASM-057).
pub const SCHEMA_VERSION: u32 = 1;

/// The JSON Schema of template files (format v1, ADR 0010), in the ADR 0009 subset.
pub const TEMPLATE_FILE_SCHEMA: &str = include_str!("../schemas/template.v1.schema.json");

/// The JSON Schema of scenario files (format v1, ADR 0010), in the ADR 0009 subset.
pub const SCENARIO_FILE_SCHEMA: &str = include_str!("../schemas/scenario.v1.schema.json");
