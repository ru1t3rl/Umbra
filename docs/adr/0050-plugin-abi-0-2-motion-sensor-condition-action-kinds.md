# ADR 0050: Plugin ABI 0.2: motion model, sensor, condition and action kinds with per-call services

- **Status**: Proposed (architect, M7 plan `docs/plans/M7.md`, 2026-10-22). Changes a frozen ABI by an
  append-only minor bump, refines how ADR 0021 / D-39 deliver "terrain queries and event emit", and needs
  literal edits to frozen M4/M6 tests: human decisions H1 and H2 of the M7 plan.
- **Requirement IDs**: REQ-P-2..9, REQ-P-11, REQ-SDK-1..3, REQ-M-1, REQ-M-4, REQ-B-3, REQ-B-15, REQ-K-4,
  REQ-K-5, REQ-C-6, D-6, D-8, D-37, D-38, D-39, D-52, ADR 0019, ADR 0020, ADR 0021, ADR 0033
- **Affected**: `umbra-plugin-api` (+ generated header), `umbra-plugin-host` (descriptor, four bridges,
  size tables), `umbra-sdk` (four traits, `export_plugin!`, minor table), `umbra-test-plugin`,
  `docs/plugin-abi-changelog.md`, `docs/plugin-guide.md`

## Context
REQ-P-4 lists `motion_model`, `sensor`, `condition` and `action` as plugin kinds; ABI 0.1 reserves kinds
2, 4, 6, 7 and rejects them. The core traits already exist and are registry kinds (`MotionModel`,
`Sensor`, `Condition`, `Action`; ADR 0011, ADR 0015), so no registry or kernel change is needed (D-8).
Motion models and sensors get a `TerrainProvider` in core (`MotionModel::step`, `SenseContext::terrain`);
conditions and actions get a `Blackboard` restricted by their declared `BlackboardParam`s. A plugin must
see exactly what a built-in sees (LSP, ADR 0021). ADR 0021 planned "terrain queries and event emit
appended to `UmbraHostApi` in M7". Appending to `UmbraHostApi` changes the size of an existing ABI 0.0
struct (its layout pin and the generated C asserts), and an event-emit callback would be a second path
next to the `CUSTOM` intent, which already becomes a `SimEvent::Custom` (ADR 0028).

## Options
1. **Four new kinds at minor 2, new structs only; terrain and blackboard as per-call service structs
   (token + function pointers, the sink pattern of ADR 0019); event emit through the existing `CUSTOM`
   intent (chosen).** Pros: no existing struct changes size, so every 0.0 and 0.1 layout pin holds; a
   service exists only in the calls that have it (ISP, LSP); tokens stay call-scoped (REQ-P-11); one event
   path (DRY). Cons: refines the mechanism ADR 0021 named.
2. **Same kinds; append `terrain_*`, `bb_get`, `bb_set`, `emit_event` to `UmbraHostApi`.** Pros: literal
   ADR 0021. Cons: `UmbraHostApi` grows (pins change), callbacks reachable outside calls that make sense,
   two event paths, sensors and motion would need kernel event plumbing (core traits have no event sink).
3. **Batched vtables (`step_batch`, `sense_batch`) now.** Pros: fewer FFI calls. Cons: new core batch hooks
   and kernel Motion/Perception paths (M1 code) with no measured need; motion at 10,000 entities is about
   10,000 calls per tick (well under 1 ms). Deferred, additive later.

## Decision
Option 1, ABI **0.2** (`UMBRA_ABI_VERSION_MINOR = 2`); exact field order by the plugin-abi-engineer under
ADR 0019 rules (no implicit padding, sizes multiples of 8), pinned in `layout.rs` and the C asserts.
- **Kinds** (descriptor minor >= 2): `UMBRA_CAPABILITY_MOTION_MODEL` 2, `..._SENSOR` 4, `..._CONDITION` 6,
  `..._ACTION` 7. Kind 5 (`component`) stays reserved (no defined semantics, ASM-620).
