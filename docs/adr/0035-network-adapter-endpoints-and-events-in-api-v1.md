# ADR 0035: Network adapter endpoints and network events in control API v1 (additive to ADR 0026, 0027)

- **Status**: Accepted by the human 2026-10-20 (proposed by the architect, M6 plan `docs/plans/M6.md`, 2026-10-20)
- **Requirement IDs**: REQ-X-5 (list/enable/disable network adapters, deferred from M5 by ADR 0026),
  REQ-X-6, REQ-X-9, REQ-C-5, REQ-K-5, D-12; ADR 0026, ADR 0027, ADR 0030
- **Affected**: `umbra-engine` (handle commands and queries), `umbra-api-types`, `umbra-server`,
  `api/*.json` (gate 10), `docs/api.md`

## Context
ADR 0026 left REQ-X-5's network adapter endpoints to M6, "additively". M6 also adds the event
`Detonation` (ADR 0030) and emits `EntityCreated`/`EntityRemoved` for ghosts. The GUI track (M5b) consumes
the committed contract in parallel on another branch; v1 changes must stay additive (ADR 0026), and
clients ignore unknown event kinds (ADR 0027).

## Decision
- `GET /api/v1/network/adapters` -> `[{ name, adapter, enabled, faulted, key_space: {base, count},
  ghosts, counters: { published, received, echo_dropped, unmapped, keys_exhausted, ghosts_capped,
  tx_dropped, rx_dropped, decode_errors, io_errors } }]` in scenario order; empty list when the session has
  no adapters; `409` (existing code) when idle.
- `POST /api/v1/network/adapters/{name}/enable` and `.../disable` -> the adapter entry; a disabled
  adapter is neither published to nor polled (its sockets stay open, its queue drops and counts) and its
  ghosts time out normally; applied between ticks like every command (ADR 0024); idempotent; unknown name
  -> new problem code `adapter_not_found` (`404`); a faulted adapter cannot be enabled -> new code
  `adapter_faulted` (`409`).
- Stream and REST events: new kind `detonation` (`shooter`, `target` optional, `position` with the
  geodetic display values of ADR 0027); entity DTOs already carry `ownership`; the remote `source` is
  exposed as `remote_source` (optional field).
- `API_VERSION` minor bump (`1.1.0`), `api/*.json` regenerated in the same work item, `docs/api.md`
  tables updated, `generate-api-client` handed to the GUI track.

## Consequences
- Easier: REQ-X-5 complete; the GUI can show adapter status in M8 (plugin/adapter status view).
- Harder: the contract changes while M5b runs on another branch; the merge order must regenerate the
  GUI client (plan risk R6).
- Revisit with M8 (adapter status view) and M7 (recording adapter status).
