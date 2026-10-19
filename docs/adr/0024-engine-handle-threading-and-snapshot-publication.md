# ADR 0024: Engine handle, threading model and snapshot publication

- **Status**: Accepted by the human 2026-10-19 (proposed by the architect, M5 plan `docs/plans/M5.md`; awaiting the human project owner)
- **Requirement IDs**: REQ-X-1, REQ-X-7, REQ-K-2, REQ-K-3, REQ-K-4, REQ-K-6, REQ-S-1, REQ-S-2, REQ-G-1, D-1, D-4, D-5, D-10, ASM-039, ASM-098
- **Affected**: `umbra-engine` (new `handle`, `bus`, `session` modules; additive `Engine` methods), `umbra-kernel` (additive `Kernel::remove`), `umbra-server` (consumer)

## Context
REQ-X-1 asks for a cloneable, thread-safe `EngineHandle` whose commands go through a bounded channel and
apply at deterministic tick boundaries. REQ-K-6 asks for an immutable per-tick snapshot behind a bounded
buffer that drops for slow observers and never blocks the tick. D-4 keeps one simulation thread; D-5 keeps
wall time inside the `Clock` (only `StdWallClock` reads it, ASM-035). The behavior interpreter needs a
tick thread with at least 4 MiB of stack (ASM-098). `Engine` today is a synchronous facade whose `run`
blocks until a tick count or idle and whose real-time wait sleeps in `WallClock::sleep_until`. D-10 and
D-1 forbid putting the server or an async runtime inside the engine.

## Options
1. **Owner thread + std channels + callback replies (chosen).** One engine thread owns the `Engine` (and
   everything it holds, so `Engine` need not be `Send`), drains a `std::sync::mpsc::sync_channel`
   between ticks, and drives the clock one decision at a time (`Engine::pump`). Replies are
   `FnOnce(Result<T, HandleError>) + Send` callbacks, so a blocking caller passes a std channel and the
   async server passes a tokio oneshot without the engine knowing tokio. Snapshots go to a std-only bus.
   Pros: no async runtime or new dependency in the engine; the CLI keeps working unchanged; one place
   applies commands. Cons: real-time waits are sliced (command latency up to the slice).
2. **Tokio inside the engine** (async channels, `tokio::time` pacing). Cons: an async runtime and a second
   time source in the engine crate, against D-5 and D-10; the CLI would pull tokio.
3. **`Arc<Mutex<Engine>>` shared with the server.** Cons: request handlers would run engine code on async
   worker threads (small stacks, ASM-098), locks across ticks, commands not at tick boundaries.

## Decision
Option 1.
- **Thread.** `EngineHandle::spawn(HandleConfig)` starts one named thread (`umbra-engine`) with an explicit
  stack; default 8 MiB, anything below 4 MiB is refused (`SpawnError::StackTooSmall`). The thread creates
  and owns every `Engine`, `PluginHost` and registry of a session. Server worker threads never tick.
- **Commands** (`Command` enum, each variant carries a typed `Reply<T>`): load, unload, start, pause,
  resume, stop, step, set clock, spawn, remove, and read-only queries (status, entities, entity, templates,
  components, catalog, plugins, graph validation). `submit` uses `try_send`; a full queue is
  `HandleError::Busy` (default capacity 64). Blocking wrappers exist for embedders and tests.
- **Tick boundary.** The thread applies every queued command, in arrival order, before polling the clock
  for the next tick, i.e. after tick N's Publish and before tick N+1's Ingest. This is the Ingest
  boundary of REQ-X-1 at the engine level; the kernel's Ingest phase stays reserved for network input
  (M6). Every reply that changes state carries the tick at which it applied (input for M7 replay).
  Queries read the engine between ticks through `&self` only.
