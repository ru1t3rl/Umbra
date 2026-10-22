# ADR 0055: State hash v3, selectable and opt-in (settles D-30's revisit; amends ADR 0007)

- **Status**: Proposed (architect, M7 plan `docs/plans/M7.md`, 2026-10-22). Human decision H5.
- **Requirement IDs**: REQ-K-4, REQ-K-7, REQ-B-5, REQ-B-16, REQ-K-5, A-2, D-22, D-30, ADR 0007, ADR 0013,
  ADR 0032 (ASM-230)
- **Affected**: `umbra-core` (`hash`: `StateHashVersion`), `umbra-core::behavior` (additive default
  method), `umbra-behavior` (instance digests), `umbra-kernel` (`state_hash_with`), `umbra-engine`,
  `umbra-cli` (`--state-hash`), `umbra-record` (header field)

## Context
State hash v2 covers tick, rate, and per entity id, ownership, motion model id, kinematic state, route and
RNG state. It does not cover behavior state (node statuses, timers, FSM state, blackboards), forces, or a
ghost's dead-reckoning basis (ASM-230). D-30 deferred "v3 with replay" to M7. Four goldens pin v2
(`849026656b7b42d2`, `3a7bfda0a76111b9`, `88954c5dc6b52b48`, `3cc077a4c83c4a63`), as do the M1/M2
reference-encoding tests and `umbra run` stdout. Replay itself only needs a hash that changes when state
diverges; v2 catches divergence once it reaches kinematics or RNG, v3 catches it at the tick it happens.

## Options
1. **Replace v2 by v3 everywhere.** Pros: one hash. Cons: every golden, reference test and recorded stdout
   changes (frozen M0-M2 tests; an ADR with migration plan and human approval), for no acceptance need.
2. **Keep v2 only.** Pros: nothing to do. Cons: behavior-state divergences are seen late (debugging replay
   and determinism failures is harder); ghost bases and forces never hashed.
3. **Versioned selection: v2 stays the default; v3 is opt-in per run and stored in recordings (chosen).**
   Pros: goldens and stdout unchanged by construction; stronger checks where asked (gate 4 rows,
   recordings, debugging); the mechanism lets a later v4 arrive the same way. Cons: two encodings to
   maintain; v3 cannot see inside plugin instances in M7.

## Decision
Option 3.
- `umbra_core::hash::StateHashVersion { V2, V3 }` (`#[non_exhaustive]`), `StateHasher::with_version(v)`;
  `StateHasher::new()` and `STATE_HASH_VERSION` keep meaning v2 (frozen API unchanged).
  `Kernel::state_hash()` stays v2; additive `Kernel::state_hash_with(StateHashVersion)`.
- **v3 encoding** = the v2 encoding with the version word 3 and, per entity after the RNG state:
  force (`u8` 0 none / 1 + string); remote track (`u8` 0 none / 1 + model `u32`, basis kinematic state,
  acceleration, angular velocity, basis tick); behavior digest (`u8` 0 none / 1 + the instance's digest
  bytes / 2 opaque).
- Additive default method `BehaviorInstance::digest(&self, h: &mut StateHasher) -> DigestSupport` (default
  `Opaque`); the `umbra-behavior` interpreter implements it for every node kind (status, memory index,
  timers in ticks, repeat counts, FSM state, subgraph children, blackboard values in key order); `[editor]`
  data is never part of it (REQ-B-16). Plugin behavior instances report `Opaque` in M7 (an optional ABI
  `digest` entry is a later additive minor).
- Selection: `umbra run --state-hash 3` (per-tick and final hashes), `umbra run --record` writes the
  chosen version in the header (default 2), replay uses the header's. Gate 4 gains v3 rows for the new
  scenarios; the reference encoding is pinned by a hand-built test like v2's.

## Consequences
- Easier: replay and determinism failures localise to the tick where behavior state first differs.
- Harder: every new hashed component must be added to v3 (or a v4), reviewed like ADR 0007 asks.
- No migration: v2 unchanged; no golden or frozen test changes.
- Revisit when plugin instance digests are wanted (ABI), or when v3 should become the default (would need
  a goldens migration ADR).
