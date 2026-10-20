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

## Amended 2026-10-19 after the M5 design review (human-approved)
These rules supersede the Decision above where they differ.

### Frames
- **Priming (M1).** A `subscribe` immediately gets one `cause: "control"` frame of the current state, even
  when nothing ticks: `idle`, `ready`, `paused` or `stopped` (ADR 0024 `Prime`). A late subscriber
  therefore never waits for a tick to see the map.
- **Events survive dropped frames (M2).**
  - Events of frames the bus drops are carried into the next delivered frame. The connection task does
    the same for frames it skips.
  - Both are capped at `max_events_per_frame`; beyond the cap the frame sets `events_truncated`.
  - Test: a `step(100)` that crosses the `Fire` at tick 1130 delivers that event to a subscriber with
    capacity 4.
- **Tick stride (M11).** Each connection subscribes with
  `min_tick_stride = max(1, tick_rate_hz / max_rate_hz)`, using integer division. Below that stride the
  engine builds no frame for that connection. Control frames, step completions, pause and stop are always
  delivered.
  *Implementation note (M5 WI-16, ASM-212; the decision is unchanged):* the connection passes its
  `max_rate_hz` (`SubscribeOptions::max_rate_hz`) and the engine bus applies the same formula with the
  tick rate the engine sets on load and unload, so the stride follows a load that changes the rate.

### Connection (M4, m9)
- **Generic loop.** The connection loop is written over a generic `Sink<Message>` and
  `Stream<Item = Result<Message, _>>`, split into a reader half and a writer half.
  - The reader handles `subscribe`, the size limits and protocol errors.
  - The writer sends frames. No `select!` ever cancels a send that is in progress.
- **Limits.**

  | Limit | Value | When exceeded |
  |---|---|---|
  | Incoming message size | 64 KiB | close with 1009 |
  | `entity_ids` per `subscribe` | 10,000 | `error`, connection stays open |
  | Concurrent stream connections | `[server.stream] max_connections`, default 64 | upgrade refused with `503 too_many_connections` |
- **Encoding.** Frame JSON is encoded on `tokio::task::spawn_blocking`, so a 2 MB frame never stalls a
  runtime worker.
- **No `unsubscribe`.** There is no unsubscribe message; a client that is done closes the socket.
- **Origin check.** The upgrade checks `Origin` against `[server] allowed_origins` (ADR 0026 amendment).

### Testing the slow client (M4)
A blocked send is not reachable reliably over loopback, so the slow-client acceptance test drives the
connection loop directly:
- the sink is a stub whose `poll_ready` returns `Pending` forever;
- time runs under `tokio::time::pause`, using tokio's `test-util` feature as a dev-dependency;
- the test asserts the `too_slow` close after `send_timeout_s`, while the engine keeps ticking.

One real-socket smoke test remains. It uses a generated fixture with 2,000 entities, so kernel socket
buffers fill quickly.

### Shutdown (M12)
Each stream task holds a shutdown signal. On server shutdown it closes the socket with 1001; the whole
graceful shutdown has a deadline (ADR 0026, plan Design).
