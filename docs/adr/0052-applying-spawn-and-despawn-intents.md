# ADR 0052: Applying `Spawn` and `Despawn` intents at the engine tick boundary (successor of ADR 0028 decision 5)

- **Status**: Accepted by the human 2026-10-22 as proposed (H3; proposed by the architect, M7 plan
  `docs/plans/M7.md`, 2026-10-22). Completes D-46 / ADR 0028 ("applied in M7 with recording and replay")
  and amends ADR 0016's note.
- **Requirement IDs**: REQ-B-2, REQ-B-1, REQ-K-2, REQ-K-4, REQ-K-5, REQ-C-2, REQ-C-5, REQ-N-2, REQ-K-7,
  D-16, D-33, D-46, ADR 0016, ADR 0024, ADR 0028, ADR 0030
- **Affected**: `umbra-kernel` (additive `TickReport::lifecycle`), `umbra-engine` (spawner, `tick_once`,
  `Session` directory), `docs/behavior.md`, `docs/architecture.md`, `docs/plugin-guide.md`

## Context
Behaviors and plugins can emit `Spawn { template, position, heading }` and `Despawn`; until now they are
recorded only. Frozen M3 kernel tests pin that a **bare `Kernel`** applies neither
(`behavior_phase.rs::fire_spawn_despawn_custom_are_recorded_not_applied` and rows reading `entities[0]`
after a `Despawn`). The kernel cannot spawn from a template: templates, the registry and the plugin build
scope live in `umbra-engine` (as for ghosts, ADR 0030). Spawned and removed entities must reach the
network manager in the same tick (publish and `EntityRemoved`). No golden scenario emits either intent.

## Options
1. **The kernel reports lifecycle intents; the engine applies them right after `Kernel::tick`, before the
   network publish and before the tick's hash and frame (chosen).** Pros: bare-kernel tests unchanged;
   spawns build exactly like placements and API spawns (`spawn_placement`, DRY); one boundary for
   commands, network and lifecycle. Cons: an entity spawned in tick N first acts in tick N+1.
2. **Apply inside the kernel Events phase through an injected spawner port.** Pros: within the tick.
   Cons: breaks the frozen M3 tests (A-001 needs a migration ADR), widens the kernel with a template
   concept.
3. **Opt-in per scenario.** Pros: none of the goldens could move even in theory. Cons: a second semantics
   of the same intent; the goldens hold anyway because no golden scenario emits these intents.

## Decision
Option 1.
- Kernel (additive): the Events phase also collects, for each **Owned** entity in `EntityId` order, its
  `Spawn` and `Despawn` intents in emission order into `TickReport::lifecycle: Vec<(EntityId, Intent)>`.
  Nothing else changes in the kernel; the intents stay in snapshots as before.
- Engine `Spawner` (attached at instantiate, built from the scenario registry with graphs and the loaded
  templates): in `tick_once`, after `Kernel::tick` and before `NetLink::publish`, it applies
  1. every `Spawn` in report order: template from the scenario, position and heading from the intent,
     speed 0, no route, no name or group, **force of the spawner** (ASM-601), `Ownership::Owned`, built
     inside the plugin build scope; a new `EntityId` (next id, REQ-C-2) whose RNG stream is keyed by its id
     (ADR 0006);
  2. then every `Despawn` (an entity that spawns and despawns itself in one tick still spawns first) via
     `Engine::remove` (behavior instance dropped, plugin `destroy_instance` on the engine thread, network
     link notified).
  Each applied spawn appends `SimEvent::EntityCreated { entity, template }`, each despawn
  `SimEvent::EntityRemoved { entity }` to `report.events` after the kernel's events.
- Limits (deterministic, ASM-603): at most 256 spawns per tick per engine and 100,000 entities in total;
  excess intents are dropped in order with one warning per tick and a counter. A spawn naming an unknown
  template or failing to build is dropped with one warning per template id and a counter; it is not a
  plugin fault (ASM-604).
- `Session` keeps its directory current from these events (template, no name). The API sees spawned and
  removed entities through the existing events and entity lists: **no contract change**.
- Recording (ADR 0053): lifecycle intents are derived state, not inputs; they are never recorded.

## Consequences
- Easier: declarative and plugin behaviors can create and remove entities; network peers see them.
- Harder: the entity count changes within a run (stream clients already receive full snapshots, ADR 0027);
  spawn storms are bounded by the caps.
- Migration: none; frozen M3 tests and all four goldens unchanged; ADR 0016 and 0028 notes updated.
- Revisit for spawned entities with names, routes or inherited velocity (a richer `Spawn` in intent set
  v2), or for in-tick application if a use case needs it.

## Implementation notes (WI-4 review, 2026-10-23; the decision above is unchanged)
- **Per-tick count.** The cap of 256 counts every `Spawn` intent *seen* in a tick (applied, rejected or
  capped), not only the applied ones, so the work per tick is bounded even for a flood of bad templates
  (ASM-683). "At most 256 spawns per tick" above reads as "at most 256 spawn intents handled".
- **Total count.** "100,000 entities in total" counts the engine's **Owned** entities; the ghosts of a
  network link are bounded by their own `max_ghosts` and do not use up the cap (ASM-689).
- **Directory.** The `Session` directory text above is superseded in mechanism: the directory of Owned
  entities built from templates lives in `Engine`, kept by `spawn_placement` and `Engine::remove`, so the API
  sees spawned and removed entities with no sync step for any driver (ASM-688, supersedes ASM-606 and
  ASM-682). Still no contract change.
- **Panic safety.** The spawner is taken out of the engine while it applies the lifecycle (ASM-681); an
  engine whose tick panicked must be dropped, not driven further (stated in the `Engine` rustdoc, ASM-693).
- **Warnings.** A rejected spawn warns once per template id for the life of the engine, naming the spawner's
  entity id, the template and the error (ASM-690).
