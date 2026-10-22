//! Engine and run status (REQ-X-5, ADR 0026).

use serde::{Deserialize, Serialize};
use utoipa::openapi::schema::{AdditionalProperties, ObjectBuilder, OneOfBuilder, Type};
use utoipa::openapi::{RefOr, Schema};
use utoipa::{PartialSchema, ToSchema};

use crate::network::NetworkFaultDto;
use crate::plugins::PluginFaultDto;
use crate::recording::RecordingDto;
use crate::validation::Diagnostic;

/// The run lifecycle state (ADR 0026): `idle` -> `ready` -> `running` <-> `paused`, any loaded state
/// -> `stopped`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunState {
    /// No scenario is loaded.
    Idle,
    /// A scenario is loaded and paused at tick 0.
    Ready,
    /// Ticking.
    Running,
    /// Paused after running.
    Paused,
    /// Stopped; see the stop reason.
    Stopped,
}

/// Why a run is `stopped`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    /// A `stop` command.
    Command,
    /// The scenario's duration was reached.
    Duration,
    /// A plugin fault under the `abort` policy; the status carries the fault.
    PluginFault,
    /// A kernel error.
    Error,
    /// A network adapter fault under the `abort` policy; the status carries `network_fault` (added in
    /// API 1.1.0).
    NetworkFault,
}

/// Where the tick rate came from (ADR 0010 note, ADR 0026).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TickRateSource {
    /// The scenario file's `tick_rate_hz`.
    Scenario,
    /// The server's configured default (60 Hz built in).
    Default,
}

/// How the clock paces ticks. The `mode` values are the scenario file's `clock` spellings, which are
/// also the CLI's `--clock` values. This type is both a request (`PUT /run/clock`) and a response part.
///
// Serde reads it through `ClockWire`, a flat struct, because `deny_unknown_fields` is not applied
// to the unit variants of an internally tagged enum; the schema is still the tagged enum.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "mode",
    rename_all = "lowercase",
    try_from = "ClockWire",
    into = "ClockWire"
)]
pub enum ClockSetting {
    /// As fast as possible.
    Afap,
    /// Real time.
    Realtime,
    /// Tick only when stepped.
    Stepped,
    /// Real time multiplied by `scale`, in `[0.001, 1000000]`.
    Scaled {
        /// The factor.
        scale: f64,
    },
}

impl PartialSchema for ClockSetting {
    fn schema() -> RefOr<Schema> {
        // Written by hand so that every branch refuses extra properties and `scale` carries the
        // kernel's range, as the serde side does.
        let branch = |mode: &str, description: &str| {
            ObjectBuilder::new()
                .schema_type(Type::Object)
                .description(Some(description))
                .property(
                    "mode",
                    ObjectBuilder::new()
                        .schema_type(Type::String)
                        .enum_values(Some([mode])),
                )
                .required("mode")
                .additional_properties(Some(AdditionalProperties::FreeForm(false)))
        };
        let scaled = branch("scaled", "Real time multiplied by `scale`")
            .property(
                "scale",
                ObjectBuilder::new()
                    .schema_type(Type::Number)
                    .minimum(Some(0.001))
                    .maximum(Some(1_000_000.0)),
            )
            .required("scale");
        OneOfBuilder::new()
            .item(branch("afap", "As fast as possible"))
            .item(branch("realtime", "Real time"))
            .item(branch("stepped", "Tick only when stepped"))
            .item(scaled)
            .into()
    }
}

impl ToSchema for ClockSetting {}

/// The wire form of [`ClockSetting`] for serde: `mode` plus `scale` for `scaled` only.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ClockWire {
    mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    scale: Option<f64>,
}

