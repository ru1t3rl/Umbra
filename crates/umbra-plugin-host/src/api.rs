//! The public host API (M4 plan, Design "`umbra-plugin-host`"): options, discovered packages,
//! loaded plugin information, and [`PluginHost`], one per scenario load: loading and registration,
//! the per-host fault state and policy, the build scope and attaching to an engine (see the crate
//! docs for the overview).

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use umbra_core::{ComponentId, ComponentKind, PluginFaultPolicy, PluginId, Registry, Version};
use umbra_plugin_api::UmbraPluginEntry;

use crate::bridge::batch::PluginCapability;
use crate::bridge::factory::BuildScope;
use crate::descriptor::Descriptor;
use crate::error::PluginError;
use crate::fault::HostState;
use crate::lifecycle::{self, LoadRequest, LoadedImage};
use crate::log::{PluginLog, TracingLog};
use crate::manifest::Manifest;
use crate::{discover, register};

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
    /// Logs to `tracing` (target `umbra_plugin`), closes libraries on release, run mode.
    fn default() -> Self {
        Self {
            log: Arc::new(TracingLog),
            keep_loaded: false,
            mode: HostMode::Run,
        }
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

/// One per scenario load, attached to at most one engine; owns that engine's fault state.
///
/// Holds its plugins' images (shared with other hosts through the process-wide image table, ADR
/// 0021) in load order; dropping the host releases them in reverse load order. An image's lifecycle
/// ends when its last holder (a host, a registered factory, a behavior or instance handle) releases
/// it.
pub struct PluginHost {
    options: HostOptions,
    /// Fault state, log and mode, shared with this host's capabilities (and so its factories).
    state: Arc<HostState>,
    plugins: Vec<PluginInfo>,
    /// `plugins()` with the `faulted` flags after `k` faults, built on demand (the faulted set only
    /// grows, so the snapshot for each count is unique); reset by `load`.
    snapshots: Vec<OnceLock<Vec<PluginInfo>>>,
    images: Vec<HostImage>,
}

/// One image held by a host.
struct HostImage {
    image: Arc<LoadedImage>,
    origin: String,
    /// The canonical library path (`None` for `load_static`).
    path: Option<PathBuf>,
    /// This host's view of each capability (the registered factories' and batches' shared state).
    capabilities: Vec<Arc<PluginCapability>>,
}

impl PluginHost {
    /// A host with no plugins.
    #[must_use]
    pub fn new(options: HostOptions) -> Self {
        Self {
            state: Arc::new(HostState::new(Arc::clone(&options.log), options.mode)),
            options,
            plugins: Vec::new(),
            snapshots: vec![OnceLock::new()],
            images: Vec::new(),
        }
    }

    /// ADR 0022 layout; packages in path order, then by UTF-8 subdirectory name bytes.
    ///
    /// # Errors
    /// `Discovery`.
    pub fn discover(paths: &[PathBuf]) -> Result<Vec<PluginPackage>, PluginError> {
        discover::discover(paths)
    }

    /// Loads one package: `plugin.toml` check (before opening), image table lookup by canonical
    /// path, open with explicit flags, lookup by entry address (before the entry call), entry, copy
    /// and validation, manifest match, `init`.
    ///
    /// # Errors
    /// Any load error.
    pub fn load(&mut self, package: &PluginPackage) -> Result<&PluginInfo, PluginError> {
        let origin = package
            .library
            .file_name()
            .map_or_else(|| package.library.clone(), |name| package.dir.join(name))
            .display()
            .to_string();
        let manifest = package
            .manifest
            .as_deref()
            .map(Manifest::read)
            .transpose()?;
        if let Some(manifest) = &manifest {
            manifest.check_abi(&origin)?;
        }
        let path = std::fs::canonicalize(&package.library).map_err(|e| PluginError::Open {
            path: package.library.clone(),
            message: format!("cannot resolve the library path: {e}"),
        })?;
        if let Some(first) = self
            .images
            .iter()
            .find(|held| held.path.as_deref() == Some(path.as_path()))
        {
            return Err(PluginError::Duplicate {
                id: first.image.descriptor().id.to_string(),
                first: first.origin.clone(),
                second: origin,
            });
        }
        let precheck = |descriptor: &Descriptor| {
            if let Some(manifest) = &manifest {
                manifest.check(&origin, descriptor)?;
            }
            self.unique_id(descriptor, &origin)
        };
        let request = LoadRequest {
            origin: &origin,
            log: &self.options.log,
            keep_loaded: self.options.keep_loaded,
            precheck: &precheck,
        };
        let image = lifecycle::load_library(&path, &request)?;
        self.adopt(image, origin, Some(path))
    }

    /// Loads an entry function linked into the process (embedders, tests): the same path as
    /// [`PluginHost::load`] without a library or manifest; the image key is the entry address.
    ///
    /// # Errors
    /// Any load error.
    pub fn load_static(
        &mut self,
        origin: &str,
        entry: UmbraPluginEntry,
    ) -> Result<&PluginInfo, PluginError> {
        let precheck = |descriptor: &Descriptor| self.unique_id(descriptor, origin);
        let request = LoadRequest {
            origin,
            log: &self.options.log,
            keep_loaded: self.options.keep_loaded,
            precheck: &precheck,
        };
        let image = lifecycle::load_static(entry, &request)?;
        self.adopt(image, origin.to_owned(), None)
    }

    /// Registers every capability of every loaded plugin (load order, then descriptor order) with
    /// `Provenance::Plugin`; all-or-nothing (ASM-091 d).
    ///
    /// # Errors
    /// `Registry`.
    pub fn register(&self, registry: &mut Registry) -> Result<(), PluginError> {
        register::register(
            self.images.iter().flat_map(|held| &held.capabilities),
            registry,
        )
    }

    /// The loaded plugins in load order, with each capability's `faulted` flag for this host.
    #[must_use]
    pub fn plugins(&self) -> &[PluginInfo] {
        let faulted = self.state.faulted();
        let Some(slot) = self.snapshots.get(faulted.len()) else {
            return &self.plugins;
        };
        slot.get_or_init(|| {
            let mut plugins = self.plugins.clone();
            for &(plugin, capability) in &faulted {
                if let Some(c) = plugins
                    .get_mut(plugin)
                    .and_then(|p| p.capabilities.get_mut(capability))
                {
                    c.faulted = true;
                }
            }
            plugins
        })
    }

    /// Sets the policy applied to later faults (ADR 0021; default `Abort`).
    pub fn set_fault_policy(&self, policy: PluginFaultPolicy) {
        self.state.set_policy(policy);
    }

    /// Faults in occurrence order, this host only.
    #[must_use]
    pub fn faults(&self) -> Vec<PluginFault> {
        self.state.faults()
    }

    /// The first fault under `Abort`, once.
    #[must_use]
    pub fn take_abort(&self) -> Option<PluginFault> {
        self.state.take_abort()
    }

    /// Opens this host's build scope on the calling thread (RR-m2, ASM-138): while the guard lives,
    /// the factories this host registered build here, which in [`HostMode::Run`] they do nowhere
    /// else ("plugin host not attached to this engine"). The engine the host is attached to opens
    /// it around building and spawning; [`HostMode::Validation`] factories build without it.
    pub fn build_scope(&self) -> BuildScope<'_> {
        BuildScope::open(&self.state)
    }

    /// Marks this host as taken by an engine (ADR 0021: a host belongs to at most one engine);
    /// `false` if an engine already took it. Called by `Engine::attach_plugins` (`umbra-engine`).
    #[must_use]
    pub fn try_attach(&self) -> bool {
        self.state.try_attach()
    }

    /// Sum of the capacities of the bridge's scratch buffers of every image of this host: the
    /// warm-up growth probe of the tests (like `Kernel::behavior_scratch_capacity`, ASM-125 a).
    #[doc(hidden)]
    #[must_use]
    pub fn scratch_capacity(&self) -> usize {
        self.images
            .iter()
            .map(|held| held.image.scratch_capacity())
            .sum()
    }

    /// ADR 0021: one plugin id per host; another image with a loaded id is a `Duplicate`.
    fn unique_id(&self, descriptor: &Descriptor, origin: &str) -> Result<(), PluginError> {
        match self
            .images
            .iter()
            .find(|held| held.image.descriptor().id == descriptor.id)
        {
            Some(first) => Err(PluginError::Duplicate {
                id: descriptor.id.to_string(),
                first: first.origin.clone(),
                second: origin.to_owned(),
            }),
            None => Ok(()),
        }
    }

    /// Takes a loaded or reused image into this host. The same image twice is a `Duplicate` (found
    /// before any entry call: a live image is reused without calling it).
    fn adopt(
        &mut self,
        image: Arc<LoadedImage>,
        origin: String,
        path: Option<PathBuf>,
    ) -> Result<&PluginInfo, PluginError> {
        if let Some(first) = self
            .images
            .iter()
            .find(|held| Arc::ptr_eq(&held.image, &image))
        {
            return Err(PluginError::Duplicate {
                id: image.descriptor().id.to_string(),
                first: first.origin.clone(),
                second: origin,
            });
        }
        let info = plugin_info(image.descriptor(), &origin);
        let index = self.plugins.len();
        let capabilities = (0..image.descriptor().capabilities.len())
            .map(|c| {
                Arc::new(PluginCapability::new(
                    Arc::clone(&image),
                    c,
                    (index, c),
                    Arc::clone(&self.state),
                ))
            })
            .collect();
        self.images.push(HostImage {
            image,
            origin,
            path,
            capabilities,
        });
        self.plugins.push(info);
        let total: usize = self.plugins.iter().map(|p| p.capabilities.len()).sum();
        self.snapshots = (0..=total).map(|_| OnceLock::new()).collect();
        Ok(&self.plugins[index])
    }
}

impl Drop for PluginHost {
    /// Releases this host's images in reverse load order; the last release of an image runs
    /// `shutdown` (attributed to this host's log) and closes the library.
    fn drop(&mut self) {
        for held in &self.images {
            held.image.released_by(&self.options.log);
        }
        while let Some(held) = self.images.pop() {
            drop(held);
        }
    }
}

/// The public view of a copied descriptor.
fn plugin_info(descriptor: &Descriptor, origin: &str) -> PluginInfo {
    PluginInfo {
        id: descriptor.id.clone(),
        name: descriptor.name.clone(),
        version: descriptor.version.clone(),
        abi: descriptor.abi,
        origin: origin.to_owned(),
        capabilities: descriptor
            .capabilities
            .iter()
            .map(|c| CapabilityInfo {
                kind: c.kind,
                id: c.id.clone(),
                version: c.version.clone(),
                faulted: false,
            })
            .collect(),
    }
}
