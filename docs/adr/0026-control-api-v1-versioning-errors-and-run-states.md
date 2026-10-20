# ADR 0026: Control API v1: versioning, run states, error model and exposure

- **Status**: Accepted by the human 2026-10-19 (proposed by the architect, M5 plan `docs/plans/M5.md`; awaiting the human project owner)
- **Requirement IDs**: REQ-X-1, REQ-X-4, REQ-X-5, REQ-X-8, REQ-X-9, REQ-X-10, REQ-X-11, REQ-X-12, REQ-T-7,
  REQ-G-1, NFR-4, spec 1.3 (no authentication), spec section 19 question 4, D-11, D-12
- **Affected**: `umbra-api-types`, `umbra-server`, `umbra-engine` (`HandleError`), `umbra-cli` (`serve`)

## Context
The spec fixes `/api/v1/`, problem-details errors with stable codes (REQ-X-9), localhost by default with no
command restrictions or authentication in v1 (REQ-X-8, spec 1.3), and lists minimum endpoints (REQ-X-5).
It does not define the run lifecycle (`start` versus `resume`, what `stop` leaves behind), how versions
evolve, which files a client may make the engine load, or what happens at the scenario's duration.
Open question 4 asks whether command access control is needed.

## Options
- **Versioning**: (a) URL major `/api/v1` plus an API semver in `info.version`, additive changes only
  within v1 (chosen); (b) header or media-type versioning (invisible in generated clients, harder to test).
- **Lifecycle**: (a) explicit states with `409` for invalid transitions (chosen); (b) every command
  always succeeds and is coerced (`step` while running auto-pauses, etc.): surprising, hard to test.
- **Scenario paths**: (a) any path the server process can read; (b) paths confined to a scenario root
  given at startup (chosen): loading a scenario can load native plugins (ADR 0022), so the path is a
  code-execution input even on localhost.
- **Access control**: (a) none in v1, with one seam to add it (recommended, matches REQ-X-8); (b) a
  read-only/operator token now (new scope, contradicts REQ-X-8 and spec 1.3).

## Decision
- **Versioning.** All routes under `/api/v1`. `API_VERSION` (semver, starts `1.0.0`) is `info.version`
  and appears in `GET /api/v1/status` and the stream `hello`. Adding an endpoint, an optional request
  field, a response field, an enum value or a stream message type is a minor bump; clients must ignore
  unknown fields, enum values, event kinds and message types (documented). Removing or changing anything
  is a breaking change: a new ADR and `/api/v2`.
- **Run states** (`RunState`, reported by status and every frame): `idle` (no scenario) -> `ready`
  (loaded, paused at tick 0) -> `running` <-> `paused`; any loaded state -> `stopped` with a
  `stop_reason`: `command`, `duration` (the scenario's `duration_ticks` reached), `plugin_fault` (ADR
  0021 `abort`, with the fault), or `error` (kernel error). `start` is valid in `ready` only, `resume` in
  `paused` only. `step(n)` is valid in `running` with the stepped clock; it replies after the `n` ticks
  ran, or earlier with the reached tick when the run stops; leaving stepped mode, pausing or stopping
  cancels pending steps (`step_cancelled`). `set_clock` is valid in any loaded state except `stopped`.
  `load` replaces any current session (new `session` number); `unload` returns to `idle`. Queries work
  in every loaded state, including `stopped`.
- **Errors (REQ-X-9).** Every non-2xx response is `application/problem+json`:
  `{ "type": "urn:umbra:problem:<code>", "title", "status", "detail", "code", "diagnostics"? }`, where
  `code` is a stable snake_case string from a closed, documented list (`bad_request`, `invalid_body`,
  `payload_too_large`, `not_found`, `no_scenario`, `invalid_state`, `not_stepped`, `step_cancelled`,
  `scenario_invalid` (with located `diagnostics`), `scenario_load_failed`, `path_not_allowed`,
  `entity_not_found`, `template_not_found`, `invalid_id`, `invalid_spawn`, `duplicate_name`,
  `unsupported_encoding`, `engine_busy`, `engine_unavailable`, `internal`). New codes are a minor bump.
  *(Amended 2026-10-19 after the M5 design review: the code list, statuses, confinement, state matrix and
  step results are superseded by the amendment section at the end.)*
  Graph validation (REQ-X-11) returns `200` with a `valid` flag and diagnostics: the diagnostics are the
  result, not an error.
- **Exposure.** Default bind `127.0.0.1:7878`; a non-loopback address must be set explicitly (`--bind`
  or `[server] bind` in `umbra.toml`, ADR 0029) and logs a warning that the API has no authentication.
  Scenario loads accept only paths that canonicalise inside the scenario root (`--scenario-root` or
  `[server] scenario_root`; default: the working directory at startup); others are `403
  path_not_allowed`. Plugin directories are the server's plugin path list (`--plugin-path` or
  `[plugins] paths`) plus the scenario's own `plugin_paths` (ADR 0022 rules). Request bodies are limited
  to 1 MiB by default (`[server] body_limit_bytes`). Precedence for every setting: CLI flag > config
  file > built-in default (ADR 0029).