impl From<ClockSetting> for ClockWire {
    fn from(setting: ClockSetting) -> Self {
        let (mode, scale) = match setting {
            ClockSetting::Afap => (ClockSetting::MODES[0], None),
            ClockSetting::Realtime => (ClockSetting::MODES[1], None),
            ClockSetting::Stepped => (ClockSetting::MODES[2], None),
            ClockSetting::Scaled { scale } => (ClockSetting::MODES[3], Some(scale)),
        };
        Self {
            mode: mode.to_string(),
            scale,
        }
    }
}

impl TryFrom<ClockWire> for ClockSetting {
    type Error = String;

    fn try_from(wire: ClockWire) -> Result<Self, String> {
        let [afap, realtime, stepped, scaled] = ClockSetting::MODES;
        let mode = wire.mode.as_str();
        match (mode, wire.scale) {
            (m, None) if m == afap => Ok(Self::Afap),
            (m, None) if m == realtime => Ok(Self::Realtime),
            (m, None) if m == stepped => Ok(Self::Stepped),
            (m, Some(scale)) if m == scaled => Ok(Self::Scaled { scale }),
            (m, None) if m == scaled => Err("clock mode `scaled` needs `scale`".to_string()),
            (m, Some(_)) if [afap, realtime, stepped].contains(&m) => Err(format!(
                "`scale` is only valid with mode `scaled`, not `{m}`"
            )),
            (other, _) => Err(format!(
                "unknown clock mode `{other}` (expected afap, realtime, stepped or scaled)"
            )),
        }
    }
}

impl ClockSetting {
    /// The `mode` spellings, in the order of the scenario file's `clock` enum.
    pub const MODES: [&'static str; 4] = ["afap", "realtime", "stepped", "scaled"];
}

/// The run status reported by `GET /status`, run control and every stream frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct RunStatus {
    /// The lifecycle state.
    pub state: RunState,
    /// Why the run stopped; only when `state` is `stopped`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<StopReason>,
    /// The plugin fault behind `stop_reason = plugin_fault`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fault: Option<PluginFaultDto>,
    /// The network adapter fault behind `stop_reason = network_fault` (added in API 1.1.0).
    // Boxed because the status travels inside every stream frame and the field is almost always
    // absent (it does not change the JSON).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network_fault: Option<Box<NetworkFaultDto>>,
    /// The session recording; only when the server records (added in API 1.2.0, ADR 0057).
    // Boxed like `network_fault`: it travels inside every stream frame and is almost always absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recording: Option<Box<RecordingDto>>,
    /// The current clock.
    pub clock: ClockSetting,
    /// The session number; it changes with every load (0 before the first).
    pub session: u64,
    /// The current tick; only when a scenario is loaded. Exact below 2^53.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tick: Option<u64>,
    /// The simulation time in seconds; only when a scenario is loaded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_s: Option<f64>,
    /// Whether a `step` request is waiting for its ticks.
    pub step_pending: bool,
    /// The tick rate of the loaded scenario.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tick_rate_hz: Option<u32>,
    /// Where the tick rate came from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tick_rate_source: Option<TickRateSource>,
}

/// The loaded scenario (`POST` and `GET /scenario`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ScenarioInfo {
    /// The session number.
    pub session: u64,
    /// The scenario name.
    pub name: String,
    /// The scenario file, relative to the scenario root (never absolute).
    pub path: String,
    /// The seed in force, as a decimal string.
    pub seed: String,
    /// Ticks per second.
    pub tick_rate_hz: u32,
    /// Where the tick rate came from.
    pub tick_rate_source: TickRateSource,
    /// The run length in ticks.
    pub duration_ticks: u64,
    /// Entities at load time.
    pub entity_count: u64,
    /// The scenario's warnings, each with `severity = warning`; absent when there are none (added in
    /// API 1.1.0).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<Diagnostic>,
}

/// `GET /status`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct EngineStatus {
    /// The contract version (`API_VERSION`).
    pub api_version: String,
    /// The run status.
    pub run: RunStatus,
    /// The loaded scenario, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scenario: Option<ScenarioInfo>,
}
