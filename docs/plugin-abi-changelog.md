# Plugin ABI changelog

The Umbra plugin C ABI (`crates/umbra-plugin-api`, header `crates/umbra-plugin-api/include/umbra_plugin_api.h`)
is versioned `major.minor` and packed as `UMBRA_ABI_VERSION = major << 16 | minor` (ADR 0019). Within a
major, structs and vtables are append-only (REQ-P-2): every appended field or entry is optional with a
documented meaning when absent, and bumps the minor. A plugin built against a newer minor needs a host of
at least that minor (the SDK writes its own minor, ADR 0019). Every entry lists the structs whose size
changed, so the host's size-per-minor table (R-B3) can be checked against it.

## 0.0 (M4, provisional)
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
- **Layout final** (M4 WI-14, 2026-10-19): the C example `examples/plugins/c-circle` compiles against the
  header with `-std=c11 -pedantic -Wall -Wextra -Werror`, loads and drives entities; no struct, field,
  constant or header text changed for it. ABI 0.0 layout final, pending the WI-16 freeze review.
