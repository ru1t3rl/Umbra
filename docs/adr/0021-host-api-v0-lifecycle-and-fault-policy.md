# ADR 0021: Host API v0, plugin lifecycle and fault policy

- **Status**: Accepted (human project owner, 2026-10-18; proposed by the architect in the M4 plan `docs/plans/M4.md`)
- **Requirement IDs**: REQ-P-5, REQ-P-7, REQ-P-8, REQ-P-11, REQ-K-4, REQ-C-1, REQ-X-3, D-8, ADR 0006
- **Affected**: `umbra-plugin-api`, `umbra-plugin-host`, `umbra-sdk`, `umbra-engine`, `umbra-core` (one enum)

## Context
REQ-P-5 lists what the host API provides: logging, `SimRng` access, current `SimTime`, terrain queries,
event emit, memory helpers, capability registration. M4 has one kind (behavior), whose built-in peers get
only time and the entity's RNG stream (`BehaviorContext`, ADR 0013), and emit intents, not events.
REQ-P-8 gives the lifecycle `load -> describe -> configure(params) -> init -> tick/call* -> shutdown ->
unload`. REQ-P-7 names the policy `on_plugin_fault` (`abort` | `disable_capability`) but not its timing
or default. Every host API entry is append-only once shipped.

## Options
1. **Full REQ-P-5 list now.** Cons: terrain and events have no M4 consumer (behaviors get neither; giving
   them terrain would be a new behavior input); untestable surface frozen early.
2. **Minimal v0, append the rest with the kinds that use them (chosen).** Appending to `UmbraHostApi` is a
   minor bump (ADR 0019).

## Decision
Option 2.
- **`UmbraHostApi` v0**: header, `host_ctx` (opaque, one per plugin, so logs are attributed),
  `log(host_ctx, level, message)` (to `tracing`, target `umbra_plugin`, field `plugin = <id>`),
  `rng_next_u64(rng)` and `rng_next_f64(rng)` (in [0, 1)) on the `UmbraRng` handle of a batch item, which
  is the entity's own hashed stream (ADR 0006), so plugin draws are covered by the state hash (REQ-K-4).
  **Time** is passed in every tick call (`tick`, `rate_hz`; REQ-C-1). **Capability registration** is the
  descriptor's capability list (REQ-P-3), the single path (D-8); there is no runtime registration call.
  **Memory helpers** are not needed (all plugin-to-host data goes through copying sinks, ADR 0019).
  **Terrain queries and event emit** are appended in M7 with the sensor, motion and network kinds.
  Plugins must not use their own randomness or the wall clock (documented; not enforceable in-process).
- **Lifecycle mapping (REQ-P-8)**: load = read `plugin.toml`, check it, open the library (ADR 0022);
  describe = `umbra_plugin_entry(host)` returns the descriptor, which the host validates completely before
  anything else; init = the optional plugin-level `init(host, err)`, once; configure(params) = capability
  `configure` per component build; tick/call = `instantiate`, `tick_batch`, `halt*`, `set_input`,
  `output`; shutdown = the optional `shutdown()`, once, after every behavior and instance handle was
  destroyed; unload = closing the library afterwards. Every handle the host gives out keeps its plugin
  loaded (reference counted), so unload cannot precede the last release.
- **Fault policy**: `on_plugin_fault` in the scenario (ADR 0022), values `abort` (default) and
  `disable_capability`; a core enum `PluginFaultPolicy` shared by the scenario reader and the host. A fault
  is a panic caught by the SDK, a non-OK code, an invalid status, intent or value from the plugin, or a
  callback misuse. On a fault the host logs it once (error level, plugin, capability, call, message) and
  marks the capability faulted; from then on its calls are skipped: items return Failure with no intents,
  `configure` and `instantiate` fail. With `abort`, the engine finishes the current tick (deterministic
  state), then `Engine::run`/`step` return `EngineError::PluginFault`; the CLI exits 1. Faults during load,
  `init` or validation are load or build errors, not runtime faults.
- **Host-side guards**: every host callback is `extern "C"`, catches panics, checks null and UTF-8, and
  returns an error code; it never unwinds.

## Consequences
- Easier: a small, fully tested v0; behaviors see exactly what built-in behaviors see (LSP).
- Harder: plugin authors wanting terrain in behaviors wait for M7; REQ-P-5 is met in stages (recorded in
  the M4 plan and `docs/plugin-guide.md`).
- Revisit when M7 adds kinds (terrain, events), if a plugin needs plugin-level configuration from the
  scenario, or if `abort` mid-tick turns out to be required.
