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

## 0.2 (M7)
- ADR 0050 (human decisions H1 and H2 of 2026-10-22, H1 extended 2026-10-23; M7 WI-7). Append-only:
  **no ABI 0.0 or 0.1 struct, field, size or constant value changed** (the 20 structs and 150 field pins
  of 0.0 and 0.1 are unchanged; the host's size-per-minor tables have a minor-2 row equal to minor 1 for
  every earlier struct, including `UmbraIntent`, `UmbraValue` and `UmbraNetMessage`). `UmbraHostApi`
  does not grow: terrain and blackboard are per-call services, and events are `CUSTOM` intents
  (H2, ADR 0021 amendment).
- New kinds, supported only in a descriptor declaring minor 2 or newer (with minor 0 or 1 rejected
  naming `capabilities[<i>].kind`): `UMBRA_CAPABILITY_MOTION_MODEL` (2), `UMBRA_CAPABILITY_SENSOR` (4),
  `UMBRA_CAPABILITY_CONDITION` (6), `UMBRA_CAPABILITY_ACTION` (7). `UMBRA_CAPABILITY_COMPONENT` (5) stays
  reserved at every minor (ASM-620). Motion model and sensor metadata must be empty; condition and
  action metadata is `{"blackboard_params": [{"param", "type", "access"}], "result"}` (ASM-643), each
  blackboard param a property of the param schema that admits a string (ASM-709).
- New structs (minor-2 sizes): services `UmbraTerrain` 40 (`ctx`, `height_at`, `normal_at`,
  `line_of_sight`) and `UmbraBlackboard` 32 (`ctx`, `get`, `set`); motion `UmbraKinematicState` 88,
  `UmbraMotionStep` 64, `UmbraMotionVTable` 32 (`configure`, `destroy`, `step`); sensor `UmbraTarget`
  120, `UmbraSenseCall` 64, `UmbraContactSink` 24, `UmbraSensorVTable` 32 (`configure`, `destroy`,
  `sense`); leaves `UmbraLeafCall` 48, `UmbraConditionVTable` 32 (`configure`, `destroy`, `check`),
  `UmbraActionVTable` 56 (`configure`, `destroy_action`, `instantiate`, `destroy_instance`, `tick`,
  `halt`). Every vtable entry is required. The four vtables exist since minor 2: a header declaring
  minor 0 or 1 is rejected. Call records hold their nested structs by pointer (`state`, `observer`,
  `terrain`, `blackboard`), so each can grow on its own (ASM-710). New opaque handles
  `UmbraMotionModel`, `UmbraSensor`, `UmbraCondition`, `UmbraAction`, `UmbraActionInstance`. 80 new
  pinned fields (230 in total), 12 new headed structs (29 in total).
- New constants (no new typedef): `UMBRA_MAX_CONTACTS_PER_SENSE` 4096, `UMBRA_OWNERSHIP_OWNED` 1,
  `UMBRA_OWNERSHIP_REMOTE` 2 (`UmbraTarget::ownership`, a plain `uint32_t`), `UMBRA_CONDITION_UNSET`
  0xFF (the pre-filled `check` result; the plugin writes 0 or 1). Intents of an action's `tick` reuse
  `UmbraIntentSink` and `UMBRA_MAX_INTENTS_PER_ITEM`.
- Host (WI-7): the descriptor validates the schema, the metadata and the vtable of every capability of
  these kinds; only after every capability passed every check, the first one of them ends the load as
  `Unsupported` naming `capabilities[<i>].kind` and the kind, until the host bridges of M7 WI-8 (motion
  model, sensor, terrain service) and WI-9 (condition, action, blackboard service) land (ASM-708).
  `PluginError::Unsupported` gained the field `field` (the capability's index).
- Host (WI-8): motion models and sensors are bridged and registered like built-ins (core `MotionModel`,
  `Sensor`); the terrain service answers from the call's `TerrainProvider` and refuses a stale, foreign
  (another call's, or another service's) or off-thread token; contacts must name a candidate. No layout
  change. Clarified (doc comment only): `UmbraMotionStep.tick` is 0 from this host, because the core
  motion model carries no time, and `rate_hz` is `1 / dt` rounded, at least 1 (ASM-720).
- The C example `c-circle` is compiled against the 0.2 header and declares 0.2 (`plugin.toml`
  `abi = "0.2"`); its behavior structs are unchanged. Behavior-only SDK plugins still declare 0.0 and the
  DIS plugin 0.1 (ADR 0033 rule).

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
- Host: the network adapter's descriptor entries are validated at load, and the capability is registered
  as `ComponentKind::NetworkAdapter` through the host bridge (M6 WI-11, ASM-408). Host-written network
  structs (`UmbraNetBatch`, `UmbraNetMessage`, `UmbraNetSink`) carry the plugin's declared minor in
  `abi_version` and its size; pushed messages are read with the declared minor as the ceiling and only
  the fields their `kind` uses are checked. No layout change.

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
