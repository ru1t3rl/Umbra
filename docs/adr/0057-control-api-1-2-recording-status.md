# ADR 0057: Control API 1.2: recording status (additive to ADR 0026 and ADR 0035)

- **Status**: Proposed (architect, M7 plan `docs/plans/M7.md`, 2026-10-22). Human decision H8.
- **Requirement IDs**: REQ-K-7, REQ-X-4, REQ-X-5, REQ-X-6, REQ-X-7, D-44, D-45, D-54, ADR 0026, ADR 0027,
  ADR 0053
- **Affected**: `umbra-api-types` (`RecordingDto`, `API_VERSION` 1.2.0), `umbra-server` (mapping),
  `umbra-engine` (`RunStatus::recording`), committed `api/openapi.json`, `api/stream.schema.json`,
  `api/asyncapi.json`; the GUI client regenerates (M8 track)

## Context
`umbra serve --record-dir` records every session (ADR 0053). A client (the M8 GUI, scripts) should see
whether the current session is being recorded, where, and whether the recording failed. M8 runs in
parallel on its own branch and regenerates its client from the committed contract, so contract changes
must land early and separately. Everything else M7 adds needs no contract change: plugin capability kinds
are already in the wire `ComponentKind` (all seven), spawned and despawned entities use the existing
`entity_created`/`entity_removed` events, and air/sea parameter schemas are data served by
`GET /components`.

## Options
1. **Additive optional `recording` object in `RunStatus` (REST status and stream run status), API 1.2.0;
   no recording or replay endpoints in M7 (chosen).** Pros: minimal, additive within v1 (ADR 0026), lets
   the GUI show a badge. Cons: recording is started only by the server flag.
2. **Also `POST /api/v1/recording/{start,stop}` and replay sessions.** Pros: full control from clients.
   Cons: path confinement and lifecycle design for files written by the server, replay pacing in the GUI:
   no M7 acceptance need; can follow additively when M8 asks.
3. **No contract change.** Pros: zero churn. Cons: recording state invisible to clients.

## Decision
Option 1. `RecordingDto { state: "recording" | "failed" | "finished", file: String (confined display
path, never absolute, ADR 0026), records: u64, last_tick: u64 }`; `RunStatus.recording:
Option<RecordingDto>` (absent when not recording). `API_VERSION` 1.2.0. The contract and a mapping that
always yields `None` land first (M7 WI-3) so the GUI track can regenerate; the live values arrive with
the recorder (M7 WI-16).

## Consequences
- Easier: the M8 GUI can show recording status from day one of its branch.
- Harder: one more field to keep consistent between status and stream (contract tests).
- Revisit when clients need to start/stop recordings or replay (additive endpoints, 1.3.0).
