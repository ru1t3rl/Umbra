//! `umbra-plugin-host`: loads native Umbra plugins and bridges them to the core traits (D-6, D-8).
//!
//! Single responsibility: discover plugin packages, check `plugin.toml` and the ABI version, open
//! the library, copy and validate the descriptor, run the plugin lifecycle, and register each
//! capability in the one `umbra_core::Registry` with `Provenance::Plugin` (REQ-P-1..10, ADR
//! 0019-0022). Plugin faults follow `on_plugin_fault` per host (REQ-P-7).
//!
//! Dependency rule (section 4): `umbra-core` and `umbra-plugin-api` only.
//!
//! # Loading (M4 WI-8)
//! [`PluginHost::discover`] lists packages (ADR 0022); [`PluginHost::load`] checks the manifest
//! before opening, looks the image up by canonical path, opens the library with explicit flags
//! (`RTLD_NOW | RTLD_LOCAL`; on Windows `LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR |
//! LOAD_LIBRARY_SEARCH_DEFAULT_DIRS` with an absolute path), looks the image up again by the
//! address of `umbra_plugin_entry` **before** calling it, then calls the entry exactly twice,
//! copies and validates the descriptor and runs `init`. [`PluginHost::load_static`] does the same
//! for an entry function linked into the process. [`PluginHost::register`] registers every
//! capability, all-or-nothing.
//!
//! # Global state, threads and locks
//! - **Process-wide image table** (module `image`): the one piece of persistent process-global
//!   mutable state (ASM-122). One `LoadedImage` per image lifecycle, shared by `Arc` across hosts,
//!   with the states `Loading`, `Live` and `Closing`; loads that meet `Loading` or `Closing` wait
//!   on a condvar. The only other process-global item is the host-id counter of the build scope
//!   (module `fault`, ASM-138), which only names hosts and never affects results.
//! - **Thread-local call records** (module `tokens`): a stack of in-progress plugin calls per
//!   thread, pushed for every call into a plugin (`init`, `shutdown`, and the bridge's calls) and
//!   popped after it, so it is call-scoped and empty whenever no plugin call runs on that thread
//!   (ASM-122). Host callbacks resolve their tokens against the top record of the calling thread
//!   only; without one they return `UMBRA_ERR_STATE`.
//! - **Lock order** (ADR 0019, RR-M2): (1) the image table, never held while plugin code runs or
//!   while an `Arc<LoadedImage>` may drop; (2) the per-image call lock, **blocking**, held for the
//!   duration of a plugin call (contention waits, never faults; same-thread re-entry, found through
//!   the call records, fails without taking it; no `try_lock` anywhere); (3) per-host fault state,
//!   a leaf taken after (2) inside callbacks; (4) [`PluginLog`], a leaf that never calls into the
//!   host.
//!
//! This is one of the three crates allowed to contain `unsafe` (CLAUDE.md rule 5, NFR-2). Every
//! `unsafe` block carries a `// SAFETY:` comment naming the invariant it relies on and is exercised
//! by a test. The behavior bridge (module `bridge`) and the per-host fault state (module `fault`)
//! are M4 WI-9.

#![deny(unsafe_op_in_unsafe_fn)]
#![warn(clippy::undocumented_unsafe_blocks)]

mod api;
mod bridge;
mod descriptor;
mod discover;
mod error;
mod fault;
mod host_api;
mod image;
mod library;
mod lifecycle;
mod log;
mod manifest;
mod register;
mod tokens;

pub use api::{
    CapabilityInfo, HostMode, HostOptions, PluginFault, PluginHost, PluginInfo, PluginPackage,
};
pub use bridge::factory::BuildScope;
pub use error::PluginError;
pub use log::{LogLevel, PluginLog};
