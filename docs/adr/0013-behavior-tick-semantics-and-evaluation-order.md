# ADR 0013: Behavior tick semantics and evaluation order

- **Status**: Accepted (decided by the human project owner on 2026-10-17; state hash stays v2, revisit hash v3 with replay in M7; M3 plan `docs/plans/M3.md`). Spec decision D-30. Amended 2026-10-17 (twice) and 2026-10-18 (halt
  scope: parent halts reach Running nodes only), see the sections at the end.
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

## Amended 2026-10-17 after step 4 (acceptance tests)
- "Others halted" (parallel) and every other halt apply to **Running** nodes only; halting a node that
  never started or already finished is a no-op.
- Exact tick arithmetic for `cooldown`, `timeout`, `wait` and `timer_elapsed` is pinned in ASM-092(a):
  cooldown re-ticks its child at `t + n`; timeout halts at `s + n`; `wait` is Running for `n` ticks, then
  Success; `timer_elapsed` compares the absolute tick with `n`.

## Amended 2026-10-18 after WI-8 review (decided by the human project owner)
Resolves a conflict between the Decision's "Halt recursively resets node state, timers" and the
2026-10-17 "halt applies to Running nodes only": as implemented in b352ccd, halting an ancestor cleared a
cooling `cooldown`'s timer, so in `sequence [enemy_near, selector [cooldown(fire), aim]]` losing and
regaining the contact while `aim` runs let `fire` skip its cooldown. This amendment replaces the
Decision's **Halt** bullet; blackboards are still not cleared (ADR 0014).
- **Running:** a node is Running when the last time it was ticked it returned Running and it has not
  been halted since. A `cooldown` that is cooling (its child finished, its timer is running) returns
  Failure, so it is not Running.
- **Parent halt** (a reactive composite skipping or pre-empting a child, a `parallel`'s decisive halt,
  a `timeout` expiry, an FSM guard transition, a subgraph node halting its child behavior through
  `halt_running` (below), and the halt of whatever these reach): a no-op on any node that is not Running. A halted Running node resets its
  **own** state (a memory composite's position, a `parallel`'s slots, a `repeat`'s count, a `timeout`'s
  start tick, an FSM's current state back to `initial`), tells a Running action to halt, and passes the
  halt on to its Running children only. A finished child (including a finished `parallel` slot) is not
  halted and keeps its state; resetting a slot to idle does not touch the child's subtree.
- **Instance halt** (`BehaviorInstance::halt`, "the next tick starts fresh"): unchanged. Every node
  returns to its `instantiate` state, `cooldown` timers included; Running actions are told to halt,
  others are not. A subgraph node's halt of its child is a parent halt, not an instance halt.
- **Subgraph halt mechanism** (human decision, 2026-10-18): a subgraph's child is a `dyn Behavior`
  (graph, built-in, or a plugin from M4), and `BehaviorInstance::halt` is the instance halt, so
  `cgf-core` gains one additive default method on `BehaviorInstance` (allowed under the A-001 freeze):
  `fn halt_running(&mut self) { self.halt() }`, documented as "a parent halt (ADR 0013): halt what is
  Running and keep the state of finished nodes; the default is the full reset of `halt`".
  `GraphInstance` overrides it with the Running-only parent halt from its root; its `halt` stays the
  full reset. A subgraph node's parent halt calls `halt_running` on its child, and only when the
  subgraph node is Running; its full-reset (instance-halt) path calls `halt` on its child. Built-in and
  plugin behaviors that do not override `halt_running` get the full reset, which is correct for any
  behavior without timers that outlive a Running activation. **M4:** the plugin ABI must mirror
  `halt_running` (an optional table entry that falls back to `halt` when absent), or document that
  plugin behaviors always get the full reset.
- **Required cases** (whole-tick arithmetic of ASM-092(a); `n` = cooldown ticks; `fire` is an action
  returning Success at once, `aim` an action returning Running):
  1. *Cooling cooldown under a pre-empted reactive sequence keeps its timer.* In
     `sequence [enemy_near, selector [cooldown(fire), aim]]` with `n = 5`: tick 0 `fire` succeeds
     (timer to tick 5); tick 1 cooldown fails, `aim` Running; tick 2 `enemy_near` fails, the selector
     and `aim` are halted, the cooldown is not; ticks 3 and 4 with `enemy_near` true: cooldown fails
     without ticking `fire`, `aim` Running; tick 5: `fire` is ticked again. No `fire` tick in 1..4.
  2. *Cooldown whose child is Running when halted resets.* The child is halted (its action `halt` runs
     once) and no timer starts, because the timer starts only when the child returns Success or
     Failure (ASM-095(c)); the next time the cooldown is ticked it ticks its child at once.
  3. *Instance halt clears all.* After case 1's tick 0, `BehaviorInstance::halt`; the next tick ticks
     `fire` (no cooldown pending).
  4. *Finished children keep their state.* A cooldown that finished inside a Running `repeat` or as a
     finished slot of a Running `parallel` keeps its timer when that node is halted.
  5. *The same through a subgraph.* Cases 1 and 4 with the cooldown inside a graph called through a
     `subgraph` node give the same traces as the inlined form; a built-in child that does not override
     `halt_running` gets the full reset.