- **New structs:** `UmbraKinematicState` (header, position, velocity, orientation), `UmbraTerrain` (header,
  `ctx` token, `height_at(ctx, const UmbraVec3* ecef, double* out_m)`, `normal_at(ctx, const UmbraVec3*,
  UmbraVec3* out_enu)`, `line_of_sight(ctx, const UmbraVec3* from, const UmbraVec3* to, uint8_t* out)`),
  `UmbraBlackboard` (header, `ctx`, `get(ctx, UmbraStr key, const UmbraValueSink*)`,
  `set(ctx, UmbraStr key, const UmbraValue* or null to unset)`), `UmbraMotionStep` (header, tick,
  rate_hz, dt, `state`, intents array + count (host-written `UmbraIntent`s in the plugin's minor),
  `const UmbraTerrain*`), `UmbraTarget` (header, id, position, velocity, orientation, force, ownership
  `u32` 1 owned / 2 remote), `UmbraSenseCall` (header, tick, rate_hz, observer `UmbraTarget`, candidates
  array + count (all other entities, id order, pre-tick), rng token, `const UmbraTerrain*`),
  `UmbraContactSink` (header, ctx, `push(ctx, const UmbraContact*)`, copying), `UmbraLeafCall` (header,
  tick, rate_hz, `const UmbraEntityView*`, rng token, `const UmbraBlackboard*`), and the vtables
  `UmbraMotionVTable` (`configure`, `destroy`, `step(model, const UmbraMotionStep*, UmbraKinematicState*
  out, err)`; `out` pre-filled with the input state), `UmbraSensorVTable` (`configure`, `destroy`,
  `sense(sensor, const UmbraSenseCall*, const UmbraContactSink*, err)`), `UmbraConditionVTable`
  (`configure`, `destroy`, `check(cond, const UmbraLeafCall*, uint8_t* out, err)`; `out` pre-filled
  `0xFF` = unset), `UmbraActionVTable` (`configure`, `destroy_action`, `instantiate(action, rate_hz, out,
  err)`, `destroy_instance`, `tick(instance, const UmbraLeafCall*, const UmbraIntentSink*, UmbraStatus* out,
  err)`, `halt(instance)`). Opaque handles `UmbraMotionModel`, `UmbraSensor`, `UmbraCondition`,
  `UmbraAction`, `UmbraActionInstance`.
- **Services:** every service token is valid only during the call that carries it (resolved against the
  call record, ADR 0021 RR-M2); a stale or foreign token faults the capability. Terrain answers from the
  `TerrainProvider` the core call received. The blackboard grants exactly the keys named by the leaf's
  declared blackboard params (resolved from its params at configure), with their `Access`; any other key,
  a wrong type or a non-finite value is refused and faults. RNG uses the existing `rng_next_*` with the
  call's rng token (entity stream, REQ-K-4). Events: plugins emit `CUSTOM` intents (actions); sensors,
  motion models and conditions emit nothing, like their core traits.
- **Metadata:** conditions and actions: `{"blackboard_params": [{"param", "type", "access"}], "result"}`
  mapped to `ComponentInfo` with `Provenance::Plugin`; motion models and sensors: empty.
- **Validation of plugin output:** motion `out` must be finite with a unit quaternion within 1e-9 (the
  core contract tolerance); contacts must name a candidate id (sorted and deduplicated by the kernel as
  today); a condition must write 0 or 1; an action status as ADR 0020. Anything else faults.
- **Faults** (ADR 0021 policy, per engine): motion: the entity keeps position and orientation with zero
  velocity (ASM-614); sensor: no contacts from it; condition: false; action: Failure, its intents of the
  call discarded. Under `abort` the run ends after the tick. `configure`/`instantiate` failures are build
  errors as for behaviors.
- **Not batched** (ASM-612): one call per entity (motion), per observer and sensor (sense), per node tick
  (condition, action), as ADR 0020 already planned for leaves.
- **SDK:** traits `MotionModel`, `Sensor`, `Condition`, `Action` + `ActionInstance` with safe context
  types (`Terrain`, `BlackboardAccess`, `Rng`), owned or borrowed mirrors only (SDK depends only on
  `umbra-plugin-api`, REQ-A-3); `export_plugin!` lists `motion_models`, `sensors`, `conditions`, `actions`;
  required minor per kind (ADR 0033 rule): behavior 0, network adapter 1, the four new kinds 2.
- **Frozen test edits (human decision H1):** the "newer minor" literals (`manifest.rs` `"0.2"`,
  `tests/descriptor.rs` `2`, `src/descriptor.rs` `(2, own32)`), `layout.rs::abi_version_packs_major_and_minor`
  and the count literals of `layout.rs` and `c_header.rs`, and `examples/plugins/c-circle/plugin.toml`
  (+ README) `abi = "0.2"`. Each literal is rewritten relative to `UMBRA_ABI_VERSION_MINOR` (or a count
  derived from the support table) so later minors need no edit. No 0.0 or 0.1 size or offset changes.

## Consequences
- Easier: every REQ-P-4 kind except `component` loads through the one loader and registry; examples prove
  the SDK; `UmbraHostApi` stays log + RNG and needs no reader-prefix care.
- Harder: four bridges and their failure rows (gate 13 grows); per-call marshalling of sensor candidates is
  O(n) per observer (O(n^2) per tick, like `builtin.visual`), measured in M9.
- ADR 0021's "appended to the host API" is replaced by "provided per call"; D-39 gains a note.
- Revisit if profiling asks for batching (additive minor), for a terrain plugin kind (not in REQ-P-4), or
  for an out-of-process host (nothing here assumes shared memory beyond a call).
