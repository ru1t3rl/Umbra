# ADR 0045: GUI live data flow: stream store, per-frame batching, data-layer exports

- **Status**: Accepted by the human 2026-10-20 (proposed by frontend-architect, M5b plan `docs/plans/M5b.md`, 2026-10-20)
- **Requirement IDs**: REQ-X-6, REQ-X-7, REQ-A-4, F-5, NFR-1 (10,000 entities), ADR 0027 (stream protocol v1), `gui/CLAUDE.md` (performance, rule 7)
- **Affected**: `@umbra/engine-stream`, `@umbra/data`, `@umbra/map-view`, decision row D-61

## Context
Stream v1 frames are full snapshots (`kind: "snapshot"`, entities ascending by id, numbers bit-identical
to REST), at most 60 per second per connection, with `seq`/`dropped` making server-side drops visible.
The GUI must not re-render React per entity and must batch per animation frame. F-5 says features use
REST only through `@umbra/data` hooks and never call a generated function; the M5b e2e test must call
`step(n)` "through the generated client" from Node, outside React.

## Options
1. **Latest-frame-wins store, flushed once per animation frame; imperative subscription for the map;
   narrow React selectors for everything else (chosen).**
2. Apply every frame to the store as it arrives. Cons: several store updates per paint at 60 Hz, wasted
   work at 10,000 entities.
3. Decode in a Web Worker and transfer typed arrays. Cons: more machinery than M5b needs; kept as the
   documented escalation if the 10,000-entity measurement (WI-10, WI-12) misses the budget.

## Decision
Option 1.
- `connectEngineStream({ url, subscription, createSocket?, scheduleFrame? })` owns one WebSocket
  (`?encoding=json`, protocol `umbra.stream.v1`), sends `subscribe` after `hello`
  (`max_rate_hz: 30`, no filter, `state_hash: false`), decodes through a `Decoder` interface (only
  `json` in M5b: the negotiation hook), and keeps the newest decoded frame in a pending slot. A scheduled
  animation-frame flush moves it into a Zustand vanilla store: entities are replaced, not merged; a new
  `session` drops everything. Unknown message types and fields are ignored; `error` messages are
  counted. Events are not stored in M5b (M8 log view). Reconnect with backoff 0.5, 1, 2, 4, 8 s
  (capped), reset on `hello`.
- React reads only narrow selectors (`useStreamStatus`, `useRunStatus`); the map reads through
  `EntitySource.subscribe` outside React and hands MapLibre at most one `setData` per animation frame.
- `@umbra/data` re-exports the orval output (hooks, query-key getters, model types, and the plain request
  functions) plus `DataProvider`, `configureEngineUrl`, `ApiProblem`. Plain request functions are for
  `gui/e2e/` and scripts only; an architecture test fails if `features/` or `feature-shells/` import one.

## Consequences
- Easier: bounded work per paint regardless of message rate; one generated client for UI and e2e.
- Harder: intermediate frames are never painted (fine for a live view; replay is M7/M8).
- Revisit when a 10,000-entity frame costs more than the frame budget to decode (escalate to option 3)
  or when M8 needs events in the store.
