# Template and scenario file reference (format v1)

Source of truth for the format: ADR 0010 (decisions; its planned key `plugins` became `plugin_paths`, ADR
0022), ASM-053..ASM-069 (details), and the readers in
`crates/umbra-templates`. The JSON Schemas `crates/umbra-templates/schemas/{template,scenario}.v1.schema.json`
(printed by `umbra schema template|scenario|config`) describe the structure for editors; they are hand-written,
use only the ADR 0009 subset, and the reader is authoritative where they differ (REQ-T-3, NFR-5, D-14).

Behavior graph files (`*.behavior.toml`, M3) have their own reference: `docs/behavior.md`.

> **Migration note (M3).** From M3 a file whose name ends in `.behavior.toml` (case-sensitive) is a
> behavior graph, whatever it contains (ADR 0012, ASM-074). An M2 template or scenario saved under such a
> name is no longer read as a template or scenario; rename it (for example `tank.toml`). `load_scenario`
> on such a file reports `not_a_scenario` ("is a behavior graph"). Every other file name is classified
> exactly as in M2.

Files are TOML 1.1. Datetimes and `nan`/`inf` are rejected everywhere. Units are given by key suffix:
`_m`, `_s`, `_deg`, `_dps`, `_mps`, `_mps2`, `_hz`. Unknown keys are errors everywhere.

## Versioning (REQ-T-5)
Every template and scenario file starts with `schema_version = 1`, a required integer.
- Newer than 1: `unsupported_schema_version` at the value; the rest of the file is not checked.
- Missing: `missing_key`; not an integer: `wrong_type`; below 1: `invalid_value`.
- There is no older version, so "migrate older" has nothing to do in v1. A future v2 adds a migration step
  over the v1 tree before the semantic checks. Adding an optional key does not bump the version;
  removing, renaming or changing the meaning of a key does.

## IDs
Template IDs and component IDs follow ADR 0002: lowercase `[a-z0-9_]` segments separated by dots, at least
two segments, each starting with a letter, at most 128 bytes (`ground_vehicle.base`,
`builtin.ground.kinematic`). A bad ID is `invalid_id`, located at the offending character. Component IDs
are global across kinds (ASM-063).

## Template files
One template per file. Keys:

| Key | Type | Notes |
|---|---|---|
| `schema_version` | integer | required |
| `id` | string | required, the template's ID; the file name does not matter |
| `extends` | string | optional parent template ID (single inheritance) |
| `description` | string | optional, not inherited |
| `[motion]` | table | `model` (component ID), `params` (table) |
| `[behavior]` | table | `ref` (a registered behavior, e.g. a built-in graph, or a `*.behavior.toml` graph of the same load), `params`; see "Behaviors and sensors" below |
| `[[sensors]]` | array of tables | `kind` (required, a registered sensor such as `builtin.visual`), `params`; resolved into the template and dry-run through the sensor's factory |
| `[network.<adapter>]` | tables | adapter name `[a-z][a-z0-9_]*`; contents are opaque (REQ-T-4) |

Every resolved template must name a motion model (in itself or in an ancestor): otherwise `missing_key`.

Example (`tank.mbt_generic`, inheriting a base):
```toml
schema_version = 1
id = "tank.mbt_generic"
extends = "ground_vehicle.base"
description = "Generic main battle tank"

[motion.params]
max_turn_rate_dps = 30.0

[network.dis]
entity_type = "1.1.225.1.1.1.0"
```