- **Tick rate on load.** `LoadScenarioRequest` has no tick-rate field: an explicit `tick_rate_hz` in the
  scenario always applies, and a scenario that omits it gets the server's configured default (60 Hz built
  in, ADR 0029). `ScenarioInfo` and `GET /api/v1/status` report `tick_rate_hz` and `tick_rate_source`
  (`scenario` | `default`), so a run can be reproduced.
- **Access control (open question 4).** Not in v1, as REQ-X-8 and spec 1.3 say. Every mutating route is
  registered through one router layer, so a later auth or role layer wraps them without touching
  handlers. Revisit when the Windows app connects to a remote engine (M8) or the API is exposed beyond
  localhost in practice.
- **Scope in M5.** REQ-X-5 endpoints except network adapters (no adapters exist before M6; added there,
  additively); REQ-X-10 and REQ-X-11 in full; REQ-X-12 (SHOULD, graph files) and REQ-T-7 (MAY, hot
  reload) deferred to a separate additive work item before M8b; API spawn takes template, position,
  heading, speed, name, group and force, but no parameter overrides (additive later).

## Consequences
- Easier: generated clients see one stable shape; state errors are explicit and testable; a GUI can show
  `stop_reason` and faults without parsing messages.
- Harder: clients must sequence `load` -> `start` -> `step`; scenarios outside the scenario root need the
  server started with another root.
- Revisit with M6 (adapter endpoints), M7 (replay commands), M8 (remote engines and access control) and
  before M8b (graph file CRUD).

## Amended 2026-10-19 after the M5 design review (human-approved)
These rules supersede the Decision above where they differ.

### Loading
- **`POST /api/v1/scenario` replaces the session (D-a).** The old session is dropped first, then the new
  one is loaded.
  - If the load fails (4xx), the server is `idle` and the previous session is gone. The endpoint
    documentation says so.
  - Loading the same plugin again starts a new plugin lifecycle (ADR 0024 amendment).
- **Confinement (D-b).** Let `R` be the canonical scenario root and `P` the canonical configured plugin
  directories.
  1. A relative request `path` resolves against `R`, never against the working directory. An absolute
     path is accepted only if it falls inside `R`.
  2. The path is first normalised lexically (resolving `.` and `..`). If the result is outside `R`, the
     answer is `403 path_not_allowed`, whether or not the file exists. A missing path outside `R` is
     therefore also 403, never 404 or an `io` diagnostic, so the API gives no existence oracle.
  3. The path is then canonicalised (resolving symlinks). If the canonical path leaves `R`, the answer
     is 403.
  4. A missing file inside `R` is `422 scenario_invalid` with the loader's `io` diagnostic.
  5. The path must name a file. A directory is `422 validation_failed`.
- **Everything the load reads is confined too.** That covers `template_paths` directories, every
  template and graph file, and symlinks among them. They must canonicalise inside `R`. A scenario's
  `plugin_paths` must canonicalise inside `R` or inside one of `P`.
  - Before loading, the engine checks this from a lenient pre-read
    (`umbra_templates::scenario_inputs`). The first violation is `403 path_not_allowed`, naming the
    offending path relative to the scenario file.
  - Terrain needs no check yet: no built-in terrain takes file paths. A future terrain that takes file
    paths (plugin terrain, M7) must use the same check.
  - The check runs before the read, so a symlink swapped in between could escape. This TOCTOU gap is a
    local race on a localhost-only server and is accepted.
- **No absolute paths in responses.** Paths are relative to `R`: `ScenarioInfo.path`, diagnostic paths
  and template sources. A plugin `origin` is shown relative to the root that contains it, with
  `root: "scenario" | "plugins[<i>]"`.

### Host and Origin checks (D-b)
- **`Host` allow-list.** One router layer accepts only these `Host` values:
  - a loopback name or address (`localhost`, `127.0.0.0/8`, `[::1]`) on any port;
  - the bound address;
  - a host listed in `[server] allowed_hosts` (ADR 0029).

  Anything else is `403 host_not_allowed`. This protects against DNS rebinding.
- **`Origin` check on the WebSocket upgrade.**
  - An upgrade with an `Origin` header must match `[server] allowed_origins`. The default allows loopback
    hosts on any port, which covers the Vite proxy during M5b.
  - An upgrade without an `Origin` header (a non-browser client) is allowed.
  - A mismatch is `403 origin_not_allowed`.

  REST needs no `Origin` check: there is no CORS, and JSON bodies require `application/json` (415
  otherwise), so a cross-site browser request either triggers a preflight, which fails, or is refused.
- **Tauri (M8, additive).** The Windows app has a cross-origin origin (`tauri://localhost` or
  `http://tauri.localhost`). Supporting it is an additive M8 item: an `allowed_origins` entry and, if
  needed, CORS for REST.

