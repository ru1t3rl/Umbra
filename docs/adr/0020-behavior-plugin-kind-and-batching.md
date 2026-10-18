# ADR 0020: The behavior plugin kind and batched ticks

- **Status**: Accepted (human project owner, 2026-10-18; proposed by the architect in the M4 plan `docs/plans/M4.md`)
- **Requirement IDs**: REQ-P-3, REQ-P-4, REQ-P-8, REQ-B-1, REQ-B-3 (layer 2), REQ-B-4, REQ-B-5, REQ-B-9,
  REQ-B-10, REQ-B-12, REQ-B-13, REQ-K-4, REQ-K-5, REQ-T-1, REQ-T-3, D-8, D-30 (ADR 0013 amendment), ASM-086,
  ASM-096, ASM-099 (e)
- **Affected**: `umbra-plugin-api`, `umbra-plugin-host`, `umbra-sdk`, `umbra-core` (additive, M0-frozen),
  `umbra-kernel` (Behavior phase), `docs/behavior.md`

## Context
M4 delivers one kind, `behavior`. A plugin behavior must be usable wherever a registered behavior is:
as a template `[behavior] ref` (REQ-T-1) and as a `subgraph` target with bound inputs and outputs
(REQ-B-9/10, ADR 0014), indistinguishable from file and built-in targets. REQ-B-4 asks for batched
calls. ADR 0013 (amendment 2026-10-18) requires the ABI to mirror `BehaviorInstance::halt_running` or
document the fallback, and ASM-096 (c) requires `halt` to be idempotent. The kernel visits entities one
at a time and owns `Box<dyn BehaviorInstance>` per entity; it must stay plugin-agnostic (D-9 spirit, DIP).

## Options
1. **Per-entity FFI call.** The bridge implements `BehaviorInstance::tick` with one call. Pros: no kernel
   change. Cons: violates REQ-B-4; an out-of-process host would pay a round trip per entity.
2. **Batched ABI entry plus an additive core hook the kernel uses to group entities (chosen).**
3. **Batch inside the bridge (defer results to a later call).** Cons: `tick` must return a status now;
   breaks ADR 0013 timing. Rejected.

## Decision
Option 2.
- **Vtable** `UmbraBehaviorVTable` (header first): `configure(params_json, out *UmbraBehavior, err)`
  (= `Factory::build`), `destroy_behavior`, `instantiate(behavior, rate_hz, out *UmbraInstance, err)`
  (= `Behavior::instantiate`, REQ-B-5 create), `destroy_instance`, `tick_batch(batch, err)`,
  `halt(instance)`, and three optional (nullable) entries: `halt_running(instance)`, `set_input(instance,
  param, value_or_null, err)`, `output(instance, port, sink)`.
- **halt**: the instance halt; MUST be idempotent and callable when the instance is not Running (ASM-096 c).
  **halt_running** mirrors the core method: when null the host calls `halt` (the full reset, ADR 0013).
  The SDK always fills it (its trait method defaults to `halt`).
- **Batch** `UmbraBehaviorBatch {tick, rate_hz, items, count, intents sink}`, item `UmbraBehaviorItem
  {behavior, instance, entity view, rng handle, status (out)}`. Items of one call all belong to one
  capability; they may come from different configured behaviors. The plugin writes every item's status;
  an unwritten or unknown status is a fault. Intents go to the sink with the item index.
- **Core hook (additive, `umbra-core`)**: `Behavior::batch(&self) -> Option<Arc<dyn BehaviorBatch>>`
  (default `None`), `BehaviorBatch::tick_batch(&self, items: &mut [BatchItem<'_>])`, `BatchItem` (context,
  view, `&mut dyn BehaviorInstance`, intent buffer, status), `BehaviorInstance::as_any_mut` (default
  `None`) so a bridge recognises its own instances, and a reference `tick_each` with the contract:
  `tick_batch` gives every item exactly what `instance.tick` would.
- **Kernel**: in the Behavior phase, Owned entities whose top-level behavior returns a batch are collected
  (id order) into groups; groups are identified by `Arc::ptr_eq` and ordered by their first entity id
  (never by address). Unbatched entities are ticked in id order as today, then each group is ticked once,
  then the per-entity post-processing (`FollowRoute` expansion, last status) runs in id order. Because each
  entity has its own intent buffer, RNG stream and pre-tick view (ADR 0013 amendment), results equal
  id-order ticking. Behaviors without a batch take the M3 path unchanged, so M2/M3 hashes cannot move.
- **Not batched**: a plugin behavior used as a `subgraph` child is ticked through `BehaviorInstance::tick`
  (a batch of one), like plugin conditions and actions in M7 (ASM-086 c).
- **Metadata**: the capability's `metadata` JSON for behaviors is `{"param_types": {name: bbtype},
  "outputs": {port: bbtype}, "result": text}` (all optional), mapped to `ComponentInfo` with
  `Provenance::Plugin`. `param_types` must agree with the schema (literal types match their property's
  `type`; `entity`/`position` are binding-only and absent from the schema, ADR 0014); `set_input` must be
  present iff `param_types` is non-empty, `output` iff `outputs` is. There is no `subgraphs` key in v0, so
  a plugin behavior counts one level of subgraph depth (ASM-099 e). Description is the schema's top-level
  `description` (ASM-100 g rule); version is the capability `version`, else the plugin version.
- **Faults**: any error code, panic, unknown status or invalid intent marks the capability faulted
  (ADR 0021); a faulted item returns Failure and emits nothing.

## Consequences
- Easier: one FFI call per capability per tick for top-level plugin behaviors; plugin targets behave like
  any registered behavior in graphs, templates, catalog and validation (D-8).
- Harder: kernel borrow plumbing to hold many entities' buffers at once; a second Behavior-phase path that
  must stay equivalent (contract and determinism tests).
- Revisit with M7 (sensor `sense_batch`, conditions, actions), replay (save/restore instance state, REQ-B-5),
  or if profiling shows the grouping costs more than it saves (NFR-1, M9).
