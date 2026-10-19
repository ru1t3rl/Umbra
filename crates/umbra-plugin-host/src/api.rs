//! Public host API: M4 step 4 stub (signatures exactly as in the M4 plan; bodies `todo!()`).
//! WI-8 (loader, registration) and WI-9 (bridge, faults) replace the bodies.

use std::path::PathBuf;
use std::sync::Arc;

use umbra_core::{
    ComponentId, ComponentKind, PluginFaultPolicy, PluginId, Registry, RegistryError, Version,
};
use umbra_plugin_api::UmbraPluginEntry;

/// Log severity of a plugin message (`UMBRA_LOG_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum LogLevel {
    /// Error.
    Error,
    /// Warning.
    Warn,
    /// Info.
    Info,
    /// Debug.
    Debug,
    /// Trace.
    Trace,
}

/// Where plugin log messages go; the default forwards to `tracing`.
pub trait PluginLog: Send + Sync {
    /// Called with the plugin the message is attributed to.
    fn log(&self, plugin: &PluginId, level: LogLevel, message: &str);
}

/// How factories registered by a host build.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HostMode {
    /// Factories build only inside the attached engine's build scope.
    #[default]
    Run,
    /// Dry-run builds allowed; instances refuse to tick.
    Validation,
}

/// Host construction options.
#[non_exhaustive]
#[derive(Clone)]
pub struct HostOptions {
    /// Receives plugin log messages.
    pub log: Arc<dyn PluginLog>,
    /// Never close plugin libraries (still calls `shutdown`); sticky per image.
    pub keep_loaded: bool,
    /// Run or validation mode.
    pub mode: HostMode,
}

impl Default for HostOptions {
    #[expect(clippy::todo, reason = "M4 WI-8 stub")]
    fn default() -> Self {
        todo!("M4 WI-8")
    }
}

/// One plugin package found by discovery.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginPackage {
    /// The package directory.
    pub dir: PathBuf,
    /// The library file.
    pub library: PathBuf,
    /// `plugin.toml`, if present.
    pub manifest: Option<PathBuf>,
}

/// A loaded capability.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
pub struct CapabilityInfo {
    /// Capability kind.
    pub kind: ComponentKind,
    /// Component id.
    pub id: ComponentId,
    /// Capability version.
    pub version: Version,
    /// Whether this host has faulted it.
    pub faulted: bool,
}

/// A loaded plugin.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
pub struct PluginInfo {
    /// Plugin id.
    pub id: PluginId,
    /// Display name.
    pub name: String,
    /// Plugin version.
    pub version: Version,
    /// Required ABI `(major, minor)`.
    pub abi: (u32, u32),
    /// Where it came from (library path or the `load_static` origin).
    pub origin: String,
    /// Its capabilities, in descriptor order.
    pub capabilities: Vec<CapabilityInfo>,
}

/// A runtime plugin fault (ADR 0021).
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
pub struct PluginFault {
    /// The plugin.
    pub plugin: PluginId,
    /// The faulted capability.
    pub capability: ComponentId,
    /// The call that faulted (`"tick_batch"`, `"configure"`, ...).
    pub call: &'static str,
    /// Message naming the offending field or panic text.
    pub message: String,
}