- **Pacing.** Additive `Engine::pump(max_wait) -> Result<Pumped, EngineError>` polls the clock
  once: `Tick` runs exactly one `Kernel::tick` (with the M4 `take_abort` check, as `run` does);
  *(amended after the M5 design review, M9)* a plugin fault under `abort` returns
  `Ok(Pumped::Aborted { report, fault })`, so the faulting tick's report and events are kept and its
  frame is published (ASM-145 a); `Err` is left for errors where no tick ran;
  `WaitUntil(d)` waits through the clock's `WallClock` until `min(d, wall_now + max_wait)` (default
  `max_wait` 10 ms), so no new wall-clock site exists; `Idle` returns at once and the thread then blocks
  on the channel. `Engine::run` and `Engine::step` are unchanged.
- **Publication.** After each tick, and after each state-changing command (a "control" frame), the thread
  builds one immutable `Arc<Frame>` (session, sequence number, `SimTime`, kernel `Snapshot`, entity
  directory, events, run status, optional state hash) **only if at least one subscriber exists**, and
  pushes it to every subscription. Each subscription is a ring of fixed capacity (default 4): on overflow
  the **oldest** frame is dropped and a drop counter increments; the newest is always kept. Pushing takes a
  short uncontended mutex per subscriber and never waits on a consumer; an optional `notify` callback
  (which must not block; the server passes `tokio::sync::Notify::notify_one`) wakes the consumer.
  Wire conversion (DTOs, geodetic display values, filters) runs on the consumer's thread, never on the
  tick thread. The kernel's Publish phase stays empty; the bus lives in `umbra-engine` (ASM-039).
- **Sessions.** A `Session` (engine-level, also usable without the handle) is one scenario load: a fresh
  `load_scenario_with_plugins` + `instantiate_with_plugins` (single-use `LoadedScenario`, ASM-145 b), the
  scenario registry, the entity directory (template, name, group) and the plugin host. A load replaces the
  current session (the old engine and host drop on the owner thread, so plugin `destroy_*`/`shutdown` run
  there). A fresh load starts paused at tick 0. *(amended after the M5 design review, D-a)* The old
  session is dropped **before** the new load starts; a failed load leaves `idle`.
- **Settings.** `HandleConfig` carries the command capacity, stack size, wait slice, plugin paths and the
  default tick rate for scenarios that omit `tick_rate_hz` (ADR 0029); the CLI fills it from flags, the
  `umbra.toml` file and built-in defaults. A session records the effective tick rate and its source.
- **Shutdown.** Dropping the last handle, or `EngineThread::shutdown(self)` *(amended after the M5
  design review, M12: there is no `EngineHandle::shutdown`)*, ends the thread after the current tick;
  pending replies get `HandleError::Closed`.

## Consequences
- Easier: the engine stays std-only and server-free (D-1, D-10); the CLI `run` path is untouched; per-tick
  hashes with and without subscribers are identical by construction (snapshots are read through `&self`).
- Harder: in real-time and scaled modes a command waits up to `max_wait` plus the remainder of the current
  tick; in as-fast-as-possible and real-time modes the tick at which a command applies depends on when it
  arrives, so scripted runs are reproducible only in stepped mode or while paused (documented; M7 replay
  records the applied ticks). Building a `Snapshot` per tick costs O(n) while anyone subscribes (NFR-1
  measured in M9).
- Revisit if profiling shows snapshot building dominates (publish every k ticks, or deltas), or if M6
  adapters need a different ingest path.

## Amended 2026-10-19 after the M5 design review (human-approved)
The api-engineer's design review found 13 Majors, none of them Blockers; the human approved every
recommendation. These rules supersede the text above where they differ.

- **Invariant: publish, then reply (M3).** A reply implies that the frame reflecting its change is
  already in every subscription.
  - The engine loop publishes the tick frame before it completes any step reply.
  - A state-changing command is applied first, then its control frame is published, then it replies.

