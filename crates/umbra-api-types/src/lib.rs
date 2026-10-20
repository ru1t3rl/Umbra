//! `umbra-api-types`: the wire types of the Umbra control API and stream.
//!
//! Responsibility: plain `serde` + `utoipa` data types shared by `umbra-server` and the generated
//! contract files (REQ-X-4, REQ-X-6, D-12, ADR 0025, ADR 0026, ADR 0027). They are separate from the
//! domain types; `umbra-server` maps between the two at the edge.
//!
//! Conventions: requests refuse unknown fields (`deny_unknown_fields`) and responses never do, so
//! clients ignore additive fields; units are field suffixes (`_deg`, `_m`, `_mps`, `_s`, `_hz`);
//! entity ids and ticks are JSON integers (exact below 2^53); a seed in a response is a decimal
//! string; a state hash is 16 lower-case hex digits ([`hex_state_hash`]).
//!
//! Dependency rule (section 4, REQ-A-1..4): no workspace dependencies. It contains no `unsafe` code.

#![forbid(unsafe_code)]

mod catalog;
mod components;
mod entities;
mod plugins;
mod problem;
mod run;
mod seed;
mod status;
mod stream;
mod templates;
mod validation;

pub use catalog::{BehaviorCatalog, CatalogEntry, CatalogKind};
pub use components::{ComponentEntry, ComponentKind, Provenance};
pub use entities::{
    EntityDetail, EntityPage, EntitySummary, GeoBox, Ownership, RouteProgress, SpawnRequest,
};
pub use plugins::{CapabilityDto, PluginDto, PluginFaultDto, PluginReport};
pub use problem::{Problem, ProblemCode};
pub use run::{LoadScenarioRequest, MAX_STEP_COUNT, StepRequest, StepResult, hex_state_hash};
pub use seed::Seed;
pub use status::{
    ClockSetting, EngineStatus, RunState, RunStatus, ScenarioInfo, StopReason, TickRateSource,
};
pub use stream::{
    ClientMessage, EventDto, Frame, FrameCause, FrameKind, Hello, STREAM_PROTOCOL, ServerMessage,
    StreamError, StreamErrorCode, StreamFilter, StreamMessage, StreamSettings, Subscription,
};
pub use templates::{ComponentRef, TemplateDetail, TemplateSummary};
pub use validation::{Diagnostic, DiagnosticNote, ValidateGraphRequest, ValidationResult};

/// Version of the REST and stream contract (semver, additive changes only; ADR 0026).
///
/// It is the `info.version` of the generated `OpenAPI` document, not the crate version.
pub const API_VERSION: &str = "1.0.0";

/// Every wire type as an `OpenAPI` component (review m6: no DTO is left out of the contract). The
/// server merges these into its document (WI-11, WI-12); this type has no paths of its own.
#[derive(utoipa::OpenApi)]
#[openapi(components(schemas(
    BehaviorCatalog,
    CapabilityDto,
    CatalogEntry,
    CatalogKind,
    ClientMessage,
    ClockSetting,
    ComponentEntry,
    ComponentKind,
    ComponentRef,
    Diagnostic,
    DiagnosticNote,
    EngineStatus,
    EntityDetail,
    EntityPage,
    EntitySummary,
    EventDto,
    Frame,
    FrameCause,
    FrameKind,
    GeoBox,
    Hello,
    LoadScenarioRequest,
    Ownership,
    PluginDto,
    PluginFaultDto,
    PluginReport,
    Problem,
    ProblemCode,
    Provenance,
    RouteProgress,
    RunState,
    RunStatus,
    ScenarioInfo,
    Seed,
    ServerMessage,
    SpawnRequest,
    StepRequest,
    StepResult,
    StopReason,
    StreamError,
    StreamErrorCode,
    StreamFilter,
    StreamMessage,
    StreamSettings,
    Subscription,
    TemplateDetail,
    TemplateSummary,
    TickRateSource,
    ValidateGraphRequest,
    ValidationResult,
)))]
pub struct ApiSchemas;

#[cfg(test)]
mod tests {
    use super::API_VERSION;

    #[test]
    fn api_version_is_semver_triple_starting_at_one() {
        let parts: Vec<&str> = API_VERSION.split('.').collect();
        assert_eq!(parts.len(), 3, "{API_VERSION}");
        assert!(
            parts.iter().all(|p| p.parse::<u32>().is_ok()),
            "{API_VERSION}"
        );
        assert_eq!(parts[0], "1");
    }
}
