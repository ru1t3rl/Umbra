# Plugin ABI changelog

The Umbra plugin C ABI (`crates/umbra-plugin-api`, header `crates/umbra-plugin-api/include/umbra_plugin_api.h`)
is versioned `major.minor` and packed as `UMBRA_ABI_VERSION = major << 16 | minor` (ADR 0019). Within a
major, structs and vtables are append-only (REQ-P-2): every appended field or entry is optional with a
documented meaning when absent, and bumps the minor. A kind's minor is the newest minor that changed
any struct on its call surface; a plugin needs a host of at least the minor its descriptor declares,
the highest minor of the kinds it exports (ADR 0033, from 0.1; until 0.1 the SDK wrote its own minor,
ADR 0019). Every entry lists the structs whose size
changed, so the host's size-per-minor table (R-B3) can be checked against it. Rules for plugin authors:
`docs/plugin-guide.md`.

A minor bump changes, in one PR: the Rust types in `umbra-plugin-api`, the host's size-per-minor table,
the pins in `crates/umbra-plugin-api/tests/layout.rs`, the regenerated header (gate 11; the generated C
asserts of `c_header.rs` follow), the SDK's kind minors (`crates/umbra-sdk/src/minor.rs` and its call
surface table), and an entry here. A minor that grows a struct on a kind's call surface bumps that
kind's minor, or the SDK must keep writing the old prefix. A major bump needs an ADR.

## 0.1 (M6)
- ADR 0033 (human decisions H1 and its extension of 2026-10-21; M6 WI-10). Append-only: **no ABI 0.0
  struct, field, size or constant value changed** (all 16 structs and 112 field pins unchanged; the
  host's size-per-minor tables have a minor-1 row equal to minor 0 for every 0.0 struct).
- New kind: `UMBRA_CAPABILITY_NETWORK_ADAPTER` (3), supported only in a descriptor declaring minor 1 or
  newer; with minor 0 it is rejected naming `capabilities[<i>].kind`. Its metadata must be empty.
- New structs (minor-1 sizes): `UmbraNetMessage` 224, `UmbraNetBatch` 40, `UmbraNetSink` 24,
  `UmbraNetworkVTable` 72 (entries `configure`, `destroy`, `key_space`, `start`, `publish`, `poll`,
  `stop` required; `counters` optional). The vtable exists since minor 1: a header declaring minor 0 is
  rejected. New opaque handle `UmbraNetAdapter`. 38 new pinned fields (150 in total).
- New typedefs and constants: `UmbraNetKind` (`UMBRA_NET_ENTITY_STATE` 1 .. `UMBRA_NET_CONTROL` 5),
  `UmbraDeadReckoning` (`UMBRA_DR_STATIC` 1 .. `UMBRA_DR_BODY_ACCELERATION` 9, ADR 0032 order),
  `UmbraNetControl` (`UMBRA_NET_CONTROL_START` 1 .. `UMBRA_NET_CONTROL_STOP` 4); `0` is invalid for
  each. Caps `UMBRA_MAX_NET_POLL` 4096, `UMBRA_MAX_NET_COUNTERS` 8.
- `configure` input of a network adapter: the ADR 0030 wrapper `{"name", "params", "templates"}`; the
  param schema applies to `params`, errors point at `/params/<key>` (ASM-401).
- **Required-minor rule** (amends ADR 0019): a descriptor declares the lowest minor that defines every
  capability kind it exports (behavior 0, network adapter 1); a capability and its vtable declare the
  minor of their kind, and so do the structs written during the capability's calls (intents, values;
  ADR 0033 clarified 2026-10-21, ASM-406). Behavior-only SDK plugins (`example-wander`,
  `umbra-test-plugin`) declare and write 0.0 throughout, so they run in an ABI 0.0 host. The C example
  `c-circle` is compiled against the 0.1 header and declares 0.1 (`plugin.toml` `abi = "0.1"`).
- Host ceiling: a nested descriptor header (capability, vtable) declaring a minor above the
  descriptor's is a descriptor error at `<prefix>.abi_version`; an intent or value declaring a minor
  above the plugin's declared minor faults the capability (`intent.abi_version`, `value.abi_version`).
- Host: the network adapter's descriptor entries are validated at load; the host bridge follows in M6
  WI-11 (until then such a capability ends the load as unsupported).

## 0.0 (M4, frozen at `m4-done`)
- First ABI. Layouts: the table in `docs/plans/M4.md` ("Layout table"), pinned by
  `crates/umbra-plugin-api/tests/layout.rs` (64-bit only, ASM-120). Minor-0 sizes: `UmbraErrorSink` 24,
  `UmbraHostApi` 40, `UmbraPluginDescriptor` 88, `UmbraCapability` 88, `UmbraBehaviorVTable` 80,
  `UmbraBehaviorBatch` 56, `UmbraBehaviorItem` 40, `UmbraEntityView` 160, `UmbraContact` 88,
  `UmbraIntent` 88, `UmbraIntentSink` 24, `UmbraValue` 80, `UmbraValueSink` 24; frozen primitives
  `UmbraStr` 16, `UmbraVec3` 24, `UmbraQuat` 32.
- Kinds: `UMBRA_CAPABILITY_BEHAVIOR` (1) supported; 2-7 reserved (motion model, network adapter, sensor,
  component, condition, action) and rejected as unsupported.
- Host API v0: `log`, `rng_next_u64`, `rng_next_f64` with opaque tokens (ADR 0021).
- Header: declares the entry `UMBRA_EXPORT const UmbraPluginDescriptor *umbra_plugin_entry(const
  UmbraHostApi *host);` (added after the WI-1..WI-4 review, still within the provisional period), so a C
  plugin's definition is checked against the entry type `UmbraPluginEntry`. No struct layout changed.
- **Provisional** (ADR 0019 R-M10, ASM-117): M4 WI-3..WI-14 may still change this layout, updating the
  plan table, the pins, the header and this entry together. The field-by-field freeze review is part of
  M4 WI-16; after `m4-done` the rules of REQ-P-2 apply in full.
- Constraints for later minors (M4 WI-15, from ASM-134 a, ASM-128, ASM-148 b2): `UmbraHostApi` is passed
  to the entry before the host knows the plugin's minor, so it is written in the host's own (newest)
  layout and plugins of an older minor read their prefix (reader rule); appended callbacks (M7) are
  invisible to older plugins (amended 2026-10-19 at the WI-16 freeze review). Readers of host-written array elements (`items`, `contacts`)
  require `struct_size` equal to their own size (the stride they index with), so the host must keep
  writing array elements in the plugin's declared minor.
- **Layout final** (M4 WI-14, 2026-10-19): the C example `examples/plugins/c-circle` compiles against the
  header with `-std=c11 -pedantic -Wall -Wextra -Werror`, loads and drives entities; no struct, field,
  constant or header text changed for it. ABI 0.0 layout final, pending the WI-16 freeze review.
- **Frozen** (M4 WI-16, 2026-10-19, at `5f41ea1`): field-by-field freeze review against ADR 0019 signed off
  ("0.0 freeze: approved"; 16 structs recomputed for x86_64, aarch64 and Win64, no implicit padding,
  112 fields pinned by `layout.rs` and the generated C asserts). The only change from the review was
  comment-level (`UmbraHostApi` is written in the host's own layout, ASM-134 a). From `m4-done` the rules
  of REQ-P-2 apply in full: within major 0, append only, with a minor bump and an entry here.
