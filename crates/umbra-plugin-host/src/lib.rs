//! `umbra-plugin-host`: loads native Umbra plugins and bridges them to the core traits (D-6, D-8).
//!
//! Single responsibility: discover plugin packages, check `plugin.toml` and the ABI version, open
//! the library, copy and validate the descriptor, run the plugin lifecycle, and register each
//! capability in the one `umbra_core::Registry` with `Provenance::Plugin` (REQ-P-1..10, ADR
//! 0019-0022). Plugin faults follow `on_plugin_fault` per host (REQ-P-7).
//!
//! Dependency rule (section 4): `umbra-core` and `umbra-plugin-api` only.
//!
//! The plugin author's reference (ABI rules, lifecycle, memory, faults, packaging) is
//! `docs/plugin-guide.md`; this crate is the host side of the rules stated there.
//!
//! # Loading and the image table
//! [`PluginHost::discover`] lists packages (ADR 0022); [`PluginHost::load`] checks the manifest
//! before opening, looks the image up by canonical path, opens the library with explicit flags
//! (`RTLD_NOW | RTLD_LOCAL`; on Windows `LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR |
//! LOAD_LIBRARY_SEARCH_DEFAULT_DIRS` with an absolute path), looks the image up again by the
//! address of `umbra_plugin_entry` **before** calling it, then calls the entry exactly twice,
//! copies and validates the descriptor and runs `init`. [`PluginHost::load_static`] does the same
//! for an entry function linked into the process. [`PluginHost::register`] registers every
//! capability, all-or-nothing.
//!
//! Images live in a process-wide table (module `image`), one `LoadedImage` per image lifecycle,
//! shared by `Arc` across hosts, so a second host (or a second load) of a live image reuses it
//! without calling the entry again (ADR 0021 "One image per process"). Every host, registered
//! factory, behavior and instance handle holds its image; the last release runs `shutdown` and
//! closes the library (module `lifecycle`), so unloading happens only after every handle is gone.
//!
//! # The bridge
//! Each behavior capability is registered as a core `Factory<dyn Behavior>` whose `build` calls the
//! plugin's `configure` (module `bridge`). The configured behavior and its instances are core
//! `Behavior` / `BehaviorInstance` objects; all instances of one capability share one
//! `BehaviorBatch`, so the kernel ticks them in one `tick_batch` call per tick. Each network adapter
//! capability (ABI 0.1, ADR 0033) is registered as a core `Factory<dyn NetworkAdapterConfig>`: its
//! configuration and started adapter are core `NetworkAdapterConfig` / `NetworkAdapter` objects, and
//! `stop` and `destroy` run before the image can be released. Every call into a
//! plugin goes through the image's call lock and pushes a call record that scopes the tokens the
//! plugin may use (host context, RNG, sinks); plugin handles are opaque and never dereferenced.
//!
//! # Per-host fault state and policies
//! Each [`PluginHost`] owns the fault state of the one engine it belongs to (module `fault`, ADR
//! 0021, ASM-118): faults ([`PluginHost::faults`]) are recorded in occurrence order, the faulted
//! capability is skipped by this host from then on, and [`PluginHost::set_fault_policy`] chooses
//! `abort` (the first fault is handed out once by [`PluginHost::take_abort`], which the engine
//! checks after every tick) or `disable_capability` (the run continues without it). Two hosts over
//! one image never share fault state.
//!
//! # Modes, build scope and attaching
//! [`HostMode::Run`] factories build only inside their host's build scope
//! ([`PluginHost::build_scope`], RR-m2, ASM-138), which `umbra-engine` opens around loading a
//! scenario (the loader dry-runs `configure`) and, for the engine the host is attached to, around
//! building and spawning (ASM-145); anywhere else `build` fails ("plugin host not attached to this
//! engine"), so a cloned registry cannot build plugin behaviors whose faults no engine would see.
//! [`HostMode::Validation`] factories build anywhere (dry runs of `configure`) but their instances
//! refuse to tick. [`PluginHost::try_attach`] marks a host as taken by an engine (a host belongs
//! to at most one engine); `umbra-engine` calls it from `Engine::attach_plugins`.
//!
//! # Global state, threads and locks
//! - **Process-wide image table** (module `image`): the one piece of persistent process-global
//!   mutable state (ASM-122), with the image states `Loading`, `Live` and `Closing`; loads that meet
//!   `Loading` or `Closing` wait on a condvar. The only other process-global item is the host-id
//!   counter of the build scope (module `fault`, ASM-138), which only names hosts and never affects
//!   results.
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
//! by a test.

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