/// Plugin load and registration errors.
#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    /// Missing directory, zero or several libraries, non-UTF-8 name.
    #[error("plugin discovery: {path}: {message}")]
    Discovery {
        /// Path.
        path: PathBuf,
        /// Message.
        message: String,
    },
    /// `plugin.toml` syntax, keys or values.
    #[error("plugin manifest {path}: {message}")]
    Manifest {
        /// Path.
        path: PathBuf,
        /// Message.
        message: String,
    },
    /// Not a loadable library.
    #[error("cannot open plugin library {path}: {message}")]
    Open {
        /// Path.
        path: PathBuf,
        /// Message.
        message: String,
    },
    /// No `umbra_plugin_entry`, or a null symbol.
    #[error("{path}: no umbra_plugin_entry symbol")]
    MissingEntry {
        /// Path.
        path: PathBuf,
    },
    /// The entry returned null (includes a panicking SDK entry).
    #[error("{origin}: umbra_plugin_entry returned a null descriptor")]
    NullDescriptor {
        /// Origin.
        origin: String,
    },
    /// The second entry call returned another pointer.
    #[error("{origin}: umbra_plugin_entry returned different descriptors")]
    UnstableEntry {
        /// Origin.
        origin: String,
    },
    /// ABI major differs or minor is newer (REQ-P-9).
    #[error("{origin}: {field}: plugin ABI {}.{} is not loadable by host ABI {}.{}", .plugin.0, .plugin.1, .host.0, .host.1)]
    AbiMismatch {
        /// Origin.
        origin: String,
        /// The struct that carries the version (`descriptor.abi_version`, `capabilities[0].abi_version`,
        /// `vtable.abi_version`, `plugin.toml abi`); a discriminator added to the plan's design (ASM-125).
        field: String,
        /// Plugin `(major, minor)`.
        plugin: (u32, u32),
        /// Host `(major, minor)`.
        host: (u32, u32),
    },
    /// Size, null, alignment, bounds, caps, UTF-8, grammar, semver or metadata problem.
    #[error("{origin}: descriptor field {field}: {message}")]
    Descriptor {
        /// Origin.
        origin: String,
        /// The offending field.
        field: String,
        /// Message.
        message: String,
    },
    /// A capability kind this host cannot load.
    #[error("{origin}: capability {capability}: unsupported kind {kind}")]
    Unsupported {
        /// Origin.
        origin: String,
        /// Capability id.
        capability: String,
        /// Kind number.
        kind: u32,
    },
    /// Descriptor and `plugin.toml` disagree.
    #[error("{origin}: {field} differs: manifest {manifest}, descriptor {descriptor}")]
    ManifestMismatch {
        /// Origin.
        origin: String,
        /// The field.
        field: &'static str,
        /// Manifest value.
        manifest: String,
        /// Descriptor value.
        descriptor: String,
    },
    /// The same image twice in one host, or one plugin id from two images.
    #[error("duplicate plugin {id}: {first} and {second}")]
    Duplicate {
        /// Plugin id.
        id: String,
        /// First location.
        first: String,
        /// Second location.
        second: String,
    },
    /// A `builtin.*` capability id.
    #[error("{origin}: capability id {id} uses the reserved builtin namespace")]
    ReservedId {
        /// Origin.
        origin: String,
        /// The id.
        id: String,
    },
    /// `init` returned an error or panicked.
    #[error("plugin {plugin}: init failed: {message}")]
    Init {
        /// Plugin.
        plugin: PluginId,
        /// Message.
        message: String,
    },
    /// Registration failed.
    #[error(transparent)]
    Registry(#[from] RegistryError),
}

/// One per scenario load, attached to at most one engine; owns that engine's fault state.
pub struct PluginHost {
    _private: (),
}

#[expect(clippy::todo, reason = "M4 WI-8/WI-9 stub")]
impl PluginHost {
    /// A host with no plugins.
    #[must_use]
    pub fn new(_options: HostOptions) -> Self {
        todo!("M4 WI-8")
    }

    /// ADR 0022 layout; packages in path order, then by UTF-8 subdirectory name bytes.
    ///
    /// # Errors
    /// `Discovery`.
    pub fn discover(_paths: &[PathBuf]) -> Result<Vec<PluginPackage>, PluginError> {
        todo!("M4 WI-8")
    }

    /// Loads one package.
    ///
    /// # Errors
    /// Any load error.
    pub fn load(&mut self, _package: &PluginPackage) -> Result<&PluginInfo, PluginError> {
        todo!("M4 WI-8")
    }

    /// Loads an entry function linked into the process (embedders, tests).
    ///
    /// # Errors
    /// Any load error.
    pub fn load_static(
        &mut self,
        _origin: &str,
        _entry: UmbraPluginEntry,
    ) -> Result<&PluginInfo, PluginError> {
        todo!("M4 WI-8")
    }

    /// Registers every capability of every loaded plugin; all-or-nothing.
    ///
    /// # Errors
    /// `Registry`.
    pub fn register(&self, _registry: &mut Registry) -> Result<(), PluginError> {
        todo!("M4 WI-8")
    }

    /// The loaded plugins in load order.
    #[must_use]
    pub fn plugins(&self) -> &[PluginInfo] {
        todo!("M4 WI-8")
    }

    /// Sets the policy applied to faults.
    pub fn set_fault_policy(&self, _policy: PluginFaultPolicy) {
        todo!("M4 WI-9")
    }

    /// Faults in occurrence order, this host only.
    #[must_use]
    pub fn faults(&self) -> Vec<PluginFault> {
        todo!("M4 WI-9")
    }

    /// The first fault under `Abort`, once.
    #[must_use]
    pub fn take_abort(&self) -> Option<PluginFault> {
        todo!("M4 WI-9")
    }
}

impl Drop for PluginHost {
    /// WI-8: releases this host's images; the last release runs `shutdown` and closes the library.
    fn drop(&mut self) {}
}
