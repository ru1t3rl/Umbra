//! Network adapters (REQ-X-5, ADR 0035): the adapters of the loaded scenario with their state and
//! counters, and the network fault that can stop a run. Added in API 1.1.0.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// The keys an adapter issues for the entities of this engine: `(base, base + count]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct KeySpaceDto {
    /// The key just below the first issued key.
    pub base: u64,
    /// How many keys are issued.
    pub count: u64,
}

/// The counters of one adapter. Every counter only grows during a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct AdapterCountersDto {
    /// Messages handed to the adapter.
    pub published: u64,
    /// Messages taken from the adapter.
    pub received: u64,
    /// Inbound messages dropped as echoes of this engine's own entities.
    pub echo_dropped: u64,
    /// Inbound entities whose type maps to no template.
    pub unmapped: u64,
    /// Local keys refused because the key space is used up.
    pub keys_exhausted: u64,
    /// Ghosts refused because the ghost limit was reached.
    pub ghosts_capped: u64,
    /// Inbound run-control messages ignored.
    pub control_ignored: u64,
    /// Ghost operations the world refused.
    pub world_errors: u64,
    /// Inbound messages of a kind this version does not know.
    pub unsupported: u64,
    /// Inbound entity states with a value that is not finite, refused.
    pub invalid_state: u64,
    /// Outbound messages the adapter dropped (queue full or send failed).
    pub tx_dropped: u64,
    /// Inbound messages the adapter dropped (queue full).
    pub rx_dropped: u64,
    /// Inbound datagrams or frames that could not be decoded.
    pub decode_errors: u64,
    /// I/O errors the adapter saw.
    pub io_errors: u64,
}

/// A network adapter of the loaded scenario: `GET /network/adapters` and the answer of enable and
/// disable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct AdapterDto {
    /// The adapter's name in the scenario; it addresses the adapter in the enable and disable paths
    /// and is the `remote_source` of its ghosts.
    pub name: String,
    /// The registered adapter component the scenario uses (`adapter` in `[[network_adapters]]`).
    pub adapter: String,
    /// The plugin that provides the adapter; absent for a built-in adapter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin: Option<String>,
    /// Whether the adapter is polled and published to.
    pub enabled: bool,
    /// Whether a fault disabled the adapter for the rest of the session.
    pub faulted: bool,
    /// Why the adapter faulted; only when `faulted`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fault_reason: Option<String>,
    /// The keys it issues.
    pub key_space: KeySpaceDto,
    /// Live ghosts from this adapter.
    pub ghosts: u64,
    /// Its counters.
    pub counters: AdapterCountersDto,
}

/// The network adapter fault behind `stop_reason = network_fault`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct NetworkFaultDto {
    /// The adapter's name in the scenario.
    pub adapter: String,
    /// Why it faulted.
    pub reason: String,
}
