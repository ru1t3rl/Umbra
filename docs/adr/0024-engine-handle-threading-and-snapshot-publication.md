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
  once: `Tick` runs exactly one `Kernel::tick` (with the M4 `take_abort` check, as `run` does; on
  `PluginFault` the tick did run and the engine holds that tick's state, ASM-145 a);
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
  there). A fresh load starts paused at tick 0.
- **Settings.** `HandleConfig` carries the command capacity, stack size, wait slice, plugin paths and the
  default tick rate for scenarios that omit `tick_rate_hz` (ADR 0029); the CLI fills it from flags, the
  `umbra.toml` file and built-in defaults. A session records the effective tick rate and its source.
- **Shutdown.** Dropping the last handle, or `EngineHandle::shutdown`, ends the thread after the current
  tick; pending replies get `HandleError::Closed`.

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
