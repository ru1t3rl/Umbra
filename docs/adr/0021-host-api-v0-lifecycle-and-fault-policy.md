# ADR 0021: Host API v0, plugin lifecycle and fault policy

- **Status**: Accepted (human project owner, 2026-10-18; proposed by the architect in the M4 plan `docs/plans/M4.md`).
  Amended 2026-10-18 after the ABI design review (approved by the human): changed rules are marked
  "(amended: R..)"; summary at the end.
- **Requirement IDs**: REQ-P-5, REQ-P-7, REQ-P-8, REQ-P-11, REQ-K-4, REQ-C-1, REQ-X-3, D-8, ADR 0006
- **Affected**: `umbra-plugin-api`, `umbra-plugin-host`, `umbra-sdk`, `umbra-engine`, `umbra-core` (one enum)

## Context
REQ-P-5 lists what the host API provides: logging, `SimRng` access, current `SimTime`, terrain queries,
event emit, memory helpers, capability registration. M4 has one kind (behavior), whose built-in peers get
only time and the entity's RNG stream (`BehaviorContext`, ADR 0013), and emit intents, not events.
REQ-P-8 gives the lifecycle `load -> describe -> configure(params) -> init -> tick/call* -> shutdown ->
unload`. REQ-P-7 names the policy `on_plugin_fault` (`abort` | `disable_capability`) but not its timing
or default. Every host API entry is append-only once shipped. A dynamic library is one image per process:
`dlopen`/`LoadLibrary` return the same image for the same file (glibc also matches by device and inode),
so two loads, or two hosts in one process, share the SDK's statics (review R-B5).

## Options
1. **Full REQ-P-5 list now.** Cons: terrain and events have no M4 consumer (behaviors get neither; giving
   them terrain would be a new behavior input); untestable surface frozen early.
2. **Minimal v0, append the rest with the kinds that use them (chosen).** Appending to `UmbraHostApi` is a
   minor bump (ADR 0019).

## Decision
Option 2.
- **`UmbraHostApi` v0** *(amended: R-B4)*: header, `host_ctx: u64` (a generation-checked token for the
  loaded image, ADR 0019), and three entries, each returning `UmbraCode`:
  `log(host_ctx, level, message)` (to `tracing`, target `umbra_plugin`, field `plugin = <id>`; during a
  call it goes to the calling host's `PluginLog`; an unknown level or invalid UTF-8 is
  `UMBRA_ERR_INVALID_ARGUMENT`; after `shutdown` it is `UMBRA_ERR_STATE`),
  `rng_next_u64(host_ctx, rng, out: *mut u64)` and `rng_next_f64(host_ctx, rng, out: *mut f64)` (in
  [0, 1)), where `rng` is the batch item's opaque RNG token. The token names the item's entity stream for
  the duration of that call only, so plugin draws come from the entity's own hashed stream (ADR 0006) and
  are covered by the state hash (REQ-K-4). Drawing with another item's token of the same call breaks
  the per-entity independence of ADR 0013 and is forbidden by contract, but the host cannot detect it
  inside one call: it draws from that other item's stream, deterministically (the failure matrix pins
  this). A token from another or a finished call, or used outside any call, is refused and faults the
  capability. The `UmbraHostApi` struct is a process-lifetime static, one per loaded image.
  **Time** is passed in every tick call (`tick`, `rate_hz`; REQ-C-1). **Capability registration** is the
  descriptor's capability list (REQ-P-3), the single path (D-8); there is no runtime registration call.
  **Memory helpers** are not needed (all plugin-to-host data goes through copying sinks, ADR 0019).
  **Terrain queries and event emit** are appended in M7 with the sensor, motion and network kinds.
  Plugins must not use their own randomness or the wall clock (documented; not enforceable in-process).
- **One image per process** *(amended: R-B5)*: `umbra-plugin-host` keeps a process-wide image table.
  Before opening, it looks the canonical library path up; after opening, it looks the address of the
  `umbra_plugin_entry` symbol up, **before calling the entry**. One `LoadedImage` exists per image per
  process: one entry call, at most one `init`, one `shutdown`, shared by `Arc` across hosts and loads.
  A second host (or a later load after an earlier host) reuses the image. Within one host, reaching the
  same image twice (same file under two paths, a symlink) is a `Duplicate` load error found before the
  entry is called (ADR 0022), and the same plugin ID from a *different* image is a `Duplicate` too. Per-host state (log sink, fault state,
  policy, registry) stays per host.
