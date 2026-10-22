# ADR 0058: Control API: engine log records and scenario listing (GUI gaps G-1, G-2), in API 1.2.0

- **Status**: Proposed (architect, M7 plan `docs/plans/M7.md`, 2026-10-22). It follows the human decision
  H-10 of the M8 plan (2026-10-22): schedule G-1 and G-2 as low-priority, additive engine work items in M7.
- **Requirement IDs**: REQ-X-3, REQ-X-4, REQ-X-5, REQ-X-7, REQ-X-8, REQ-X-9, REQ-K-4, REQ-K-6, NFR-4, D-44,
  D-45, ADR 0026, ADR 0027, ADR 0057
- **Affected**: `umbra-api-types` (`LogRecordDto`, `LogPage`, `ScenarioEntryDto`, `ScenarioList`),
  `umbra-server` (`LogRing`, two handlers), `umbra-engine` (`list_scenarios` over `Confinement`),
  `umbra-templates` (additive `scenario_name` helper reusing `is_scenario`), `umbra-cli` (a tracing
  layer feeding the ring in `serve`), committed `api/openapi.json`, `docs/api.md`

## Context
The M8 GUI needs two things the contract does not give it (M8 plan, contract gaps):
- **G-1, a log view of engine log lines.** These are the `tracing` records: plugin load warnings, adapter
  errors, scenario warnings.
- **G-2, a list of loadable scenarios.** Today the user must type a path.

Both are reads only; they change no state. Logs are never hashed and never read by the simulation, so
determinism (REQ-K-4) is unaffected. The constraints:
- loads are confined to the scenario root, with no existence oracle and paths never shown absolute
  (ADR 0026, `Confinement::{resolve_request, display, scrub}`);
- a slow client must never slow the tick (REQ-X-7, REQ-K-6);
- the server binds to localhost by default and has no authentication (REQ-X-8).

M7 already changes the contract once (API 1.2.0, ADR 0057). That WI also makes the approved edit of the
frozen test `cli_serve.rs` (`api_version` 1.2.0, M7 H8).

## Options
For G-1:
1. **REST pull from a bounded ring (chosen):** `GET /api/v1/logs?after_seq=&limit=&min_level=`.
   - Pros: no change to the stream protocol or its slow-client policy. The producer never waits; gaps are
     visible through `seq` and a `dropped` count.
   - Cons: the client polls (about 1 s is enough for a log view).
2. **A stream message `log` with a subscription option.**
   - Pros: push.
   - Cons: grows the stream schema, `subscribe` and the slow-client rules (ADR 0027) for a low-priority
     view. It can be added later on top of the same ring.

For G-2:
1. **`GET /api/v1/scenarios`, a bounded walk under the scenario root (chosen).** Scenario files are
   recognised the same way the loader recognises them (`is_scenario`: a top-level `[scenario]` table).
2. **List every `*.toml`.**
   - Cons: shows templates, `umbra.toml` and `plugin.toml` files, and the client would have to guess.

For the version:
1. **Fold both into API 1.2.0 (chosen).** 1.2.0 is the M7 contract version, unreleased until `m7` merges
   into `develop`. Additions within the branch keep it, so `cli_serve.rs` needs no second frozen edit.
2. **A separate 1.3.0.**
   - Cons: a second edit of the frozen `cli_serve.rs` assertion, for a version no client outside the branch
     ever sees.

## Decision
REST ring for G-1, bounded walk for G-2, both in API 1.2.0.

### G-1: engine log records
- **Ring:** `LogRing` in `umbra-server`, std only (no new dependency).
  - It is a bounded ring of 2048 records; the oldest are overwritten.
  - `seq` is a `u64` that grows monotonically from 1. `push` takes a short mutex and never waits on a
    reader.
  - The `serve` command of `umbra-cli` installs a `tracing_subscriber::Layer` that pushes into the ring
    (`tracing-subscriber` is already a CLI dependency).
  - The ring captures `info` and above by default. `[server] log_buffer_level` in `umbra.toml` is an
    additive key and cannot go below `debug`. The ring never captures `trace`.
- **Record:** `LogRecordDto { seq, level: "error"|"warn"|"info"|"debug", target, message, tick?: u64,
  plugin?: String }`.
  - `message` is passed through `Confinement::scrub`, so no absolute path leaves the server.
  - `message` is truncated at a character boundary to 4 KiB.
  - Fields other than `message`, `plugin` and `tick` are dropped.
- **Endpoint:** `GET /api/v1/logs?after_seq=0&limit=200&min_level=info`.
  - `limit` is at most 1000.
  - It returns `LogPage { records (ascending seq), next_seq, first_available_seq, dropped }`.
  - `dropped` counts records overwritten after `after_seq` before they were read.
  - Bad query values are `400 bad_request` (the existing code).
  - The handler reads the ring on the async worker, never through the engine thread.

### G-2: scenario listing
- **Function:** `umbra_engine::list_scenarios(&Confinement) -> ScenarioListing`.
  - It walks the scenario root breadth-first with depth at most 8.
  - It never follows symbolic links (files or directories), skips hidden entries and `target/`, and
    visits names in UTF-8 byte order.
  - It reads at most 2000 `*.toml` files of at most 1 MiB each, skipping `*.behavior.toml`.
  - It keeps the files that parse and pass `umbra_templates::scenario_name`, a new public helper that
    returns `Some(name)` for a scenario file (`[scenario] name`, else the file stem).
  - It stops at 1000 entries and sets `truncated`.
- **Endpoint:** `GET /api/v1/scenarios` returns `ScenarioList { scenarios: [ScenarioEntryDto { path, name }],
  truncated }`, sorted by `path`.
  - `path` is the `Confinement::display` form (relative, `/` separators), so it is exactly what
    `POST /scenario` accepts.
  - The handler runs the walk on a blocking thread (`spawn_blocking`), never on the engine thread.
  - Unreadable entries are skipped silently, so there is no oracle beyond what is listed.

### Both
- Both endpoints are read-only and work with or without a loaded session.
- `docs/api.md` documents the localhost-only posture: logs can reveal plugin and template ids (REQ-X-8).

## Consequences
- **Easier:** the M8 log view and scenario picker work in the web build without the desktop's stdout
  fallback.
- **Harder:**
  - Contract and docs grow by two routes. The GUI client regenerates once per WI.
  - `API_VERSION` stays 1.2.0 while its content grows on the `m7` branch, so it is final only at the merge
    into `develop` (documented in `docs/api.md`).
- **Revisit:**
  - when a push log stream is wanted (add a stream `log` message over the same ring);
  - for very large scenario trees (paging);
  - with access control (spec open question 4).
