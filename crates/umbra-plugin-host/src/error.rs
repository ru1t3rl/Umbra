//! Load and registration errors (REQ-P-9, ADR 0019, ADR 0022). Every variant names the origin of
//! the plugin (library path as discovered, or the `load_static` origin) and, where there is one,
//! the offending field (`descriptor.<f>`, `capabilities[<i>].<f>`, `capabilities[<i>].vtable.<f>`,
//! `plugin.toml abi`; ASM-125 b3).

use std::path::PathBuf;

use umbra_core::{PluginId, RegistryError};

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
    #[error(
        "{origin}: {field}: plugin ABI {}.{} is not loadable by host ABI {}.{}",
        .plugin.0, .plugin.1, .host.0, .host.1
    )]
    AbiMismatch {
        /// Origin.
        origin: String,
        /// The struct that carries the version (`descriptor.abi_version`,
        /// `capabilities[0].abi_version`, `capabilities[0].vtable.abi_version`, `plugin.toml abi`);
        /// a discriminator added to the plan's design (ASM-125).
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
    /// A capability kind this host cannot load: a reserved or unknown kind, or (ABI 0.2, until WI-9) a
    /// kind this host has no bridge for yet (condition, action; motion models and sensors are bridged
    /// since WI-8). Reported for the first such capability, only after every capability of the
    /// descriptor passed every descriptor check (ASM-708).
    #[error("{origin}: {field}: capability {capability}: unsupported kind {kind}")]
    Unsupported {
        /// Origin.
        origin: String,
        /// The offending field, `capabilities[<i>].kind` (names the capability's index; ASM-125 b3,
        /// ASM-708).
        field: String,
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
