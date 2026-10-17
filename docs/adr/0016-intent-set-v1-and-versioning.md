# ADR 0016: Intent set v1 and its versioning

- **Status**: Accepted (decided by the human project owner on 2026-10-17; `Fire`, `Spawn`, `Despawn`, `Custom` recorded only until M5; M3 plan `docs/plans/M3.md`). Spec decision D-33.
- **Requirement IDs**: REQ-B-1, REQ-B-2, REQ-M-1, REQ-K-2, REQ-K-5, REQ-P-2, REQ-P-6, ASM-029, ASM-030, ASM-048
- **Affected**: `cgf-core::intent`, `cgf-kernel` (route expansion, snapshot), `cgf-motion` (`SetHeading`), M4 ABI

## Context
REQ-B-2 lists a closed, versioned set: `MoveTo`, `FollowRoute`, `SetSpeed`, `SetHeading`, `Hold`,
`Fire{target, weapon}`, `Spawn`, `Despawn`, `Custom{kind,payload}`. M1 shipped `MoveTo`, `SetSpeed`, `Hold`
(`#[non_exhaustive]`). Existing variants are frozen. Fire resolution, in-tick spawning and despawning have
no designed consumer yet (events, damage and the spawn path come later).

## Options
1. **Define every variant now; consume what M3 needs; record the rest in the snapshot.**
2. **Add only variants with a consumer.** Cons: the M4 ABI would be designed against a moving set.

## Decision
Option 1. `pub const INTENT_SET_VERSION: u32 = 1;` Variants added (existing ones unchanged):
- `FollowRoute` (no fields): the kernel route system replaces it, in place, with the M1 stand-in output
  for that entity's initial route (`SetSpeed` + `MoveTo`, or `Hold` at the end); no route: dropped.
- `SetHeading(Radians)`: true heading; the ground model steers to it when no `MoveTo` follows it.
- `Fire { target: EntityId, weapon: WeaponId }`, `Spawn { template: TemplateId, position: Ecef,
  heading: Radians }`, `Despawn`, `Custom { kind: ComponentId, payload: ParamValue }`: emitted and visible
  in `EntitySnapshot::intents`, not applied in M3 (no state change, not hashed).
- `WeaponId` is a new ADR 0002 string ID. Entities without a behavior keep the M1 stand-in exactly.
- Versioning: adding a variant increments `INTENT_SET_VERSION`; changing a variant is not allowed within an
  ABI major. Consumers ignore variants they do not handle (ASM-030).

## Consequences
- Easier: M4 can mirror a complete set; `follow_route` reuses the proven M1 route code (DRY).
- Harder: `Fire`, `Spawn`, `Despawn` look effective but are not; documented in `docs/behavior.md` and in
  the M3 open questions.
- Revisit when Events-phase resolution (`Fire` -> `Fire` event, REQ-C-5) and the spawn path (M5) land.
