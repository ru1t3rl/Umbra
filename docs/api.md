# Umbra control API v1

The REST API and the WebSocket stream of `umbra serve` (M5; REQ-X-4..9, D-10, D-11, ADR 0024..0029).
This page is the human reference. The machine-readable contract is committed in `api/`:

| File | Content |
|---|---|
| `api/openapi.json` | OpenAPI 3.1: every REST route, request, response and problem code (also served at `GET /api/v1/openapi.json`) |
| `api/stream.schema.json` | JSON Schema 2020-12 of every stream message (root `StreamMessage`) |
| `api/asyncapi.json` | AsyncAPI 3.0 description of the channel `/api/v1/stream` |

The files are generated from the Rust wire types (`umbra-api-types`) by `cargo xtask gen-api`; gate 10
(`cargo xtask gen-api --check`) fails on drift. If this page and the files differ, the files win. The
test `crates/umbra-server/tests/docs_api.rs` checks the tables of this page against them (see "Keeping
this page true").

## Starting the server
```
umbra serve [--bind ADDR] [--scenario PATH [--clock CLOCK] [--seed SEED] [--start]]
            [--scenario-root DIR] [--plugin-path DIR].. [--config FILE] [--log-level LEVEL]
```
It prints one line on stdout, `listening on http://<addr>` (the bound address, so `--bind 127.0.0.1:0`
picks a free port), and runs until SIGINT or SIGTERM. The default address is `127.0.0.1:7878`. The server
is part of the default `server` feature of `umbra-cli`; `cargo build -p umbra-cli --no-default-features`
builds the engine-only CLI without it (gate 16).

## Versioning and compatibility
- All routes are under `/api/v1`. `info.version` in `api/openapi.json`, `api_version` in `GET /api/v1/status`
  and in the stream `hello` is the API semver (`1.1.0`).
- Within v1 every change is additive and bumps the minor version: a new endpoint, an optional request
  field, a response field, an enum value, a problem code, a stream message type. Clients must ignore
  unknown fields, enum values, event kinds and message types.
- Removing or changing anything is a breaking change: it needs an ADR and a `/api/v2`.
- Changelog: 1.1.0 adds the `network_adapter` value of `ComponentKind` (M6 WI-3; `GET /components` lists
  every registry kind); M6 WI-17 (adapter endpoints) ships under the same 1.1.0.
- After a contract change, `frontend-engineer` runs the `generate-api-client` skill.

## Determinism
The API never changes simulation results by itself. Per-tick hashes with and without stream subscribers
are identical (snapshots are read through `&self`). Commands apply between ticks, in arrival order. In
`afap`, `realtime` and `scaled` modes the tick at which a command applies depends on when it arrives, so
only a stepped clock (or a paused run) gives a scripted run that reproduces. A scenario that omits
`tick_rate_hz` runs at the configured default, so `tick_rate_hz` and `tick_rate_source` (`scenario` or
`default`) are reported by `GET /api/v1/status`, the scenario info and every run status.

## No authentication
Version 1 has no authentication and no authorisation (REQ-X-8, spec 1.3). The server binds `127.0.0.1`
by default. A non-loopback `bind` (flag or `[server] bind`) must be given explicitly and logs a warning
that the API has no authentication. Any client that can reach the port can load scenarios (which can load
native plugins), start and stop runs. Do not expose it to an untrusted network. Every mutating route goes
through one router layer, so a later auth layer can wrap it without touching handlers.

## Confinement, `Host` and `Origin`
- **Scenario root.** `POST /api/v1/scenario` accepts only paths inside the scenario root (`--scenario-root`,
  `[server] scenario_root`, default the working directory at startup). A relative `path` resolves against
  the root, never the working directory; an absolute path must be inside it. The path is normalised
  lexically first, then canonicalised (symlinks). Outside the root is `403 path_not_allowed`, whether or
  not the file exists, so there is no existence oracle. A missing file inside the root is
  `422 scenario_invalid` with an `io` diagnostic; a directory is `422 validation_failed`.
- **Everything the load reads is confined too:** `template_paths`, template and graph files and their
  symlinks must stay inside the root; a scenario's `plugin_paths` must stay inside the root or inside one
  of the configured plugin directories. The first violation is `403 path_not_allowed`.
- **No absolute paths in responses.** Paths in `ScenarioInfo`, diagnostics and template sources are
  relative to the root; a plugin `origin` is relative to the root that holds it.
- **`Host` allow-list.** A request is accepted only if its `Host` is a loopback name or address
  (`localhost`, `127.0.0.0/8`, `[::1]`, any port), the bound address, or listed in `[server] allowed_hosts`.
  Anything else is `403 host_not_allowed` (DNS-rebinding protection). A non-loopback bind such as
  `0.0.0.0` therefore needs `allowed_hosts` for the names clients use.
- **`Origin` on the stream upgrade.** An upgrade that carries `Origin` must come from a loopback host (any
  port, `http` or `https`) or match `[server] allowed_origins` exactly; otherwise `403 origin_not_allowed`.
  An upgrade without `Origin` (a non-browser client) is allowed. REST has no CORS: JSON bodies need
  `Content-Type: application/json` (`415 unsupported_media_type` otherwise).
- **Tauri origins** (`tauri://localhost`, `http://tauri.localhost`) are M8 work (an `allowed_origins` entry
  and, if needed, CORS for REST). They are not allowed today.

## Run states
`RunState` is reported by status, every run-control reply and every stream frame.

| State | Meaning |
|---|---|
| `idle` | No scenario is loaded |
| `ready` | A scenario is loaded and paused at tick 0 |
| `running` | Ticking (or, with a stepped clock, accepting `step`) |
| `paused` | Loaded and not ticking |
| `stopped` | Ended; `stop_reason` is `command`, `duration` (the scenario's `duration_ticks` was reached), `plugin_fault` (with `fault`) or `error` |

`idle -> ready -> running <-> paused`, and any loaded state `-> stopped`. `load` replaces any session (a
new `session` number); `unload` returns to `idle`. Queries work in every loaded state, `stopped` included.

## Commands and states
Each cell is the response of the command in that state. "ok" means it applies. A second `start` is `409`,
a second `DELETE /scenario` is `409 no_scenario`, a second `DELETE` of an entity is `404`: there are no
idempotency keys. Two clients racing one command get one success and one `409`.

| Command | `idle` | `ready` | `running` | `paused` | `stopped` |
|---|---|---|---|---|---|
| load | ok -> `ready` | ok (replace) | ok (replace) | ok (replace) | ok (replace) |
| unload | `no_scenario` | ok -> `idle` | ok | ok | ok |
| start | `no_scenario` | ok -> `running` | `invalid_state` | `invalid_state` | `invalid_state` |
| pause | `no_scenario` | `invalid_state` | ok -> `paused` | `invalid_state` | `invalid_state` |
| resume | `no_scenario` | `invalid_state` | `invalid_state` | ok -> `running` | `invalid_state` |
| stop | `no_scenario` | ok -> `stopped` | ok | ok | `invalid_state` |
| step | `no_scenario` | `invalid_state` | ok if stepped, else `not_stepped`; `step_in_progress` if one is pending | `invalid_state` | `invalid_state` |
| set clock | `no_scenario` | ok | ok | ok | `invalid_state` |
| spawn, remove | `no_scenario` | ok | ok (between ticks) | ok | `invalid_state` |
| entity and template queries | `no_scenario` | ok | ok | ok | ok |
| status, catalog, components, plugins, validate | ok | ok | ok | ok | ok |

All error entries are `409` problems.

**Loading replaces the session (D-a).** `POST /api/v1/scenario` drops the old session first, then loads
the new one. If the load fails (`403`, `422`), the server is `idle` and the previous session is gone.
Loading the same plugin again starts a new plugin lifecycle (`init` runs again).

**Step.** `POST /api/v1/run/step` needs `running` with a stepped clock. It answers after `count` ticks
ran. `200` with `completed: true` when all ran. `200` with `completed: false` and the reached tick when
the duration or a `stop` ended the run first. `409 step_cancelled` only for pause, unload, a clock change
away from stepped, or a plugin fault. Another step while one is pending is `409 step_in_progress`. The
reply carries `state_hash` and the `run` status after the step; the frame of the last tick is already in
every stream subscription when the reply arrives (publish, then reply).

## Endpoints
Successful status in the third column. Request bodies are JSON (`application/json`); every non-2xx
response is a problem (next section).

| Endpoint | Operation id | Success | Purpose |
|---|---|---|---|
| `GET /api/v1/status` | `getStatus` | 200 | Contract version, run status, loaded scenario |
| `POST /api/v1/scenario` | `loadScenario` | 201 | Load a scenario (replaces the session); body `path`, optional `clock`, optional `seed` |
| `GET /api/v1/scenario` | `getScenario` | 200 | The loaded scenario (`ScenarioInfo`) |
| `DELETE /api/v1/scenario` | `unloadScenario` | 204 | Unload; the run returns to `idle` |
| `POST /api/v1/run/start` | `startRun` | 200 | `ready` to `running` |
| `POST /api/v1/run/pause` | `pauseRun` | 200 | `running` to `paused`; cancels a pending step |
| `POST /api/v1/run/resume` | `resumeRun` | 200 | `paused` to `running` |
| `POST /api/v1/run/stop` | `stopRun` | 200 | Any loaded state but `stopped` to `stopped` (reason `command`) |
| `POST /api/v1/run/step` | `stepRun` | 200 | Run `count` ticks of a stepped clock |
| `PUT /api/v1/run/clock` | `setClock` | 200 | Change the clock in any loaded state but `stopped` |
| `GET /api/v1/entities` | `listEntities` | 200 | A page of entities, filtered |
| `POST /api/v1/entities` | `spawnEntity` | 201 | Spawn from a template of the scenario; `Location` names the entity |
| `GET /api/v1/entities/{id}` | `getEntity` | 200 | One entity |
| `DELETE /api/v1/entities/{id}` | `removeEntity` | 204 | Remove an entity; ids are never reused |
| `GET /api/v1/templates` | `listTemplates` | 200 | Templates of the loaded scenario |
| `GET /api/v1/templates/{id}` | `getTemplate` | 200 | One template with inheritance applied |
| `GET /api/v1/components` | `listComponents` | 200 | Registered components, optional `kind` |
| `GET /api/v1/catalog/behavior` | `getBehaviorCatalog` | 200 | Behavior nodes, conditions, actions and graphs |
| `POST /api/v1/behaviors/validate` | `validateBehavior` | 200 | Validate a graph document; diagnostics are the result (`valid: false`), not an error |
| `GET /api/v1/plugins` | `getPlugins` | 200 | Plugins of the loaded scenario and the fault that stopped the run |
| `GET /api/v1/openapi.json` | `getOpenApi` | 200 | The OpenAPI document (equal to `api/openapi.json`) |
| `GET /api/v1/stream` | `openStream` | 101 | WebSocket upgrade; see "The stream" |

There is no `GET /api/v1/scenarios` (listing loadable scenarios under the root) yet.

### Seeds and clocks
- **Seed.** Responses always carry `seed` as a decimal string. Requests accept a decimal string or a JSON
  integer up to 2^53 - 1; a larger number is `422 validation_failed` (send it as a string).
- **Clock.** An object with `mode`: `{"mode":"afap"}`, `{"mode":"realtime"}`, `{"mode":"stepped"}` or
  `{"mode":"scaled","scale":2.0}` (scale 0.001 to 1000000).
- There is no request field for the tick rate: an explicit `tick_rate_hz` in the scenario applies, else the
  configured default (60 Hz built in).

### Paging and query styles
- `GET /api/v1/entities` pages with the keyset cursor `after_id` (exclusive) and `limit` (1 to 1000,
  default 100) and returns `total`, `tick` and `next_after_id` (`null` on the last page). Ids are monotone
  and never reused, so the cursor is stable. Pages of one listing may come from different ticks; each
  carries its `tick`. There is no `offset`.
- Filters: `ids`, `name`, `force`, `group`, `template`, `ownership` (`owned` or `remote`).
- Array parameters (`ids`) accept the repeated form (`ids=1&ids=2`) and the comma-separated form
  (`ids=1,2`). The server accepts both the repeated and the comma form, so the GUI's orval-generated client (`@umbra/data`) works with its default style.
- A bounding box is four scalars, `min_lat_deg`, `min_lon_deg`, `max_lat_deg`, `max_lon_deg`: give all four
  or none (`400 bad_request`). `min_lon_deg` larger than `max_lon_deg` crosses the antimeridian.
- `GET /api/v1/components?kind=` takes `motion_model`, `terrain`, `behavior`, `sensor`, `condition`, `action` or
  `network_adapter`.

## Error codes
Every non-2xx response is `application/problem+json`:
`{"type":"urn:umbra:problem:<code>","title":...,"status":...,"detail":...,"code":"<code>","diagnostics"?:[...]}`.
`code` is stable, snake_case and maps to exactly one status. A bare framework error body never reaches a
client: unparseable bodies, bad query strings, unknown routes and wrong methods are all problems. New
codes are a minor bump; clients must tolerate unknown ones.

| Code | Status | Meaning |
|---|---|---|
| `bad_request` | 400 | A query or path parameter is invalid |
| `invalid_body` | 400 | The body is not parseable JSON |
| `unsupported_encoding` | 400 | The stream `encoding` is not `json` |
| `path_not_allowed` | 403 | A path leaves the scenario root or the plugin directories |
| `host_not_allowed` | 403 | The `Host` header is not allowed |
| `origin_not_allowed` | 403 | The stream `Origin` is not allowed |
| `not_found` | 404 | No such route |
| `entity_not_found` | 404 | No such entity |
| `template_not_found` | 404 | No such template |
| `method_not_allowed` | 405 | The method is not allowed on this route |
| `no_scenario` | 409 | No scenario is loaded |
| `invalid_state` | 409 | The command is not valid in the current run state |
| `not_stepped` | 409 | The clock is not stepped |
| `step_in_progress` | 409 | Another step is pending |
| `step_cancelled` | 409 | The step was cancelled before it completed |
| `duplicate_name` | 409 | An entity with this name exists |
| `payload_too_large` | 413 | The body exceeds `[server] body_limit_bytes` (1 MiB) |
| `unsupported_media_type` | 415 | The body needs `Content-Type: application/json` |
| `validation_failed` | 422 | Well formed but invalid: unknown field, wrong type, out of range |
| `invalid_id` | 422 | An id is not valid |
| `invalid_spawn` | 422 | The spawn parameters are invalid |
| `scenario_invalid` | 422 | The scenario has errors; `diagnostics` locate them |
| `scenario_load_failed` | 422 | The scenario or its plugins could not be loaded |
| `internal` | 500 | Internal error |
| `engine_busy` | 503 | The engine command queue is full (capacity `[server] command_queue`) |
| `engine_unavailable` | 503 | The engine thread has ended; permanent until the server restarts |
| `too_many_connections` | 503 | `[server.stream] max_connections` stream connections are open |

## The stream
`GET /api/v1/stream?encoding=json` upgrades to a WebSocket (protocol `umbra.stream.v1`). `encoding`
defaults to `json`; `msgpack` and anything else are refused before the upgrade with
`400 unsupported_encoding`. Messages are text frames, one JSON object each, discriminated by `type`. The
schema is `api/stream.schema.json`. There is no `unsubscribe`: a client that is done closes the socket.

**Server to client.**
- `hello`: first message. `protocol`, `api_version`, `encoding` and `settings` (the limits in force:
  `default_rate_hz`, `max_rate_hz`, `queue_frames`, `max_events_per_frame`, `send_timeout_s`,
  `max_message_bytes`, `max_entity_ids`).
- `frame`: `kind` (`snapshot`), `cause` (`tick` or `control`), `session`, `seq`, `tick`, `time_s`, `run`
  (the run status), `dropped`, `entities` (the same `EntitySummary` as `GET /api/v1/entities`, ascending by
  id, numbers bit-identical to REST for the same tick), `events`, `events_truncated`, optional `state_hash`.
- `error`: `code` (`invalid_message`, `unknown_message_type`, `invalid_subscription`) and `message`, for a
  bad client message; the connection stays open.

**Client to server.** `subscribe`, all fields optional: `filter` (`bbox` with the four `*_deg` edges,
`entity_ids` at most 10,000, `event_kinds`), `max_rate_hz` (in (0, 60], default 30), `state_hash`
(default false). A new `subscribe` replaces the previous one. Until the first `subscribe` only `hello` is
sent. Unknown fields are refused with an `error`.

**Priming.** A `subscribe` immediately gets one `cause: "control"` frame of the current state, even when
nothing ticks (`idle`, `ready`, `paused`, `stopped`). Control frames are also published after every
state-changing command (load, unload, spawn, remove, start, pause, resume, stop, set clock), so a client
sees the entities right after a load or a spawn without a tick, and the frame of a `step` completion, a
pause and a stop is always delivered. Control frames carry `state_hash` when a subscription asked for it.

**Rate.** The simulation tick rate is independent of the stream rate. A connection with `max_rate_hz` r on
a scenario of `tick_rate_hz` t gets a frame every `max(1, t / r)` ticks (integer division), and the engine
builds no frame for it in between. A 30 Hz scenario at `max_rate_hz: 10` gets every third tick.

**Slow clients (REQ-X-7, REQ-K-6).** A slow client never slows the tick; memory is bounded.
1. Each connection has an engine-side queue of `queue_frames` (4) frames; on overflow the oldest is
   dropped.
2. The connection task sends only the newest queued frame when the socket is ready, then waits
   `1 / max_rate_hz` of wall time.
3. `dropped` is the number of frames never sent since the previous frame; `seq` is numbered per
   subscription from 0 at the priming frame and grows by `dropped + 1` between sent frames, so a gap in
   `seq` is exactly a drop.
4. **Events are carried over.** The events of dropped or skipped frames are concatenated, in order, into
   the next frame, up to `max_events_per_frame` (4096). Beyond that `events_truncated` is true. Events are
   best-effort; a lossless event log is a later (M7 recording) feature.
5. A send that does not finish within `send_timeout_s` (10) closes the connection with code 1008 and
   reason `too_slow` (when the close frame can still be delivered; otherwise the connection is dropped).

**Limits.** A client message over 64 KiB closes with 1009. `[server.stream] max_connections` (64) open
streams; the next upgrade is `503 too_many_connections`. If the engine thread ends, open streams close
with 1011.

**Events** (`EventDto`): `tick`, `kind` and optional `entity_id`, `target_id`, `template`, `weapon`,
`detail`. Kinds today: `entity_created` (API spawns only; the initial load emits none, its control frame
lists the entities), `entity_removed` (API removes), `fire` and `custom` (from behavior and plugin `Fire`
and `Custom` intents), `simulation_control` (start, pause, resume, stop, step). `Spawn` and `Despawn`
intents from behaviors are recorded but not applied until M7. Unknown kinds must be ignored.

## Shutdown
SIGINT or SIGTERM (Ctrl-C on Windows) starts a graceful shutdown: new connections stop, pending `step`
requests are answered `503 engine_unavailable`, open streams close with 1001, and the engine thread is
joined, all within `[server] shutdown_grace_s` (5 s). Plugins are shut down on the engine thread. The
process exits 0 after a clean shutdown. A tick that never returns cannot hang the process: past the grace
period the engine thread is abandoned and a warning is logged.

## Configuration reference
`umbra serve` reads the same `umbra.toml` as the other commands (ADR 0029): `--config FILE` (a missing file
is exit 1), else `./umbra.toml` if it exists, else built-in defaults. Precedence per setting is
flag > file > default; lists are replaced, not merged. Unknown keys are located `unknown_key` errors. Every
key is optional. Relative paths resolve against the file's directory. `umbra schema config` prints the
JSON Schema (`crates/umbra-templates/schemas/config.v1.schema.json`).

| Key | Default | Meaning |
|---|---|---|
| `schema_version` | 1 | Optional; a newer value is `unsupported_schema_version` |
| `[simulation] tick_rate_hz` | 60 | Tick rate of scenarios that omit `tick_rate_hz`; whole hertz; an explicit scenario value wins |
| `[plugins] paths` | none | Plugin directories, before each scenario's own `plugin_paths`; `--plugin-path` replaces them |
| `[log] level` | `warn` | `off`, `error`, `warn`, `info`, `debug` or `trace`; `--log-level` wins |
| `[server] bind` | `127.0.0.1:7878` | One socket address (`[::1]:7878` for IPv6); `--bind` replaces it whole |
| `[server] scenario_root` | `.` | Loads are confined here |
| `[server] body_limit_bytes` | 1048576 | Largest request body |
| `[server] command_queue` | 64 | Engine command channel capacity; full is `engine_busy` |
| `[server] allowed_hosts` | none | Extra `Host` values, names or addresses, port optional |
| `[server] allowed_origins` | none | Extra stream origins, exact `scheme://host[:port]` |
| `[server] shutdown_grace_s` | 5 | Graceful-shutdown deadline, seconds |
| `[server.stream] default_rate_hz` | 30 | Used when `subscribe` gives no `max_rate_hz`; at most `max_rate_hz` |
| `[server.stream] max_rate_hz` | 60 | The highest rate a subscriber may ask for; never above 60 |
| `[server.stream] queue_frames` | 4 | Per-connection queue capacity |
| `[server.stream] max_events_per_frame` | 4096 | Events per frame before `events_truncated` |
| `[server.stream] send_timeout_s` | 10 | A longer send ends the connection (`1008 too_slow` when deliverable, otherwise dropped) |
| `[server.stream] max_connections` | 64 | Concurrent stream connections |

Example, a server for a LAN with a larger body limit and a lower stream ceiling:

```toml
schema_version = 1

[simulation]
tick_rate_hz = 30

[plugins]
paths = ["plugins"]

[log]
level = "info"

[server]
bind = "0.0.0.0:7878"
scenario_root = "scenarios"
body_limit_bytes = 4194304
allowed_hosts = ["umbra.lan"]
shutdown_grace_s = 10.0

[server.stream]
max_rate_hz = 30
max_connections = 16
```

A non-loopback `bind` logs the no-authentication warning on stderr.

## Examples
Captured from `umbra serve --bind 127.0.0.1:7878 --scenario-root examples` on loopback (the server was
stopped afterwards); long values are shortened with `...`. The scenario is
`examples/scenarios/walking_skeleton/scenario.toml`.

```
$ curl -s http://127.0.0.1:7878/api/v1/status
{"api_version":"1.1.0","run":{"state":"idle","clock":{"mode":"realtime"},"session":0,"step_pending":false}}

$ curl -s -X POST http://127.0.0.1:7878/api/v1/scenario -H 'content-type: application/json' \
    -d '{"path":"scenarios/walking_skeleton/scenario.toml","clock":{"mode":"stepped"},"seed":"42"}'
{"session":1,"name":"walking_skeleton","path":"scenarios/walking_skeleton/scenario.toml","seed":"42","tick_rate_hz":30,"tick_rate_source":"scenario","duration_ticks":7200,"entity_count":3}

$ curl -s -X POST http://127.0.0.1:7878/api/v1/run/start
{"state":"running","clock":{"mode":"stepped"},"session":1,"tick":0,"time_s":0.0,"step_pending":false,"tick_rate_hz":30,"tick_rate_source":"scenario"}

$ curl -s -X POST http://127.0.0.1:7878/api/v1/run/step -H 'content-type: application/json' -d '{"count":30}'
{"requested":30,"completed":true,"reached":30,"time_s":1.0,"state_hash":"385bab74cfc4ba03","run":{"state":"running",...,"tick":30,"time_s":1.0,...}}

$ curl -s 'http://127.0.0.1:7878/api/v1/entities?limit=2'
{"tick":30,"total":3,"items":[{"id":1,"name":"base-1","template":"ground_vehicle.base","group":"alpha","ownership":"owned","lat_deg":52.00000000006294,"lon_deg":4.000018807571051,"alt_m":0.0,"heading_deg":89.99969538903314,"speed_mps":2.499999999999999,"route":{"next":0,"len":4}},{"id":2,...}],"next_after_id":2}

$ curl -s 'http://127.0.0.1:7878/api/v1/entities?ids=1,3'     # also ids=1&ids=3
{"tick":30,"total":2,"items":[{"id":1,...},{"id":3,"name":"truck-1",...}],"next_after_id":null}

$ curl -s -i -X POST http://127.0.0.1:7878/api/v1/entities -H 'content-type: application/json' \
    -d '{"template":"truck.cargo","lat_deg":52.01,"lon_deg":4.01,"name":"truck-2"}'
HTTP/1.1 201 Created
location: /api/v1/entities/4
{"summary":{"id":4,"name":"truck-2","template":"truck.cargo","ownership":"owned",...},"motion_model":"builtin.ground.kinematic","contacts":[]}

$ curl -s -X PUT http://127.0.0.1:7878/api/v1/run/clock -H 'content-type: application/json' -d '{"mode":"scaled","scale":2.0}'
{"state":"running","clock":{"mode":"scaled","scale":2.0},"session":3,"tick":3,...}
```

Errors (problem details):

```
$ curl -s -X POST http://127.0.0.1:7878/api/v1/run/start          # already running
{"type":"urn:umbra:problem:invalid_state","title":"The command is not valid in the current run state","status":409,"detail":"start is not valid while the run is running","code":"invalid_state"}

$ curl -s -X POST http://127.0.0.1:7878/api/v1/scenario -H 'content-type: application/json' -d '{"path":"../etc/passwd"}'
{"type":"urn:umbra:problem:path_not_allowed","title":"The path is outside the allowed directories","status":403,"detail":"path not allowed: ../etc/passwd","code":"path_not_allowed"}
# the failed load also dropped the session: the server is idle now (D-a)

$ curl -s -H 'Host: evil.example' http://127.0.0.1:7878/api/v1/status
{"type":"urn:umbra:problem:host_not_allowed",...,"status":403,"detail":"the Host header is not allowed (see [server] allowed_hosts)","code":"host_not_allowed"}

$ curl -s 'http://127.0.0.1:7878/api/v1/stream?encoding=msgpack'
{"type":"urn:umbra:problem:unsupported_encoding",...,"status":400,"detail":"`encoding=msgpack` is not supported; the stream speaks `json`","code":"unsupported_encoding"}
```

A stream session (a Bun script; any WebSocket client works). The scenario was loaded with a stepped clock
and started; the client subscribed to entities 1 and 2 at 10 Hz with hashes, sent a bad message, and
another client ran `step` with `count` 3. The scenario runs at 30 Hz, so a 10 Hz subscription sees every
third tick:

```
<- {"type":"hello","protocol":"umbra.stream.v1","api_version":"1.1.0","encoding":"json","settings":{"default_rate_hz":30.0,"max_rate_hz":60.0,"queue_frames":4,"max_events_per_frame":4096,"send_timeout_s":10.0,"max_message_bytes":65536,"max_entity_ids":10000}}
-> {"type":"subscribe","max_rate_hz":10,"state_hash":true,"filter":{"entity_ids":[1,2]}}
<- {"type":"frame","kind":"snapshot","cause":"control","session":3,"seq":0,"tick":0,"time_s":0.0,"run":{"state":"running","clock":{"mode":"stepped"},"session":3,"tick":0,...},"dropped":0,"entities":[{"id":1,"name":"base-1",...},{"id":2,"name":"tank-1",...}],...}
-> {"type":"nonsense"}
<- {"type":"error","code":"unknown_message_type","message":"unknown message type `nonsense`; the only client message is `subscribe`"}
<- {"type":"frame","kind":"snapshot","cause":"tick","session":3,"seq":1,"tick":3,"time_s":0.1,"run":{"state":"running",...,"step_pending":true,...},"dropped":0,"entities":[{"id":1,...,"lat_deg":52.000000000000824,"lon_deg":4.000000242678336,...,"speed_mps":0.25,...}],...}
```

And the refusals of the upgrade:

```
$ curl -s -i -H 'Origin: http://evil.example' -H 'Connection: Upgrade' -H 'Upgrade: websocket' \
    -H 'Sec-WebSocket-Version: 13' -H 'Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==' http://127.0.0.1:7878/api/v1/stream
HTTP/1.1 403 Forbidden
content-type: application/problem+json
{"type":"urn:umbra:problem:origin_not_allowed",...,"code":"origin_not_allowed"}
```

## Notes for the GUI (M5b and M8)
- `GET /api/v1/scenarios` (listing loadable scenarios under the root) does not exist. It is a later
  additive item for M5b or M8; until then the GUI asks the user for a scenario path under the root.
- The GUI client is generated by orval into `@umbra/data` (TanStack Query hooks); `ids` accepts its repeated array style unchanged.
- The Vite dev proxy (loopback origin, any port) passes the `Origin` check. The Tauri origins are M8.
- Generate the TypeScript client from the three committed files with the `generate-api-client` skill; never
  hand-edit generated files.

## Keeping this page true
`crates/umbra-server/tests/docs_api.rs` (gate 3) parses this page with one rule: in the section of the
named heading, every table row whose first cell is a single code span is read.
- `## Endpoints`: the cell is `METHOD /path`; the second cell is the operation id. The set of
  (method, path, operation id) equals the paths of `api/openapi.json`, in both directions.
- `## Error codes`: the cell is a code and the second cell its status. The set equals `ProblemCode::ALL`
  with the same statuses, in both directions.
- `## Run states`: the cell is a state. The set equals the `RunState` enum of `api/openapi.json`.
- `## Configuration reference`: the cell is `[table] key` (or a bare key at the top level). The set equals
  the leaf keys of `config.v1.schema.json`, in both directions.

When a route, a code or a config key changes, update the tables here in the same change.