- **Lifecycle mapping (REQ-P-8)**: load = read `plugin.toml`, check it, open the library (ADR 0022, ADR
  0023 load flags) or reuse its image; describe = `umbra_plugin_entry(host)` returns the descriptor, which
  the host copies and validates completely before anything else (ADR 0019 bounds); init = the optional
  plugin-level `init(host, err)`, once per image; configure(params) = capability `configure` per component
  build; tick/call = `instantiate`, `tick_batch`, `halt*`, `set_input`, `output`; shutdown = the optional
  `shutdown()`, once per image, after every behavior and instance handle was destroyed, and *(amended:
  review minor)* only if `init` returned OK (or there is no `init`); a panicking `shutdown` is caught,
  logged and ignored; unload = closing the library afterwards. Every handle keeps its image loaded
  (reference counted), so unload cannot precede the last release, and every copied function pointer is
  gone before the library closes. `destroy_*` is still called for handles of a faulted capability (panics
  swallowed and logged). `HostOptions::keep_loaded` (default false) skips the final close, for platforms
  where closing a library with thread-local destructors is unsafe (macOS risk; glibc delays the unload by
  itself).
- **Fault policy** *(amended: R-M3)*: `on_plugin_fault` in the scenario (ADR 0022), values `abort` (default)
  and `disable_capability`; a core enum `PluginFaultPolicy` shared by the scenario reader and the host. A
  fault is a panic caught by the SDK, a non-OK code, an unset or invalid status, intent or value from the
  plugin, or a token or callback misuse. **Fault state and the policy belong to one engine**: a
  `PluginHost` is created per scenario load and attached to exactly one engine (`Engine::attach_plugins`
  refuses a host that is already attached), and the faulted-capability set and the pending abort live in
  that host. The shared image keeps only a "tainted" record for logging, which never changes what a run
  does, so a run never depends on the history of other runs in the process. On a fault the host logs it
  once (error level, plugin, capability, call, message) and marks the capability faulted for its engine;
  the faulting `tick_batch` call is discarded whole (ADR 0020); from then on that engine skips the
  capability: items return Failure with no intents, `configure` and `instantiate` fail. With `abort`, the
  engine finishes the current tick (deterministic state, including the RNG draws made before the fault),
  then `Engine::run`/`step` return `EngineError::PluginFault`; the CLI exits 1. Faults during load, `init`
  or validation are load or build errors, not runtime faults.
- **Host-side guards**: every host callback is `extern "C"`, catches panics, checks tokens, null and UTF-8,
  and returns an error code; it never unwinds and never calls into a plugin (ADR 0019). Sinks: a second
  `output` sink call in one `output` call is misuse (fault); a second error-sink message is ignored.

## Consequences
- Easier: a small, fully tested v0; behaviors see exactly what built-in behaviors see (LSP); results do not
  depend on what else the process loaded or on earlier runs.
- Harder: plugin authors wanting terrain in behaviors wait for M7; REQ-P-5 is met in stages (recorded in
  the M4 plan and `docs/plugin-guide.md`); the host keeps a process-wide image table and token tables.
- Revisit when M7 adds kinds (terrain, events), if a plugin needs plugin-level configuration from the
  scenario, or if `abort` mid-tick turns out to be required.

## Amended 2026-10-18 after the ABI design review (approved by the human)
R-B4: `host_ctx` is a `u64` token, the host API is a process-lifetime static per image, `log` and `rng_*`
return `UmbraCode` and RNG draws use an out pointer and a per-item, per-call token; R-B5: process-wide image
table keyed by canonical path and entry-symbol address, one entry call, `init` and `shutdown` per image,
shared across hosts; R-M3: fault state and policy per engine (one host per scenario load, attached to one
engine), image-level "tainted" for logging only, whole-batch discard, RNG draws before a fault stay
consumed; minors: `shutdown` only after a successful `init`, `destroy_*` on faulted capabilities, panicking
`shutdown` swallowed, `keep_loaded` option, sink misuse rules.
