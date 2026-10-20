# ADR 0033: Network adapter plugin kind (ABI 0.1) and the SDK's required minor (amends ADR 0019)

- **Status**: Accepted by the human 2026-10-20 (proposed by the architect, M6 plan `docs/plans/M6.md`, 2026-10-20).
  Changes a frozen ABI (append-only minor bump), amends one rule of accepted ADR 0019 (D-37) and needs
  edits to M4 tests that pin "minor 1 is newer than the host": all three are human decisions.
- **Requirement IDs**: REQ-P-2, REQ-P-3, REQ-P-4, REQ-P-6, REQ-P-7, REQ-P-8, REQ-P-9, REQ-P-11, REQ-SDK-1,
  REQ-SDK-2, REQ-SDK-3, REQ-N-1, REQ-K-4, D-6, D-8, D-37, D-39
- **Affected**: `umbra-plugin-api`, generated header, `umbra-plugin-host` (size tables, network bridge,
  fault handling), `umbra-sdk` (`NetworkAdapter` trait, `export_plugin!`), `umbra-test-plugin`,
  `docs/plugin-abi-changelog.md`, `docs/plugin-guide.md`

## Context
M6 must ship the DIS adapter "as plugin", built with the public SDK (REQ-SDK-3, D-9). ABI 0.0 reserves
`UMBRA_CAPABILITY_NETWORK_ADAPTER = 3` "for M6" and rejects it as unsupported. Every new kind, struct or
vtable entry bumps the minor (REQ-P-2, changelog rules). ADR 0019 says "the SDK writes its own minor", so
after the bump every SDK plugin, including the frozen M4 examples, would declare 0.1; `example-wander`'s
`plugin.toml` (`abi = "0.0"`, checked for equality before opening) and the M4 tests that assert `"0.0"`
(`cli_list_plugins.rs`, `xtask/tests/build_plugins.rs`) would then fail. The C example uses the header's
`UMBRA_ABI_VERSION`, which becomes 0.1. Three M4 tests use "minor 1" as "newer than the host"
(`umbra-plugin-host/tests/manifest.rs` `given_a_manifest_with_a_newer_abi_minor_then_rejected_without_opening`,
`tests/descriptor.rs` `descriptor_abi_minor_is_newer`, the unit test `header_checks_major_minor_and_size`
in `src/descriptor.rs`).

## Options
1. **ABI 0.1 with new, separate structs for the kind; existing layouts untouched; the SDK declares the
   lowest minor that has every kind the plugin exports (chosen).** Behavior-only SDK plugins stay 0.0;
   the C example's manifest says 0.1 (it is compiled against the 0.1 header); the three "newer minor"
   tests use `host minor + 1`.
2. **Same, but the SDK keeps writing its own minor.** Edits `examples/plugins/wander/plugin.toml`,
   `cli_list_plugins.rs` and `build_plugins.rs` as well, and makes every rebuilt plugin need a 0.1 host for
   no reason.
3. **Ship DIS statically linked, not as a plugin.** Violates M6's deliverable and REQ-SDK-3. Rejected.

