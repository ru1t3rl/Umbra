# ADR 0071: Log view data: a lossless event journal in engine-stream, pluggable log sources

- **Status**: Accepted by the human 2026-10-22 (with the M8 plan; G-1 scheduled as a low-priority engine WI on the M7 side, H-10; proposed by frontend-architect, M8 plan `docs/plans/M8.md`, 2026-10-22)
- **Requirement IDs**: spec section 13 ("log view"), REQ-X-6, REQ-X-7, NFR-1, ADR 0045 (amended here), ASM-300
- **Affected**: `@umbra/engine-stream` (journal), new domain feature `@umbra/log-view`, `@umbra/desktop` (engine process output), decision row D-81

## Context
M8 ships a log view. The contract has no engine log: `umbra serve` writes its `tracing` output to
stderr and no endpoint or stream message carries it. What the GUI does get:
- stream `events` in every frame (`entity_created`, `entity_removed`, `fire`, `detonation`, `custom`,
  `simulation_control`), with `events_truncated` and `dropped` making loss visible;
- the run status in every frame (state, stop reason, plugin and network faults);
- its own connection lifecycle;
- in the desktop app with a managed engine (ADR 0076), the engine process's stdout and stderr.

ADR 0045 keeps only the newest frame in a pending slot (latest wins per animation frame) and stores no
events (ASM-300). Taking events from the store would therefore lose the events of every coalesced frame.

## Options
1. **Event journal inside `@umbra/engine-stream`, fed at decode time; log view merges typed sources
   (chosen).** Every decoded frame appends its events to a bounded ring buffer *before* coalescing;
   synthetic records mark loss, connection changes, session changes and run-state transitions.
   `@umbra/log-view` reads it through an imperative `JournalSource` (like `EntitySource`) and merges the
   desktop engine-process source when present.
2. Second WebSocket with an `event_kinds` filter for the log. Cons: doubles connections and decoding;
   the server's per-connection limits apply twice; same loss semantics.
3. Store every frame (no coalescing). Cons: reverses ADR 0045's bounded work per paint at 10,000
   entities.
4. Wait for an engine log endpoint. Cons: contract gap G-1 is not M8's to close (ADR 0047); the log view
   would be empty in M8.

## Decision
Option 1.
- `@umbra/engine-stream` keeps a ring of `JournalRecord`s (default 5,000, setting 1,000..50,000,
  ASM-824). Appending is O(events); notifications reach subscribers at most once per animation frame,
  in the same flush as the entity store. A new session keeps the journal (the log spans sessions) and
  appends a session record.
- Public additions: `useJournalSource(): JournalSource` (`snapshot()`, `subscribe(listener)`,
  `capacity`), types `JournalRecord`, `JournalSource`.
- `JournalRecord = { seq; source: "event" | "run" | "connection" | "engine-process" | "gui"; level:
  "info" | "warning" | "error"; kind: string; message: string; tick?: number; time_s?: number;
  session?: number; entity_id?: number; detail?: unknown; received_at: number }` (ASM-825). `kind`
  keeps unknown event kinds verbatim (forward compatible).
- `@umbra/log-view` (domain) merges `JournalSource` and, when `@umbra/desktop` reports a managed
  engine, its engine-output source, ordered by arrival. It renders a virtualized `role="log"` list
  (ADR 0072) with filters by source, level and text.
- Engine log lines over the API are requested as contract gap G-1; when the engine side delivers them,
  they become a third source in a follow-up work item, without changing this design.

## Consequences
- Easier: no event is lost on the client; the log view works in the web build from day one.
- Harder: engine-stream holds one more bounded structure; the web build shows no engine `tracing`
  lines until G-1.
- ASM-300 ("stream events are not stored") is superseded for events only; entity handling is unchanged.
- Revisit when G-1 lands, or when the journal's memory at its maximum capacity matters on the target
  machines.
