# Scenario plugin-key fixtures (M4 WI-11, AT-10)

Provenance: written by the test-engineer for M4 step 4 (ADR 0022, `docs/plans/M4.md` AT-10). New
directory; no M0-M3 fixture is touched.

- `lenient/scenario.toml`: valid `plugin_paths`, but an error elsewhere (`seed` is a string). The lenient
  pre-read `scenario_plugin_paths` must still return the paths; the full load reports the error.
- `walk/`: a directory holding two scenario files (`a/`, `b/`, each with its own `plugin_paths`) and a
  template file that is not a scenario (`templates/t.toml`, contributes nothing).