## Decision
Option 1 (ABI **0.1**; final layout by the plugin-abi-engineer under ADR 0019 rules, pinned in
`layout.rs`, generated C asserts, changelog entry).
- **Constants:** `UMBRA_ABI_VERSION_MINOR = 1`; kind 3 supported for descriptors declaring minor >= 1.
  `UmbraNetKind` (`UMBRA_NET_ENTITY_STATE` 1, `..._ENTITY_REMOVED` 2, `..._FIRE` 3, `..._DETONATION` 4,
  `..._CONTROL` 5), `UmbraDeadReckoning` (1..9 in the order of ADR 0032's models), `UmbraNetControl`
  (`START` 1, `PAUSE` 2, `RESUME` 3, `STOP` 4); `0` is invalid for each. Caps `UMBRA_MAX_NET_POLL` (4096
  messages per poll) and `UMBRA_MAX_NET_COUNTERS` (8).
- **Structs (new; nothing existing changes):**
  - `UmbraNetMessage` (header; `kind`, `dead_reckoning`, `key`, `other_key` (target), `position`,
    `velocity`, `acceleration`, `angular_velocity`, `orientation` (`UmbraVec3` = yaw, pitch, roll,
    ADR 0032), `entity_type`, `force`, `name`, `weapon` (`UmbraStr`), `control`, named reserved fields).
    Written by the host (publish) or the plugin (poll, copied by the sink).
  - `UmbraNetBatch` (header; `tick`, `rate_hz`, `messages` array with the plugin's stride, `count`).
  - `UmbraNetSink` (header; `ctx` token; `push(ctx, const UmbraNetMessage*) -> UmbraCode`), copying.
  - `UmbraNetworkVTable` (header; `configure(params, out **UmbraNetAdapter, err)`, `destroy(adapter)`,
    `key_space(adapter, *u64 base, *u64 count)`, `start(adapter, tick, rate_hz, err)`,
    `publish(adapter, const UmbraNetBatch*, err)`, `poll(adapter, tick, max, const UmbraNetSink*, err)`,
    `stop(adapter)`; optional `counters(adapter, *u64 out, size_t n)` (tx dropped, rx dropped, decode
    errors, I/O errors; `0` when absent). `UmbraNetAdapter` is an opaque plugin-owned handle.
  - `configure` params JSON: `{"name", "params", "templates"}` (ADR 0030). No I/O before `start`.
- **Threads (REQ-N-1, REQ-P-11):** the adapter's I/O threads belong to the plugin. They never call the
  host API (it returns `UMBRA_ERR_STATE` off a call record anyway, ADR 0021); `stop` and `destroy` join
  every thread before returning, so no plugin code runs after `destroy` and the image can unload
  (lifecycle REQ-P-8: `configure -> start -> publish/poll* -> stop -> destroy -> shutdown`). `publish`
  and `poll` never block on I/O: they move data to or from bounded queues (drop-and-count).
- **Faults (ADR 0021):** a panic, a non-OK code, an invalid message or a token misuse faults the
  capability under the scenario's `on_plugin_fault`: `abort` ends the run after the tick (as for
  behaviors); `disable_capability` disables that adapter in the manager (no more calls except `stop` and
  `destroy`), its ghosts time out. Messages of a faulted `poll` are discarded whole.
- **Host bridge:** `PluginNetworkConfig` (implements `umbra_core::net::NetworkAdapterConfig`, registered
  as `ComponentKind::NetworkAdapter` through the existing descriptor path, D-8) and `PluginNetworkAdapter`;
  host-side conversion `Quaternion <-> WorldEuler` happens in `umbra-net`/`umbra-core`, never in the
  plugin; strings are validated UTF-8 with caps; `NetEntityType` tokens are passed through unchanged.
- **SDK:** trait `NetworkAdapter` (+ `NetworkAdapterFactory` for configure), owned message mirror types
  (the SDK still depends only on `umbra-plugin-api`, REQ-A-3), `export_plugin!` support, and
  **amended ADR 0019 rule:** the descriptor declares the lowest minor that defines every capability kind
  it exports (behavior 0, network adapter 1), and each vtable header carries the minor of its own layout.
  A plugin built with a newer SDK no longer needs a newer host unless it uses a newer kind.
- **M4 test edits (human decision):** the three "newer minor" tests change their literal minor 1 to
  `UMBRA_ABI_VERSION_MINOR + 1` (manifest text `"0.2"`), and `examples/plugins/c-circle/plugin.toml`
  (and its README line) change `abi = "0.0"` to `"0.1"`. No struct, field or behavior of ABI 0.0 changes;
  goldens `88954c5dc6b52b48` and `3cc077a4c83c4a63` are unaffected.

## Consequences
- Easier: network adapters are ordinary plugins (one loader, one registry, one fault policy); DIS proves
  the SDK is sufficient; M7 adds its kinds the same way (0.2, ...).
- Harder: plugin-owned threads are a new contract to test (join on `stop`/`destroy`, unload after
  threads ended); the failure matrix grows by the network rows.
- Revisit in M7 (host API additions: terrain, event emit) and if out-of-process hosts arrive (REQ-P-11:
  nothing here assumes a shared address space beyond the call).