### Merge rules (inheritance)
Chains are folded root first. `id`, `extends`, `description` and `schema_version` are not inherited.
- `motion` and `behavior` merge key-wise: a child that names a `model`/`ref` replaces it; `params` merge
  key-wise (a child key replaces the parent's value for that key, whole values, no deep merge).
- A child that names a **different** `model`/`ref` drops the parent's `params` (they belong to the
  parent's model). Naming the same one again keeps merging.
- `sensors` arrays replace as a whole (the most derived list wins; an empty list clears).
- Each `network.<adapter>` table replaces the parent's table for that adapter as a whole; other adapters
  are inherited. The tables are never validated by the engine and are handed to the adapter unchanged,
  but they still cross as JSON: datetimes, `nan`/`inf` and integers outside 64 bits are rejected there too
  (`unsupported_value`, ASM-060).
- The merged `params` are validated against the component's JSON Schema, then dry-run through the
  component factory (`Factory::build`), so `validate` catches what spawning would (ASM-058).
- Errors are located where the offending value is written, so an error inherited from a parent is reported
  once, in the parent's file.

### Behaviors and sensors (M3)
Since M3 `[behavior]` and `[[sensors]]` resolve: the merged (resolved) template carries them and every
placement of it is spawned with that behavior and those sensors (ASM-083, ASM-107). They are validated
like the motion model, with these rules for behavior params (ADR 0014):
- `ref` must name a `Behavior`: a registered one (the built-in graphs `builtin.fsm.patrol_and_engage` and
  `builtin.subgraph.engage_target`, plugins from M4) or a graph file of the same load. An unknown ID is
  `unknown_id`; a condition or action is `unknown_id` with "`x` is a condition, not a behavior". To give
  an entity a single action, wrap it in a one-node graph (`docs/behavior.md`).
- `params` are literals only. The graph forms `{ key = "<key>" }` (input binding) and `{ param = "<name>" }`
  (reference) are `invalid_binding` anywhere in template `params`, because a template's behavior has no
  parent blackboard and no enclosing interface.
- `params` are checked against the graph's `[interface]` (`missing_param`, `unknown_param`,
  `param_wrong_type`, `param_out_of_range`, at the template's value), then followed into the graph's
  leaves and subgraph calls, so a value that breaks a leaf's range deep in the graph is reported at the
  template with notes along the chain (`docs/behavior.md`, "Load-time checks").
- A behavior with a **required binding-only param** (`type = "entity"` or `"position"` in its interface)
  cannot be a template behavior: it can only be called as a subgraph with a `{ key }` binding, so the
  template gets `missing_param`. `builtin.subgraph.engage_target` is such a behavior.
- Behaviors are not overridable per placement (scenario overrides stay motion-only, ASM-083): swapping a
  behavior is a template change.

**Plugin behaviors (M4).** A plugin behavior is a registered behavior (ASM-107, ASM-114) and follows the
rules above. Its plugin must be loaded (`--plugin-path` or the scenario's `plugin_paths`), otherwise its
ID is `unknown_id`; `umbra validate` loads the plugins and dry-runs their `configure` (ADR 0022). A
param the plugin declares bindable but not in its schema (and every `entity`/`position` param) is
binding-only, so such a behavior is `missing_param` as a template behavior. A rule that combines two
params passed through different calls may be checked only by the plugin's `configure` at spawn time
(ASM-106 d). Details: `docs/plugin-guide.md`, "Params, schemas and bindable params".

Sensor `params` are validated against the sensor's schema and dry-run; an entity without a sensor has no
contacts, so conditions such as `builtin.cond.enemy_in_range` never succeed for it.

## Scenario files
A file whose name ends in `.behavior.toml` is a behavior graph (M3, ASM-074; see the migration note
above). Of the other `.toml` files, one with a top-level `scenario` table is a scenario and any other is a
template (ASM-053).

| Key | Type | Notes |
|---|---|---|
| `schema_version` | integer | required |
| `[scenario]` `name` | string | required |
| `seed` | integer >= 0 | required, master seed |
| `tick_rate_hz` | integer >= 1 | optional; whole hertz only. Absent: the loader's default applies, 60 Hz (the configuration file `umbra.toml` can change it with `[simulation] tick_rate_hz`, see "Configuration file" below; ADR 0010 note, ADR 0029, ASM-168, ASM-169). A present value is always used as written; 0 or a negative number is `invalid_value`, a fraction or a string `wrong_type`. `umbra run` and `umbra validate` print a `note:` line on stderr when the default applied to a valid scenario (stdout is unchanged). A scenario without the key is not self-contained: its results also depend on that default, so examples keep an explicit `tick_rate_hz` |
| `duration_s` | number > 0 | required; run length is `ceil(duration_s * hz)` ticks (a product within two ULPs of a whole number counts as that number, so 0.1 s at 30 Hz is 3 ticks; ASM-066) |
| `clock` | `afap` \| `realtime` \| `stepped` \| `scaled` | default `afap` |
| `time_scale` | number in [0.001, 1000000] | required with `clock = "scaled"`, an error otherwise |
| `template_paths` | array of strings | directories relative to the scenario file, searched recursively for `*.toml` (templates and `*.behavior.toml` graphs) |
| `plugin_paths` | array of strings | plugin directories relative to the scenario file, like `template_paths` (joined as written, not normalised; REQ-P-10, ADR 0022, ASM-143); absent is none. Each entry must be a string (`wrong_type`, located at the entry). CLI `--plugin-path` directories come first, duplicates (by canonical path) are dropped; a missing directory is a load error, not a diagnostic (ASM-144). Package layout: `docs/plugin-guide.md`. `plugins` is not a key (`unknown_key`) |
| `on_plugin_fault` | `abort` \| `disable_capability` | default `abort` (REQ-P-7, ADR 0021): what a plugin fault during the run does (`abort` ends the run after the faulting tick, `disable_capability` continues without the capability; `docs/plugin-guide.md`, "Faults"); another string is `invalid_value` naming both values, a non-string `wrong_type` (ASM-143) |
| `[terrain]` | table | `provider` (default `builtin.terrain.flat`), `params` (default `{ height_m = 0.0 }` for the default provider) |
| `[[entities]]` | array of tables | spawned in file order |

Entity keys: `template` (required), `position` (required), `name`, `group`, `heading_deg` (default 0),
`speed_mps` (>= 0, default 0), `force` (optional, `[a-z][a-z0-9_]*`, at most 128 bytes; absent means neutral; two entities are hostile iff both have a force and the forces differ, so a neutral entity is never hostile to anyone; a bad name is `invalid_value`, a non-string `wrong_type`; ADR 0015, ASM-079), `[entities.overrides.motion]` (`model`, `params`; merged over the template's
motion and validated again, errors located in the scenario), `[entities.route]` (`waypoints` non-empty,
`arrival_radius_m` > 0, `cruise_speed_mps` > 0). Duplicate non-empty names are `duplicate_name`. Every
template and graph found under `template_paths` is validated, not only the referenced ones; any error fails
the load.

### Positions (ASM-044)
Everywhere a position is accepted (entity `position`, route `waypoints`), exactly one form:
```toml
position = { lat_deg = 52.0, lon_deg = 4.0, alt_m = 10.0 }   # WGS-84; alt above the ellipsoid, default 0
position = { ecef_m = [3921872.0, 274244.0, 5005546.0] }     # exactly three numbers, meters
```
Latitude must be within [-90, 90] and longitude within [-180, 180]; violations are `invalid_value` located
at the position table. Both forms are converted to ECEF once.

## Configuration file (`umbra.toml`, ADR 0029)
Read by `run`, `validate` and `list-plugins` (and `serve` from WI-17). `--config FILE` (a missing file is
exit 1), else `./umbra.toml` if it exists (announced on stderr as `note: using configuration ./umbra.toml`),
else built-in defaults. Precedence per setting is flag > file > default; lists are replaced, not merged.
Same reader and diagnostics as the other files: unknown keys are `unknown_key` at every level, all errors
are reported. Every key is optional; `schema_version` may be omitted (absent means 1). Relative paths
resolve against the file's directory. Print the schema with `umbra schema config`.

| Key | Type / default | Meaning |
|---|---|---|
| `[simulation] tick_rate_hz` | integer >= 1, 60 | Default for scenarios that omit it; an explicit scenario value wins |
| `[plugins] paths` | array of paths | Plugin directories before each scenario's `plugin_paths`; any `--plugin-path` replaces them |
| `[log] level` | `off` `error` `warn` `info` `debug` `trace`, `warn` | Log level of `run`, lower-case names only; `--log-level` wins and is case-insensitive |
| `[server] bind` | socket address, `127.0.0.1:7878` | `[::1]:7878` for IPv6 |
| `[server] scenario_root` | path, `.` | Served loads are confined here |
| `[server] body_limit_bytes` | integer >= 1, 1048576 | |
| `[server] command_queue` | integer >= 1, 64 | |
| `[server] allowed_hosts` | array of strings, empty | Extra `Host` values |
| `[server] allowed_origins` | array of `scheme://host[:port]`, empty | Extra WebSocket origins |
| `[server] shutdown_grace_s` | number > 0, 5 | |
| `[server.stream] default_rate_hz` | number in (0, 60], 30 | Must not exceed `max_rate_hz` |
| `[server.stream] max_rate_hz` | number in (0, 60], 60 | |
| `[server.stream] queue_frames` | integer >= 1, 4 | |
| `[server.stream] max_events_per_frame` | integer >= 1, 4096 | |
| `[server.stream] send_timeout_s` | number > 0, 10 | |
| `[server.stream] max_connections` | integer >= 1, 64 | |

The `[server]` keys are read and validated now; `umbra serve` (WI-17) uses them.

## Diagnostics
Text form, one per error, sorted by (path, byte offset, code, message) without duplicates:
```
path:line:col: error[code]: message
  note: extra line (path:line:col)
```
Line and column are 1-based; the column counts Unicode scalar values; CRLF is one line break. A diagnostic
without a position (`io` on a path, or a parser error that reports no span) has no `path:line:col:` prefix. All errors of all files are reported
together. Files with syntax errors are not checked further; templates that depend on a broken or
cyclic template produce no follow-on errors.

| Code | Meaning |
|---|---|
| `io` | A file or directory could not be read |
| `syntax` | TOML syntax error (an unclosed array or inline table is reported at its opening bracket) |
| `unsupported_value` | A datetime, a non-finite or out-of-range float, or an integer outside 64 bits |
| `unknown_key` | A key the format does not define |
| `missing_key` | A required key or table is missing, or a template has no motion model |
| `wrong_type` | A value of the wrong TOML type |
| `invalid_value` | A value out of range or otherwise invalid |
| `invalid_id` | An ID that breaks the ADR 0002 grammar |
| `unsupported_schema_version` | `schema_version` newer than supported |
| `duplicate_id` | Two templates with one ID (note points at the first) |
| `unknown_template` | `extends` or an entity's `template` names an undefined template |
| `unknown_id` | A component ID that is not registered (note lists the registered ones) |
| `inheritance_cycle` | Templates extend each other in a loop (one error per cycle, notes along the chain) |
| `missing_param` | A required component parameter is missing |
| `param_wrong_type` | A component parameter has the wrong type |
| `param_out_of_range` | A parameter outside its bounds or length limits |
| `unknown_param` | A parameter the component's schema does not define |
| `param_invalid` | `enum`/`const`/`oneOf` violation, or a combination the factory rejected |
| `duplicate_name` | Two entities with one name |
| `not_a_scenario` | A file given as a scenario has no `[scenario]` table, or is a `*.behavior.toml` graph |

Graph files add their own codes (`unknown_node`, `node_reused`, `invalid_binding`, `subgraph_cycle`, ...)
and reuse several of these; they are listed in `docs/behavior.md`, "Diagnostics". `invalid_binding` and
`missing_param` also appear at a template's `[behavior] params` (see "Behaviors and sensors").

## Component parameter schemas (the JSON Schema subset, ADR 0009)
Components supply their own parameter schema. The validator supports: `type` (string or list; `integer`
accepts only TOML integers, `number` accepts both), `properties`, `required`, `additionalProperties`
(boolean or schema), `items` (single schema), `minItems`, `maxItems`, `enum`, `const`, `minimum`,
`maximum`, `exclusiveMinimum`, `exclusiveMaximum`, `minLength`, `maxLength`, `oneOf` (when none matches,
the errors of the closest branch are reported), `$defs` at the root with local `$ref: "#/$defs/<name>"`
(any cycle is rejected, including through `properties` or `items`; a schema whose `$ref` expansion exceeds 10 000 nodes is rejected, ASM-068). `$schema`, `$id`, `$comment`, `title`, `description`, `default`,
`examples`, `deprecated`, `readOnly`, `writeOnly` are accepted and ignored. Any other keyword makes the
schema invalid; a registered component with such a schema is a component bug and the loader refuses to start.
For a plugin capability this is `LoaderError::Schema` naming the capability, so every scenario load with
that plugin fails (ASM-114).

## Built-in components
`umbra schema component <ID>` prints the registered parameter schema of any component and is the
authoritative source; this section is a summary. Unknown parameters are errors for both.

### `builtin.ground.kinematic` (motion model)
Kinematic ground vehicle limits (spec section 7.1). All three parameters are required numbers (integers
are accepted), greater than 0, with upper bounds that reject absurd values (ASM-065).

| Parameter | Unit | Bound |
|---|---|---|
| `max_speed_mps` | m/s | > 0, <= 1000 |
| `max_turn_rate_dps` | degrees per second | > 0, <= 3600 |
| `max_accel_mps2` | m/s^2 (acceleration and braking) | > 0, <= 1000 |

```toml
params = { max_speed_mps = 18.0, max_turn_rate_dps = 40.0, max_accel_mps2 = 2.5 }
```

### `builtin.terrain.flat` (terrain provider, the default for scenarios)
Flat terrain at a constant height above the WGS-84 ellipsoid.

| Parameter | Unit | Bound |
|---|---|---|
| `height_m` (required) | m | within [-1000, 10000] (ASM-069) |

```toml
[terrain]
provider = "builtin.terrain.flat"
params = { height_m = 0.0 }
```

### `builtin.visual` (sensor, M3)
Perfect-detection visual sensor (ADR 0015, ASM-080, ASM-108). An entity is a contact when it is within
`range_m` (3D distance, inclusive), inside the horizontal field of view centred on the observer's heading
(edge inclusive), and the terrain gives line of sight between the two entities' eyes.

| Parameter | Unit | Bound |
|---|---|---|
| `range_m` (required) | m | > 0, <= 100000 |
| `fov_deg` (required) | degrees | > 0, <= 360 (360 sees all around) |
| `eye_height_m` (default 2) | m | >= 0 |

With `eye_height_m = 0` two entities on smooth ground (flat or curved) never see each other: the straight
line between them dips below the surface. Details and the behavior side: `docs/behavior.md`, "Perception".
```toml
[[sensors]]
kind = "builtin.visual"
params = { range_m = 3000.0, fov_deg = 120.0 }
```

### Behaviors, conditions and actions (M3)
The built-in graphs, conditions and actions are listed with their parameters in `docs/behavior.md`,
"Built-in catalog".
