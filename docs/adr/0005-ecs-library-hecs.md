# ADR 0005: ECS library for the kernel is `hecs`

- **Status**: Accepted (approved by the human project owner on 2026-10-17)
- **Requirement IDs**: D-3, D-4, REQ-K-1, REQ-K-2, REQ-K-4, A-1, NFR-1, NFR-4, D-18
- **Affected**: `cgf-kernel` (only crate that may name ECS types), `deny.toml`, `Cargo.lock`

## Context
D-3 fixes "ECS (`bevy_ecs` standalone, or `hecs`)" but leaves the pick open. M1 builds the kernel, so the
choice is due now. Forces: determinism (REQ-K-4: fixed system order, no hash-order dependence), a
single-threaded tick first (D-4), our own fixed six-phase pipeline (REQ-K-2), 10k entities at 30 Hz
(A-1/NFR-1), a small and license-clean dependency tree (NFR-4, ADR 0001), and low churn, because the
kernel will live with this choice for years. `EntityId` is ours (REQ-C-2: u64, never reused), so the
ECS's own generational handles must never leave the kernel either way.

## Options
1. **`hecs`.** A small, archetype-based ECS: a `World` plus queries, no scheduler, no executor.
   - Pros: tiny API and dependency tree; MIT/Apache dual licence; no threads unless we add them; we
     write the phase pipeline ourselves, which REQ-K-2 needs anyway, so the system order is plain code
     and obviously fixed; infrequent breaking releases. Query iteration order is archetype index, then
     row; both depend only on the sequence of world operations, so it is reproducible.
   - Cons: no built-in scheduler, change detection or events (we do not want the first and do not need
     the others in v1); handles are reused after despawn (hidden behind `EntityId` anyway).
2. **`bevy_ecs` standalone.**
   - Pros: rich (schedules, change detection, events, resources), fast, very active community; a
     parallel executor if A-1 ever needs it.
   - Cons: much larger dependency tree (many `bevy_*` crates, task pool); multi-threaded executor by
     default, so single-threaded execution and a total system order must be configured and guarded
     (unordered systems in a schedule are ambiguous unless chained); breaking releases every few months;
     a second scheduler concept next to the REQ-K-2 pipeline (two ways to order systems, against DRY).

## Decision
Option 1, `hecs`, used only inside `cgf-kernel` (KISS, D-4, ISP). Rules that keep it deterministic and
replaceable:
- `hecs` types never appear in a public signature of `cgf-kernel`. Entities are addressed by
  `cgf_core::EntityId`, mapped through a `BTreeMap<EntityId, hecs::Entity>` (REQ-C-2, REQ-K-4).
- Systems either treat entities independently or iterate in `EntityId` order. State hashes and snapshots
  always walk entities in `EntityId` order, never in ECS storage order (ASM-037).
- The ECS's internal hash maps are only used for lookups, never iterated by us; the determinism test
  (gate 4) runs the scenario twice in one process and the CLI test runs it in two processes, so a
  per-process hasher seed would be caught.
- The dependency is added with `default-features = false` plus the features we need; any new licence it
  brings (for example `Zlib` from `foldhash`) is added to `deny.toml` in the same PR (ASM-020).

## Consequences
- Easier: the tick pipeline is ordinary, readable Rust; the dependency tree stays small; swapping the
  ECS later touches one crate.
- Harder: if A-1 grows (much more than 10k entities) or profiling demands parallel phases, we write the
  scheduler ourselves (D-4 says "via scheduler later" anyway) or revisit this ADR.
- Revisit if: `hecs` becomes unmaintained, a needed feature (change detection for publish thresholds in
  M6) costs more to build than switching, or the human changes A-1 (spec section 19 Q2).
