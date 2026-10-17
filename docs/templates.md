# Template and scenario file reference (format v1)

Source of truth for the format: ADR 0010 (decisions), ASM-053..ASM-069 (details), and the readers in
`crates/cgf-templates`. The JSON Schemas `crates/cgf-templates/schemas/{template,scenario}.v1.schema.json`
(printed by `cgf-cli schema template|scenario`) describe the structure for editors; they are hand-written,
use only the ADR 0009 subset, and the reader is authoritative where they differ (REQ-T-3, NFR-5, D-14).

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
| `[behavior]` | table | `ref` (component ID), `params`. In M2 no behavior is registered, so any `ref` is `unknown_id` |
| `[[sensors]]` | array of tables | `kind` (required), `params`. In M2 any `kind` is `unknown_id` |
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

## Scenario files
A file with a top-level `scenario` table is a scenario; any other `.toml` file is a template (ASM-053).

| Key | Type | Notes |
|---|---|---|
| `schema_version` | integer | required |
| `[scenario]` `name` | string | required |
| `seed` | integer >= 0 | required, master seed |
| `tick_rate_hz` | integer >= 1 | required; whole hertz only |
| `duration_s` | number > 0 | required; run length is `ceil(duration_s * hz)` ticks (a product within two ULPs of a whole number counts as that number, so 0.1 s at 30 Hz is 3 ticks; ASM-066) |
| `clock` | `afap` \| `realtime` \| `stepped` \| `scaled` | default `afap` |
| `time_scale` | number in [0.001, 1000000] | required with `clock = "scaled"`, an error otherwise |
| `template_paths` | array of strings | directories relative to the scenario file, searched recursively for `*.toml` |
| `[terrain]` | table | `provider` (default `builtin.terrain.flat`), `params` (default `{ height_m = 0.0 }` for the default provider) |
| `[[entities]]` | array of tables | spawned in file order |

Entity keys: `template` (required), `position` (required), `name`, `group`, `heading_deg` (default 0),
`speed_mps` (>= 0, default 0), `[entities.overrides.motion]` (`model`, `params`; merged over the template's
motion and validated again, errors located in the scenario), `[entities.route]` (`waypoints` non-empty,
`arrival_radius_m` > 0, `cruise_speed_mps` > 0). Duplicate non-empty names are `duplicate_name`. Every
template found under `template_paths` is validated, not only the referenced ones; any error fails the load.

### Positions (ASM-044)
Everywhere a position is accepted (entity `position`, route `waypoints`), exactly one form:
```toml
position = { lat_deg = 52.0, lon_deg = 4.0, alt_m = 10.0 }   # WGS-84; alt above the ellipsoid, default 0
position = { ecef_m = [3921872.0, 274244.0, 5005546.0] }     # exactly three numbers, meters
```
Latitude must be within [-90, 90] and longitude within [-180, 180]; violations are `invalid_value` located
at the position table. Both forms are converted to ECEF once.

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
| `not_a_scenario` | A file given as a scenario has no `[scenario]` table |

## Component parameter schemas (the JSON Schema subset, ADR 0009)
Components supply their own parameter schema. The validator supports: `type` (string or list; `integer`
accepts only TOML integers, `number` accepts both), `properties`, `required`, `additionalProperties`
(boolean or schema), `items` (single schema), `minItems`, `maxItems`, `enum`, `const`, `minimum`,
`maximum`, `exclusiveMinimum`, `exclusiveMaximum`, `minLength`, `maxLength`, `oneOf` (when none matches,
the errors of the closest branch are reported), `$defs` at the root with local `$ref: "#/$defs/<name>"`
(any cycle is rejected, including through `properties` or `items`; a schema whose `$ref` expansion exceeds 10 000 nodes is rejected, ASM-068). `$schema`, `$id`, `$comment`, `title`, `description`, `default`,
`examples`, `deprecated`, `readOnly`, `writeOnly` are accepted and ignored. Any other keyword makes the
schema invalid; a registered component with such a schema is a component bug and the loader refuses to start.

## Built-in components
`cgf-cli schema component <ID>` prints the registered parameter schema of any component and is the
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
