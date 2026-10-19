# ADR 0027: Stream protocol v1 and the slow-client policy

- **Status**: Accepted by the human 2026-10-19 (proposed by the architect, M5 plan `docs/plans/M5.md`; awaiting the human project owner)
- **Requirement IDs**: REQ-X-6, REQ-X-7, REQ-K-6, REQ-K-4, REQ-A-4, NFR-1, ADR 0024, ADR 0026
- **Affected**: `umbra-api-types` (stream messages), `umbra-server` (stream handler), `api/stream.schema.json`,
  `api/asyncapi.json`, `gui/features/engine-stream` (M5b consumer)

## Context
REQ-X-6: WebSocket `/api/v1/stream`, subscriptions with filters (bounding box, entity ids, event kinds) and
a max rate, batched snapshot/delta frames, JSON in v1 with negotiated encoding (`json` | `msgpack`), types
exported as JSON Schema and AsyncAPI. REQ-X-7 and REQ-K-6: a slow client never slows the tick; slow
observers lose snapshots instead of queueing them. M5b needs, after `step(n)`, the entity positions of
exactly that tick, and the initial positions right after a load.

## Options
1. **Full filtered snapshots per frame, newest wins (chosen).** Each frame carries the complete filtered
   entity state of one tick; a client that falls behind skips frames and loses nothing but intermediate
   states. Pros: no client-side reconstruction, dropping is always safe, trivially correct after a drop.
   Cons: larger frames (mitigated by filters, rate limit, and later msgpack or deltas).
2. **Deltas with periodic keyframes.** Pros: smaller frames. Cons: a dropped delta forces a resync
   protocol; more state per client on the server; premature before profiling (NFR-1 is M9).
3. **Per-client unbounded queues.** Rejected by REQ-K-6.

## Decision
Option 1; the `delta` frame kind is reserved for a later minor version.
- **Connect.** `GET /api/v1/stream?encoding=json` upgrades to WebSocket. `encoding` defaults to `json`;
  `msgpack` and anything else are refused before the upgrade with `400 unsupported_encoding`. Text
  messages, one JSON object each, discriminated by `type`.
- **Server -> client.** `hello` (`protocol: "umbra.stream.v1"`, `api_version`, `encoding`, limits);
  `frame` (`kind: "snapshot"`, `cause: "tick" | "control"`, `session`, `seq`, `tick`, `time_s`, `run`
  status, `dropped`, `entities`, `events`, `events_truncated`, optional `state_hash`); `error` (`code`,
  `message`, for a malformed or invalid client message; the connection stays open). Control frames are
  published after state-changing commands (load, unload, spawn, remove, start, pause, resume, stop,
  set clock), so a client sees the entities right after a load and after a spawn without a tick.
- **Client -> server.** `subscribe` with optional `filter` (`bbox` in degrees `min_lat_deg`,
  `min_lon_deg`, `max_lat_deg`, `max_lon_deg`, `min_lon > max_lon` crosses the antimeridian;
  `entity_ids`; `event_kinds`), `max_rate_hz` (default 30, range (0, 60]), `state_hash` (bool, default
  false). A new `subscribe` replaces the previous one. Until the first `subscribe`, nothing but `hello` is
  sent. Unknown message types get an `error` and are otherwise ignored.
- **Entities** in frames use the same `EntitySummary` DTO as `GET /api/v1/entities` (id, name, template,
  group, force, ownership, `lat_deg`, `lon_deg`, `alt_m`, `heading_deg`, `speed_mps`, behavior, status,
  route progress), sorted by id; numbers are the engine's `f64` values serialised shortest-round-trip, so
  REST and stream values for one tick are bit-identical. Events carry their `tick` and `kind`.
- **Slow-client policy (two layers, both bounded).** (1) Engine bus (ADR 0024): one subscription per
  WebSocket connection, capacity 4 frames, oldest dropped. (2) Connection task: when the socket is ready
  it drains the subscription, sends only the **newest** frame, sets `dropped` to the number of frames it
  never sent since the previous frame (bus drops plus skipped frames), and concatenates the events of the
  skipped frames in order (at most 4096 per frame; beyond that `events_truncated = true`), then waits
  `1 / max_rate_hz` of wall time before the next send. Filtering and DTO conversion happen in this task,
  never on the tick thread. A send that does not complete within the send timeout (default 10 s) closes
  the connection with close code 1008 and reason `too_slow`. Events on the stream are therefore
  best-effort; the `dropped` and `events_truncated` fields make loss visible. A lossless event log is
  M7 (recording).
- **Configurable defaults** (ADR 0029, `[server.stream]` in `umbra.toml`): `default_rate_hz` (30),
  `max_rate_hz` (60, never above 60), `queue_frames` (4), `max_events_per_frame` (4096),
  `send_timeout_s` (10). `hello` reports the values in force. The simulation tick rate is independent of
  the stream rate (default 60 Hz since ADR 0029; a client at 30 Hz sees about every second tick).
- **Schema.** All stream messages are `umbra-api-types` types; `api/stream.schema.json` and
  `api/asyncapi.json` are generated (ADR 0025); the GUI uses only those (REQ-A-4).

## Consequences
- Easier: a stepped-mode client always gets the frame of the last tick; a slow or stalled client costs
  bounded memory (4 frames plus one in flight) and never touches the tick thread.
- Harder: frames are big at 10,000 entities (roughly 2 MB of JSON); clients should filter by bbox and
  rate; msgpack or deltas may follow after M9 profiling as additive encodings or frame kinds.
- Revisit when M9 profiling or the M8 GUI shows bandwidth problems, or when M7 recording offers a
  lossless event feed.
