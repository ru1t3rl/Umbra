# ADR 0015: Perception, the `builtin.visual` sensor and forces

- **Status**: Accepted (decided by the human project owner on 2026-10-17; O(n^2) perception accepted, spatial index and profiling in M9; M3 plan `docs/plans/M3.md`). Spec decision D-32.
- **Requirement IDs**: REQ-K-2 (Perception phase), REQ-K-4, REQ-K-5, REQ-C-6 (`line_of_sight`), REQ-T-1, REQ-B-15, REQ-B-16, ADR 0010, ASM-032
- **Affected**: `cgf-core` (`perception`, `terrain`), `cgf-kernel`, `cgf-engine::builtins`, `cgf-templates` (scenario `force`), `scenario.v1.schema.json`

## Context
M3 must deliver "perception (visual sensor)" and an engage behavior, which needs "enemy". Templates already
accept `[[sensors]] kind = "builtin.visual"` with `range_m` and `fov_deg` (spec 7.1). There is no notion of
side or force yet, `TerrainProvider` has no `line_of_sight` (ASM-032 deferred it to M3), and section 4 has
no perception crate.

## Options
1. **Core `Sensor` trait and `Contact` type; kernel Perception phase; `builtin.visual` in
   `cgf-engine::builtins` (precedent: `builtin.terrain.flat`); optional placement key `force`; hostile means
   "both have a force and they differ".**
2. **Same, with a new crate `cgf-perception`.** Pros: SRP. Cons: rule-table and spec section 4 change for
   one small sensor.
3. **Forces in templates instead of placements.** Cons: one template per side for the same vehicle.

## Decision
Option 1.
- `Sensor::sense(&self, observer: &Observer, candidates: &[Target], terrain: &dyn TerrainProvider,
  out: &mut Vec<Contact>)`; factories register as `ComponentKind::Sensor`. The kernel runs sensors for
  Owned entities only, in `EntityId` order, against all other entities (Owned and Remote) as they were
  before the tick; contacts from several sensors are merged per target (first sensor in template order
  wins) and sorted by target `EntityId`.
- `Contact { entity, position, velocity, force, range }`; perfect detection (no probability) in v1.
- `builtin.visual` params: `range_m` (> 0, <= 100 km), `fov_deg` (in (0, 360]), `eye_height_m` (>= 0,
  default 2). Detected iff 3D range <= `range_m`, the horizontal bearing is within `fov_deg / 2` of the
  observer's heading, and the terrain has line of sight between the two points raised by `eye_height_m`.
- `TerrainProvider::line_of_sight(&self, from: Ecef, to: Ecef) -> bool` is added with a default
  implementation (fixed-spacing sampling against `height_at`, ASM-080) so existing implementors and M2
  tests compile unchanged; `impl TerrainProvider for Arc<T>` forwards it.
- Scenario `[[entities]]` gains optional `force = "<name>"` (`[a-z][a-z0-9_]*`), an additive key under
  ADR 0010's policy (no `schema_version` bump); entities without a force are neutral.

## Consequences
- Easier: the spec 7.1 template validates once `builtin.visual` is registered; sensors from plugins (M7)
  use the same trait.
- Harder: perception is O(n^2) per tick; NFR-1 (10k entities) will need a spatial index (M9 profiling).
- Revisit when a second sensor or the sensor plugin kind (M7) arrives (move to `cgf-perception`), or when
  force relations (allies, neutrals that are hostile) are needed.
