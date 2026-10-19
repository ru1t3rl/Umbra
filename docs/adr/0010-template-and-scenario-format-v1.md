# ADR 0010: Template and scenario file format v1

- **Status**: Accepted (approved by the human project owner on 2026-10-17; M2 plan `docs/plans/M2.md`)
- **Requirement IDs**: REQ-T-1..7, REQ-S-1, REQ-C-3, REQ-C-4, D-14, D-15, ASM-044, ASM-016, ASM-029
- **Affected**: `umbra-templates`, `umbra-engine`, `umbra-cli`, `examples/scenarios/`, `docs/templates.md`, every user file

## Context
Section 7 gives one template example and a prose list for scenarios. The file format becomes a user
contract the day M2 ships (`schema_version = 1`), so its shape, inheritance rules and versioning policy
must be fixed explicitly. Constraints: all errors reported with file and line (REQ-T-2); positions by ECEF
or lat/lon/alt (ASM-044, human requirement); whole-hertz tick rates (ASM-016, human 2026-10-17); behaviors,
sensors, plugins, network adapters and scheduled events have no runtime before M3-M6.

## Options
1. **Strict format, hand-read from the spanned TOML tree; unknown keys are errors; additive evolution.**
   One template per file; scenarios are recognised by a top-level `[scenario]` table; features without
   a runtime are either absent from the format (added later as optional keys) or resolve through the
   registry, where their IDs are unknown in M2.
2. **Lenient format via serde derives; unknown keys ignored.** Pros: less code. Cons: serde stops at the
   first error (violates "all errors"), typos pass silently, spans need wrapping on every field.
3. **Accept every section 7.3 field now and ignore what cannot run.** Cons: silently ignoring `plugins` or
   scheduled events misleads users; dishonest.

## Decision
Option 1:
- **Versioning.** Both file kinds carry a required integer `schema_version`; the engine supports exactly
  `1`. Newer is rejected (REQ-T-5 MUST). There is no older version, so "migrate older" is a documented
  seam: a future v2 adds a `migrate` step from the v1 tree before semantic checks. Adding an optional key
  does not bump the version; removing, renaming or changing the meaning of a key does.
- **Template keys.** `schema_version`, `id`, optional `extends`, optional `description`, `[motion]`
  (`model`, `params`), `[behavior]` (`ref`, `params`), `[[sensors]]` (`kind`, `params`),
  `[network.<adapter>]`. Every resolved template must name a motion model (no abstract templates).
- **Inheritance** (single, by `extends`): scalars replace; `motion` and `behavior` merge key-wise, and their
  `params` merge key-wise unless the child names a different `model`/`ref`, in which case the parent's
  `params` are dropped; `sensors` arrays replace entirely; each `network.<adapter>` table replaces the
  parent's table for that adapter as a whole (opaque, REQ-T-4). `id`, `extends`, `description`,
  `schema_version` are not inherited.
- **Behavior and sensor sections** are read and checked structurally; their IDs resolve through the
  registry, which has none in M2, so they are `unknown_id` errors with a note that none are registered.
- **Scenario keys.** `schema_version`; `[scenario]` (`name`, `seed`, `tick_rate_hz` positive integer,
  `clock` = `afap`|`realtime`|`stepped`|`scaled` with `time_scale`, `duration_s`, `template_paths`);
  optional `[terrain]` (`provider`, `params`; default `builtin.terrain.flat` at 0 m); `[[entities]]`
  (`template`, optional `name`, `group`, `position`, `heading_deg`, `speed_mps`, `[entities.overrides]`,
  `[entities.route]`). Entities are spawned in file order.
- **Positions** everywhere: `{ lat_deg, lon_deg, alt_m? }` (alt above the WGS-84 ellipsoid, default 0) or
  `{ ecef_m = [x, y, z] }`; exactly one form; converted to ECEF once (ASM-044).
- **Initial route** per entity (`waypoints`, `arrival_radius_m`, `cruise_speed_mps`) maps to the M1
  route stand-in (ASM-029). It is defined as the entity's initial route, a concept that survives M3 (the
  behavior runtime follows it via `FollowRoute`), so the key does not have to be removed later.
- **Not in v1 yet** (unknown keys today, optional keys when their milestone lands): `plugins` and
  `on_plugin_fault` (M4), scenario-level network adapters (M6), scheduled events (M5, with command
  ingest), template `components`. *(Superseded in part by ADR 0022 (D-40), M4: the key is `plugin_paths`
  (plugin directories), not `plugins`, which stays an unknown key so the frozen M2 tests hold;
  `on_plugin_fault` was added as planned. Marked in M4 WI-15.)*
- **Units at the edge** by key suffix: `_m`, `_s`, `_deg`, `_dps`, `_mps`, `_mps2`, `_hz` (REQ-C-4).
- **Diagnostics**: `path:line:column: error[code]: message` plus `note:` lines; line and column 1-based,
  column in Unicode scalar values; stable snake_case codes; sorted by path, byte offset, code, message.

## Consequences
- Easier: typos are caught; every error is located; later milestones add keys without breaking v1 files;
  one reader for templates, scenarios and overrides (DRY).
- Harder: hand-written readers are more code than derives; the spec 7.1 example template does not
  validate in M2 (its behavior and sensor IDs are unknown) and validates once M3 registers them.
- `network.<adapter>` tables are opaque to validation but still cross as JSON: datetimes, `nan`/`inf` and integers outside 64 bits are rejected there (ASM-060).
- Fractional tick rates would need a new form of `tick_rate_hz` and a version bump (human 2026-10-17).
- Revisit if users need abstract templates, multiple templates per file, or array merging by key.

## Note (Accepted 2026-10-19, M5 plan `docs/plans/M5.md` WI-2; human decision 2026-10-19)
`[scenario] tick_rate_hz` becomes **optional**. When it is absent the loader uses a default tick rate,
60 Hz built in, which the `[simulation] tick_rate_hz` key of the `umbra.toml` configuration file can
change (ADR 0029). An explicit value in the scenario always wins, and every check on a present value is
unchanged: `invalid_value` for 0 or a negative number, `wrong_type` for a fraction or a string. The
key leaves `required` in `scenario.v1.schema.json`. `SCHEMA_VERSION` stays 1: relaxing a required key is
additive under the versioning rule above, because every existing file stays valid with the same meaning.
`template.v1.schema.json` is unchanged.
`Scenario` records where its rate came from (`tick_rate_source`: `scenario` or `default`), so tools can
report it. A scenario that omits the key is no longer self-contained: its results also depend on the
default that applied, so that default is reported wherever the run is (ASM-158). The examples keep an
explicit `tick_rate_hz = 30`, so every golden hash is unchanged.
