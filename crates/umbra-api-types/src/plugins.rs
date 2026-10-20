//! Loaded plugins (REQ-X-5, ADR 0026).

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::components::ComponentKind;

/// A runtime plugin fault (ADR 0021).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct PluginFaultDto {
    /// The plugin id.
    pub plugin: String,
    /// The faulted capability id.
    pub capability: String,
    /// The call that faulted.
    pub call: String,
    /// The message.
    pub message: String,
}

/// One capability of a plugin.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct CapabilityDto {
    /// The kind.
    pub kind: ComponentKind,
    /// The component id.
    pub id: String,
    /// Its version.
    pub version: String,
    /// Whether this session's host has faulted it.
    pub faulted: bool,
}

/// A loaded plugin.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct PluginDto {
    /// The plugin id.
    pub id: String,
    /// Its display name.
    pub name: String,
    /// Its version.
    pub version: String,
    /// The ABI major version it needs.
    pub abi_major: u32,
    /// The ABI minor version it needs.
    pub abi_minor: u32,
    /// Where it was loaded from, relative to `root` (never absolute).
    pub origin: String,
    /// The directory it came from: `scenario` or `plugins[<i>]`.
    pub root: String,
    /// Its capabilities in descriptor order.
    pub capabilities: Vec<CapabilityDto>,
}

/// `GET /plugins`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct PluginReport {
    /// The loaded plugins in load order.
    pub plugins: Vec<PluginDto>,
    /// The fault that stopped the run, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fault: Option<PluginFaultDto>,
}