- **Replacing a session (D-a).** `Load` works in this order:
  1. Cancel the pending steps of the old session.
  2. Drop the old session on the engine thread.
  3. Load the new one.

  A failed load leaves `idle`; the old session is gone either way.

  `Session` fields drop in this order: engine (instances, so plugin `destroy_instance`), then the scenario
  registry and behaviors (`destroy_behavior`), then the `Arc<PluginHost>`. When that releases the last
  image handle, `shutdown` runs and the library closes.

  Consequence: reloading a scenario with the same plugin starts a **new image lifecycle**. The entry is
  called twice again and `init` runs again (ADR 0021 RR-M1), unless `keep_loaded` keeps the library
  mapped.

  The same image rule applies to `validate_path_with_plugins` (validation-mode host):
  - while a session holds the image, validation reuses the live image: no entry call, no `init`, no
    `shutdown`, independent fault state;
  - when no session holds it, validation starts and ends its own lifecycle.

- **Late subscribers (M1).** `subscribe` registers the slot and marks it "needs prime". It then tries to
  enqueue the internal `Command::Prime`.
  - If the queue is full, the engine thread services the mark on its next iteration; it is iterating,
    because the queue is not empty.
  - The engine thread publishes one `cause: control` frame of the current state to that slot only.
  - Every state is primed: `idle` (no snapshot), `ready`, `paused` and `stopped`.

- **No lost events on drops (M2).** When a slot drops its oldest frame, the frame's events move into the
  slot's carry-over buffer instead of being lost.
  - The buffer is bounded by `max_events_per_frame`; beyond the cap the slot sets `events_truncated`.
  - The next `drain` returns the carried events first, ahead of the drained frames' own events.
  - `Drained` gains `carried_events` and `events_truncated`.

- **Frames nobody needs are not built (M11).**
  - `SubscribeOptions` gains `min_tick_stride` (default 1), and `Bus::wants_frame(tick)` asks every slot
    whether it is due.
  - The engine builds a tick frame only when some slot is due.
  - Control frames are always built (if any slot exists), and so are frames at the completion of a
    step, at pause and at stop or duration. So the last state is never skipped.
  - The server derives the stride from the tick rate and the subscription rate (ADR 0027).

- **Bus lock (m1).** The registry is a `Mutex<Vec<Weak<Slot>>>`.
  - `publish` copies the live `Vec<Arc<Slot>>` under the lock and releases it before any push or
    `notify`.
  - Poisoning is recovered with `PoisonError::into_inner`, never `unwrap`.

- **Replies and configuration (m1).**
  - `Reply<T>` invokes its callback with `HandleError::Closed` in `Drop` if it was never completed.
  - A `command_capacity` of 0 is `SpawnError::ZeroCapacity`.

- **Panics (m1).** The engine thread does **not** catch panics. A panic ends the thread, is logged, and
  every later command returns `Closed`; the server maps that to a permanent `503 engine_unavailable`. We
  chose this over `catch_unwind` per tick because the state after a panic is unknown, so continuing is
  unsafe.

- **Shutdown (M12).** `EngineThread::shutdown(self)` and `EngineThread`'s `Drop` both shut down the
  same way:
  1. Set a shared shutdown flag.
  2. `try_send` the internal `Command::Shutdown`. If the queue is full, the engine thread sees the flag
     on its next iteration; the real-time wait slice also checks it.
  3. Join the thread. The thread is never detached, so `Drop` blocks until the current tick ends.

  On shutdown the engine thread does three things, in order:
  1. Cancels pending steps; their replies get `Closed`.
  2. Drops the session.
  3. Exits.

  `submit` never blocks, so neither does shutdown.

- **Loop (m2).**
  - `Pumped::Idle` makes the thread block on `recv()` (no spin).
  - `stopped` and the duration stop are handle-level states; the clock is merely paused.
  - The loop uses the additive accessors `Engine::pending_steps()` and `Clock::pending_steps()`.

- **Observability (m16).** After each tick the handle emits a trace-level span `tick` with the
  per-phase wall times from `TickReport::phase_wall` (REQ-X-3).
