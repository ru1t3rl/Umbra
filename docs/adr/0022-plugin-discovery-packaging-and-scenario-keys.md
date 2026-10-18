# ADR 0022: Plugin discovery, packaging and scenario keys

- **Status**: Accepted (human project owner, 2026-10-18; proposed by the architect in the M4 plan `docs/plans/M4.md`)
- **Requirement IDs**: REQ-P-9, REQ-P-10, REQ-P-7, REQ-T-2, REQ-T-5, REQ-T-6, REQ-S-1, NFR-3, D-25 (ADR 0010),
  A-001 constraint 3, ADR 0002
- **Affected**: `umbra-templates` (scenario reader, `scenario.v1.schema.json`), `umbra-plugin-host`,
  `umbra-engine`, `umbra-cli`, `xtask build-plugins`, `docs/templates.md`, `docs/plugin-guide.md`

## Context
REQ-P-10: plugin directories come from engine config and the scenario; an optional `plugin.toml` next to
each library is checked before the library is loaded. Section 7.3 says a scenario lists "plugins to load".
ADR 0010 planned the keys `plugins` and `on_plugin_fault` for M4, but two frozen M2 tests pin
`plugins` in `[scenario]` as `unknown_key`: `umbra-templates/tests/scenario_file.rs`
(`unknown_key_in_the_scenario_table_is_unknown_key`) and the fixture
`tests/fixtures/scenarios/invalid_structure/unknown_scenario_key.toml` (used by `review_scenarios.rs`).
Adding `plugins` breaks them (A-001 constraint 3).

## Options
1. **Directories, key `plugin_paths` (chosen).** `[scenario] plugin_paths = ["plugins"]`, relative to the
   scenario file like `template_paths`; every plugin package found there is loaded. Pros: matches REQ-P-10
   ("plugin directories"); no M2 test changes; same shape users know. Cons: a scenario cannot pick a subset
   of a directory; ADR 0010's planned key name changes.
2. **`plugins = [..]` as ADR 0010 planned, with an approved edit of the two M2 tests.** Cons: breaks the
   freeze; needs a migration ADR for one key name.
3. **Engine config only.** Cons: violates REQ-P-10 and section 7.3 (the scenario names its plugins).

## Decision
Option 1.
- **Scenario keys** (additive, `schema_version` stays 1, ADR 0010 rules): `[scenario] plugin_paths`
  (array of strings, default none) and `[scenario] on_plugin_fault` (`"abort"` | `"disable_capability"`,
  default `"abort"`). Wrong types and values are located diagnostics (`wrong_type`, `invalid_value`).
  `plugins` stays an unknown key. ADR 0010's note is superseded on this point.
- **Engine config**: `umbra run|validate|list-plugins --plugin-path <DIR>` (repeatable) and the matching
  library parameter; CLI paths come first, then the scenario's, duplicates (by canonical path) dropped.
- **Package layout**: a plugin path is a directory; each immediate subdirectory is one plugin package
  holding exactly one library with the platform extension (`.so`, `.dll`, `.dylib`) and optionally a
  `plugin.toml`. Packages are loaded in path order, then by subdirectory name (bytes), so registration
  order is deterministic. Zero or several libraries in a package, a missing directory, or the same plugin
  ID twice (any paths, including the same file reached twice) are load errors naming both locations.
- **`plugin.toml`** (optional, checked before the library is opened): `schema_version = 1`, `id`
  (`PluginId`, ADR 0002), `version` (semver), `abi = "<major>.<minor>"`, optional `name`; unknown keys are
  errors. An ABI the host cannot load (major differs, or minor newer, REQ-P-9) is rejected without opening
  the library. After loading, descriptor `id`, `version` and ABI must equal the manifest.
- **Validation**: `umbra validate` loads the plugins of the CLI paths and of every scenario file it checks
  (their `plugin_paths`), so templates that name plugin behaviors validate (REQ-T-6); a scenario's plugin
  paths are read leniently before the full load (like `template_paths`) and the full reader reports
  their errors. Loading runs plugin `init`; validation dry-runs `configure` (plugins are trusted, NFR-4).
- **Build output**: `cargo xtask build-plugins [--release]` writes packages to `target/plugins/<name>/`.

## Consequences
- Easier: no M2 test or fixture changes; one discovery rule on Windows and Linux; deterministic order.
- Harder: an example scenario cannot point at a build directory portably, so the example passes
  `--plugin-path target/plugins` (documented); per-plugin selection inside a directory needs a later key.
- Revisit if users need to select plugins by ID or version range (a new additive key, e.g. `[[plugin]]`).
