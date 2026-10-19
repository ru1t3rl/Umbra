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
