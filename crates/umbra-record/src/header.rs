//! The header line of a recording and the types it holds (ADR 0053).

use std::path::PathBuf;

use umbra_core::net::KeySpace;

/// The first line of a recording.
#[derive(Debug, Clone, PartialEq)]
pub struct Header {
    /// The engine version that wrote the file.
    pub umbra: String,
    /// The scenario and the digests of the files it was loaded from.
    pub scenario: ScenarioRef,
    /// The effective seed of the run.
    pub seed: u64,
    /// The effective tick rate, Hz.
    pub tick_rate_hz: u32,
    /// Where the tick rate came from (ADR 0029).
    pub tick_rate_source: String,
    /// The state hash version the `hash` records use (ADR 0055).
    pub state_hash_version: u32,
    /// Hash cadence in ticks (ASM-624).
    pub hash_every: u64,
    /// The scenario's plugin fault policy.
    pub on_plugin_fault: String,
    /// The plugins the run loaded.
    pub plugins: Vec<PluginRef>,
    /// The network adapters of the run, in scenario order.
    pub network_adapters: Vec<AdapterRef>,
}

/// The scenario a recording was made from.
#[derive(Debug, Clone, PartialEq)]
pub struct ScenarioRef {
    /// The scenario file as given (canonical absolute).
    pub path: PathBuf,
    /// The same, relative to the recording's directory when possible (ASM-633).
    pub relative_path: Option<PathBuf>,
    /// Every file the scenario was loaded from.
    pub files: Vec<FileDigest>,
}

/// A file and its FNV-1a 64 digest (ASM-623).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileDigest {
    /// The file.
    pub path: PathBuf,
    /// FNV-1a 64 over the bytes.
    pub fnv1a64: u64,
}

/// A plugin the run loaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginRef {
    /// The plugin id.
    pub id: String,
    /// The plugin version.
    pub version: String,
    /// The ABI the plugin declared, `(major, minor)`.
    pub abi: (u32, u32),
    /// FNV-1a 64 over the library bytes.
    pub library_fnv1a64: u64,
}

/// A network adapter of the run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterRef {
    /// The scenario name of the adapter.
    pub name: String,
    /// The adapter component id.
    pub adapter: String,
    /// The keys it issues.
    pub key_space: KeySpace,
}
