# ADR 0028: Canonical events v1 and the Events phase (amends ADR 0016)

- **Status**: Accepted by the human 2026-10-19 (proposed by the architect, M5 plan `docs/plans/M5.md`; awaiting the human project owner; amends
  ADR 0016 / D-33 "recorded only until M5")
- **Requirement IDs**: REQ-C-5, REQ-K-2, REQ-K-4, REQ-K-5, REQ-K-6, REQ-B-2, REQ-X-1, REQ-X-6, D-9, D-33
- **Affected**: `umbra-core` (new `event` module), `umbra-kernel` (Events phase, `TickReport::events`,
  `Kernel::remove`), `umbra-engine` (session events), `umbra-api-types` (event DTOs)

## Context
REQ-K-6 snapshots carry "events since the last snapshot", and REQ-X-6 lets stream clients filter by event
kind, but no canonical event type exists yet (REQ-C-5 lists `EntityStateUpdate`, `EntityCreated`,
`EntityRemoved`, `Fire`, `Detonation`, `Collision`, `SimulationControl`, `Custom`). ADR 0016 records
`Fire`, `Spawn`, `Despawn` and `Custom` intents without applying them "until M5" (human decision
2026-10-17). M3 tests that must keep passing unchanged pin that a bare `Kernel` applies neither `Spawn`
nor `Despawn` (`umbra-kernel/tests/behavior_phase.rs`
`fire_spawn_despawn_custom_are_recorded_not_applied`, and several tests read `entities[0]` after a
`Despawn`). M6 needs the same event model for network adapters (D-9).

## Options
1. **Events only in M5 (chosen):** `Fire` and `Custom` intents become canonical events in the kernel
   Events phase (no state change); the engine adds lifecycle events; `Spawn` and `Despawn` intents stay
   recorded and are applied in M7 together with recording and replay. Pros: no hash or M3 test changes;
   M5 gets a real event feed. Cons: behaviors still cannot create or remove entities until M7.
2. **Also apply `Despawn` (and `Spawn`) in M5.** Applying them in the kernel breaks the M3 tests above
   (A-001 needs an ADR with a migration plan); applying them only in the engine behind an opt-in needs a
   second code path for `Engine::run` and a runtime template spawner inside the tick. Larger scope with no
   M5 acceptance criterion behind it.

## Decision
Option 1.
- `umbra_core::event`: `#[non_exhaustive] enum SimEvent { EntityCreated { entity, template: Option<TemplateId> },
  EntityRemoved { entity }, Fire { shooter, target, weapon: WeaponId }, Custom { source: Option<EntityId>,
  kind: ComponentId, payload: ParamValue }, SimulationControl(ControlAction) }` with
  `#[non_exhaustive] enum ControlAction { Start, Pause, Resume, Stop, Step(u64) }`. `Detonation`,
  `Collision` and `EntityStateUpdate` are added when they get a producer (M6/M7); the stream states that
  clients ignore unknown kinds (ADR 0026).
- Kernel Events phase: for each **Owned** entity in `EntityId` order, its `Fire` and `Custom` intents of
  this tick, in emission order, become events; `TickReport` (`#[non_exhaustive]`) gains `events:
  Vec<SimEvent>`. Nothing is hashed (state hash stays v2), no state changes, so every golden hash holds.
- Engine (session, ADR 0024): `EntityCreated` for scenario spawns (in the load's control frame) and API
  spawns, `EntityRemoved` for API removes (`Kernel::remove`, additive: drops the entity and its behavior
  instance; ids are never reused, REQ-C-2), `SimulationControl` for start, pause, resume, stop and step.
- `Spawn` and `Despawn` intents: still recorded only (ADR 0016 text "until M5" becomes "until M7");
  `docs/behavior.md` and `docs/architecture.md` updated.

## Consequences
- Easier: one event model for the stream now and the network layer in M6; no frozen test or golden
  changes.
- Harder: `Spawn`/`Despawn` from behaviors and plugins (the ABI already carries them) still do nothing;
  documented in `docs/behavior.md` and `docs/plugin-guide.md`.
- Revisit in M7 (apply `Spawn`/`Despawn` with replay; M3 tests that pin "not applied" then need this ADR's
  successor with a migration plan) and in M6 (`EntityStateUpdate`, `Detonation` for DIS).
