# ADR 0029: The `umbra.toml` configuration file and the default tick rate

- **Status**: Accepted by the human 2026-10-19 (proposed by the architect, M5 plan `docs/plans/M5.md`; human decisions of 2026-10-19 "default 60 Hz,
  adjustable" and "server settings through a config file"; awaiting acceptance of the plan)
- **Requirement IDs**: REQ-X-2, REQ-X-8, REQ-T-2, REQ-T-5, REQ-K-4, REQ-C-1, REQ-P-10, REQ-X-3, NFR-4, A-2,
  ADR 0010 (note of 2026-10-19), ADR 0022, ADR 0024, ADR 0026, ADR 0027
- **Affected**: `umbra-templates` (new `config` module, `LoaderOptions::default_tick_rate`, reader),
  `umbra-engine` (re-exports, load options), `umbra-cli` (`--config`, every command that loads a
  scenario), `umbra-server` (`ServerConfig` from the file), `schemas/config.v1.schema.json`

## Context
The human decided on 2026-10-19 that the default tick rate is 60 Hz and can be adjusted, and that server
settings (bind address, scenario root, body limit, stream defaults) come from a configuration file. The
project already has one located TOML reading pipeline in `umbra-templates` (spanned values,
`path:line:col: error[code]` diagnostics, `unknown_key`/`wrong_type`/`invalid_value`, all errors
reported; ADR 0010, REQ-T-2). Settings that change simulation results are determinism inputs (A-2).

## Options
1. **One `umbra.toml`, read by `umbra-templates` into a neutral settings struct; the CLI maps it to the
   handle and server settings (chosen).** Same diagnostics, codes and strictness as the other formats,
   with no new parser; `umbra-templates` stays free of server types (it returns plain values).
2. **Parse in `umbra-cli` with `toml` + serde derives.** Cons: serde stops at the first error and gives
   another diagnostic format (the reason ADR 0010 rejected derives); a second TOML pipeline (DRY).
3. **Parse in `umbra-server`.** Cons: `run` and `validate` need `[simulation]` and `[plugins]` too, and they
   must work without the server feature (D-1, gate 16).

## Decision
Option 1.
- **Location.** `--config FILE` on every command (a missing named file is an error, exit 1); otherwise
  `./umbra.toml` in the working directory if it exists; otherwise built-in defaults. Relative paths in the
  file resolve against the file's directory, like scenario paths. The file used is logged at info level.
- **Keys** (all optional; unknown keys anywhere are located `unknown_key` errors; every error is reported):
  ```toml
  schema_version = 1                 # optional; newer is unsupported_schema_version
  [simulation]
  tick_rate_hz = 60                  # default for scenarios that omit tick_rate_hz
  [plugins]
  paths = ["plugins"]                # plugin directories, before each scenario's own plugin_paths
  [log]
  level = "warn"                     # off | error | warn | info | debug | trace
  [server]
  bind = "127.0.0.1:7878"            # one socket address; IPv6 as "[::1]:7878"
  scenario_root = "."                # loads are confined here (ADR 0026)
  body_limit_bytes = 1048576
  command_queue = 64                 # engine command channel capacity (ADR 0024)
  [server.stream]
  default_rate_hz = 30               # used when a subscriber gives no max_rate_hz
  max_rate_hz = 60                   # upper bound a subscriber may ask for
  queue_frames = 4                   # per-connection subscription capacity
  max_events_per_frame = 4096
  send_timeout_s = 10.0              # then close with too_slow
  ```
  Validation: `tick_rate_hz` a positive whole number (as in scenarios); `bind` parses as a socket address;
  stream rates in (0, 60] (the ADR 0027 ceiling stays) with `default_rate_hz <= max_rate_hz`; sizes and counts positive integers;
  `send_timeout_s` finite and > 0; `level` one of the six names. A JSON Schema
  `config.v1.schema.json` (`$id` `urn:umbra:schema:config:v1`) is published and printed by
  `umbra schema config`.
- **One `bind` key, not `host` + `port`.** It mirrors the `--bind` flag one to one, so precedence is a
  single rule (a flag replaces the whole address, never half of it), it parses with `SocketAddr`
  without inventing how a host and a port combine, and IPv6 needs no special case.
- **Precedence.** CLI flag > config file > built-in default, per setting. Lists are replaced, not merged:
  any `--plugin-path` replaces `[plugins] paths` (the scenario's own `plugin_paths` are still added after,
  ADR 0022). A non-loopback `bind` from either source logs the no-authentication warning (ADR 0026).
- **Default tick rate.** Built-in 60 Hz (`umbra_core::DEFAULT_TICK_RATE_HZ`); `[simulation] tick_rate_hz`
  replaces it. It reaches the loader as `LoaderOptions::default_tick_rate` (the struct is
  `#[non_exhaustive]`, so this is additive), and the reader applies it only when the scenario has no
  `tick_rate_hz`; an explicit scenario value always wins. There is no CLI flag and no REST field that
  overrides the tick rate of a loaded scenario: the scenario file and, for files that omit the key, the
  configured default are the only two inputs.
- **Scope.** `run`, `validate`, `list-plugins` and `serve` all read the configuration (the same default
  must give `validate` and `run` the same `duration_ticks`); each uses the keys that apply to it.

## Consequences
- Easier: one place for deployment settings; the same diagnostics users know from scenario files; no new
  dependency.
- Harder: a scenario that omits `tick_rate_hz` gives different results under different configurations,
  and a `./umbra.toml` in the working directory is an implicit input. Mitigations: the effective rate and
  its source are reported in `SessionInfo`, `GET /api/v1/status`, the stream `hello`/`run` status, and on
  stderr by `umbra run` and `umbra validate` whenever the default applied (stdout stays byte-identical);
  the examples keep explicit rates; M7 recordings must store the effective rate.
- Revisit if a second configuration consumer appears outside the CLI (then the reader may move), or if
  users need per-environment overlays.
