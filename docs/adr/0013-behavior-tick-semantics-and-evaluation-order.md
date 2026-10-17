# ADR 0013: Behavior tick semantics and evaluation order

- **Status**: Accepted (decided by the human project owner on 2026-10-17; state hash stays v2, revisit hash v3 with replay in M7; M3 plan `docs/plans/M3.md`). Spec decision D-30.
- **Requirement IDs**: REQ-B-1, REQ-B-8, REQ-B-16, REQ-K-2, REQ-K-4, REQ-K-5, D-26, ADR 0007
- **Affected**: `cgf-behavior` (interpreter), `cgf-kernel` (Behavior phase), `docs/behavior.md`

## Context
REQ-B-8: every node returns Success, Failure or Running; the order is fixed and documented; nodes only
emit intents. Behavior trees differ widely on memory, preemption ("halt") and parallel semantics. The A-001
example (`selector [engage, patrol]` with `engage = sequence [enemy_near, suppress]`) only works if the
selector re-checks `engage` while `patrol` is running. The state hash is frozen at v2 by an M1 test.

## Options
1. **Tick from the root every tick; composites reactive by default, `memory = true` opt-in.** A node that
   was Running and is not reached (or is aborted) is halted. Pros: matches the A-001 example; one rule to
   explain; preemption is natural. Cons: sequential action chains need `memory = true`.
2. **Memory composites by default (resume at the running child).** Pros: classic sequences work without a
   flag. Cons: the A-001 example never re-checks `engage` once `patrol` runs; users must learn "reactive"
   variants for the most common pattern.
3. **Event-driven trees (only re-tick on blackboard change).** Cons: complex, hard to make deterministic.

## Decision
Option 1.
- **Order:** entities in `EntityId` order (Owned only, REQ-K-5); within an entity, depth-first from the
  root, children in their array order. Behaviors see contacts from the Perception phase of the same tick
  (computed from pre-tick state), so entity order cannot leak between entities.
- **Composites:** `sequence` ticks children until one returns Failure or Running (returns that) or all
  succeed (Success). `selector` mirrors it. Reactive (default): start at the first child every tick; a
  child that was Running last tick and is not reached, or is skipped because an earlier child now returns
  Running or decides the result, is halted. `memory = true`: resume at the Running child; restart from the
  first child after the composite finishes or is halted. `parallel` ticks every unfinished child each tick
  in order; `all`: Failure as soon as one fails (others halted), Success when all succeeded; `any`: Success
  as soon as one succeeds (others halted), Failure when all failed; finished results are kept until the
  parallel finishes or is halted.
- **Decorators:** `inverter` swaps Success/Failure; `succeed_always` maps Failure to Success; both pass
  Running. `repeat` ticks its child at most once per tick; after a child Success it returns Running until
  `count` successes (then Success); child Failure is Failure. `cooldown`: after the child finishes, returns
  Failure without ticking it until `duration` has elapsed. `timeout`: halts the child and returns Failure
  when it is still Running `duration` after it started. Durations become whole ticks once per instance:
  `ceil(duration_s * hz)` with the ASM-066 two-ULP guard, at least 1.
- **Leaves:** a condition returns Success or Failure, never Running. Actions may return any status.
- **Subgraph node:** copy bound inputs into the child (an unset required input: Failure, child halted,
  not ticked), tick the child once, copy bound outputs to the parent (ADR 0014).
- **FSM** (per tick, at most one guard transition and one completion transition): (1) the first `when`
  transition of the current state whose guard returns Success fires (halt current state node; entering an
  exit returns its mapped status and resets the FSM); (2) tick the current state's node once; (3) if it
  returned Success or Failure, the first matching `on` transition fires (an exit returns its status); a
  state node that finished without a matching `on` restarts next tick; otherwise Running. Halting an FSM
  resets it to `initial`.
- **Halt** recursively resets node state, timers and FSMs below the halted node; blackboards are not
  cleared (ADR 0014).
- **Intents** from all leaves are appended in evaluation order; motion resolves them as in ASM-030.
- **Randomness and time:** only `BehaviorContext::{now, dt, rng}`; `rng` is the entity's existing hashed
  `EntityRng` stream (ADR 0006), so draws are covered by the state hash.
- **State hash:** stays v2; behavior node state and blackboards are not hashed (their effects reach the
  hash through intents, motion and RNG state). Gate 4 additionally compares per-tick intents.

## Consequences
- Easier: the A-001 example runs as written; preemption needs no extra node type; bounded work per tick
  (no node ticks twice; no unbounded loops).
- Harder: a latent behavior-state divergence with no effect is invisible to the hash (mitigated by the
  per-tick intent comparison in gate 4).
- Revisit when replay (REQ-K-7, M7) needs behavior state in snapshots, or if profiling (NFR-1) asks for
  memory composites by default.

## Amended 2026-10-17 after WI-3 review
- **Erratum:** `BehaviorContext` has no `dt` field (the Decision's "`BehaviorContext::{now, dt, rng}`"
  reads `{now, rng}`). Durations are whole tick counts converted once at `instantiate` (ASM-078); a later
  `dt` would be an additive field with a new constructor.
- `instantiate` is fallible (`InstantiateError`): a duration that does not convert to ticks at the
  scenario rate fails the spawn instead of panicking or saturating.
- **Order independence:** each entity has its own `IntentBuffer`, contact vector and RNG stream and sees
  only pre-tick state, so the outcome of the Perception and Behavior phases does not depend on the order
  in which entities are visited (tested in WI-6); id order remains the fixed visiting order.