### Wire format
- **`seed` (D-c).** Responses always carry `seed` as a decimal string. Requests accept either a decimal
  string or a JSON integer up to `2^53 - 1`. Larger seeds must be strings; a larger number is
  `422 validation_failed`.
- **Entity paging (m4).**
  - `GET /api/v1/entities` pages with the keyset cursor `after_id` (exclusive) plus `limit`, and returns
    `next_after_id`. Ids are monotone and never reused, so this cursor is stable. `offset` is gone.
  - Each page carries its `tick`. Pages of one listing may come from different ticks.
- **Query parameters (m5).**
  - Array parameters (`ids`, `event_kinds`) accept both repeated (`ids=1&ids=2`) and comma-separated
    (`ids=1,2`) forms. The server parses the raw query with `form_urlencoded`.
  - `bbox` becomes four scalar parameters: `min_lat_deg`, `min_lon_deg`, `max_lat_deg`, `max_lon_deg`.
    Give all four or none; anything else is `400 bad_request`.
  - `openapi-fetch`'s default array style (repeated) works unchanged for M5b.

### Commands
**Command x state matrix (m3).** Each cell is the response for that command in that state; "ok" means the
command applies. This table is also the AT-4 test table.

| Command | `idle` | `ready` | `running` | `paused` | `stopped` |
|---|---|---|---|---|---|
| load | ok -> `ready` | ok (replace) | ok (replace) | ok (replace) | ok (replace) |
| unload | 409 `no_scenario` | ok -> `idle` | ok | ok | ok |
| start | 409 `no_scenario` | ok -> `running` | 409 `invalid_state` | 409 `invalid_state` | 409 `invalid_state` |
| pause | 409 `no_scenario` | 409 `invalid_state` | ok -> `paused` | 409 `invalid_state` | 409 `invalid_state` |
| resume | 409 `no_scenario` | 409 `invalid_state` | 409 `invalid_state` | ok -> `running` | 409 `invalid_state` |
| stop | 409 `no_scenario` | ok -> `stopped` | ok | ok | 409 `invalid_state` |
| step | 409 `no_scenario` | 409 `invalid_state` | ok if stepped, else 409 `not_stepped`; 409 `step_in_progress` if a step is pending | 409 `invalid_state` | 409 `invalid_state` |
| set clock | 409 `no_scenario` | ok | ok | ok | 409 `invalid_state` |
| spawn, remove | 409 `no_scenario` | ok | ok (between ticks) | ok | 409 `invalid_state` |
| entity, template queries | 409 `no_scenario` | ok | ok | ok | ok |
| status, catalog, components, plugins, validate | ok | ok | ok | ok | ok |

**No idempotency keys.** A second `start` is 409, a second `DELETE /scenario` is 409 `no_scenario`, and
a second `DELETE` of an entity is 404. Two clients racing the same command get one success and one 409.
Commands from different clients apply in arrival order at the engine.

**Step results (M10).** `StepResult { requested, completed, reached, time_s, state_hash, run }` (`run`
is the `RunStatus` after the step; corrected 2026-10-20 by the human to match the M5 plan, which
wins over the earlier wording `status` without `time_s`; M5 WI-6, ASM-172).
- `200` when all `requested` ticks ran (`completed: true`).
- `200` with `completed: false` when the duration or a `stop` command ended the run first.
- `409 step_cancelled` only for pause, unload, a clock change away from stepped, or a plugin fault.
- A `step` while another is pending is `409 step_in_progress`.

### Errors (m7)
**Error codes.** Every code maps to exactly one HTTP status. This list supersedes the one in the
Decision.

| Status | Codes |
|---|---|
| 400 | `bad_request` (query or path parameters), `invalid_body` (not parseable JSON), `unsupported_encoding` |
| 403 | `path_not_allowed`, `host_not_allowed`, `origin_not_allowed` |
| 404 | `not_found` (route), `entity_not_found`, `template_not_found` |
| 405 | `method_not_allowed` (with a body) |
| 409 | `no_scenario`, `invalid_state`, `not_stepped`, `step_in_progress`, `step_cancelled`, `duplicate_name` |
| 413 | `payload_too_large` (from `DefaultBodyLimit`) |
| 415 | `unsupported_media_type` |
| 422 | `validation_failed` (well-formed but invalid: unknown field, wrong type, out of range), `invalid_id`, `invalid_spawn`, `scenario_invalid`, `scenario_load_failed` |
| 500 | `internal` |
| 503 | `engine_busy`, `engine_unavailable`, `too_many_connections` |

**axum rejections.** Custom `Json`, `Query` and `Path` extractors wrap axum's rejections, and a fallback
handler plus a method-not-allowed handler map everything to the codes above. A bare axum error body never
reaches a client.

**Later, additively.** `GET /api/v1/scenarios`, listing loadable scenarios under the root, may be added
for M5b or M8.
