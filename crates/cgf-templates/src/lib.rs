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
//!
//! STUB (M2 step 4): the module signatures exist so the acceptance tests compile; the logic
//! arrives in WI-4..WI-8 and WI-12 (`todo!()` bodies).

#![forbid(unsafe_code)]

pub mod diagnostic;
pub mod loader;
pub mod resolve;
pub mod scenario;
pub mod schema;
pub mod source;
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

/// The JSON Schema of template files (WI-12; the stub is empty so the drift tests fail).
pub const TEMPLATE_FILE_SCHEMA: &str = "";

/// The JSON Schema of scenario files (WI-12; the stub is empty so the drift tests fail).
pub const SCENARIO_FILE_SCHEMA: &str = "";
