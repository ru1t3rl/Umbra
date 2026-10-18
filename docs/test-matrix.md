# Test matrix

REQ-ID | test type | test name | status. Status values: `failing (not implemented)`, `passing`, `waived`, `manual`. Status as of tag `m2-done`: every M0-M2 row passes except M0 AT-1 (waived, ADR 0004).
Files: `xtask/tests/check_rules.rs` (R), `xtask/tests/check_deps.rs` (D), `crates/cgf-core/tests/{time,ids,units}.rs`; M1 files are listed in the M1 table.

## M0

| AT | REQ-ID | Type | Test names (file) | Status |
|---|---|---|---|---|
| AT-1 | M0 acceptance 1, NFR-3 | CI evidence | CI run URL on M0 head: fmt, clippy, test, check-deps, deny, audit on ubuntu and windows | waived (ADR 0004, no CI); replaced by the local gate passing on Linux at e785518 |
| AT-2 | REQ-A-1 | unit | `given_core_depends_on_kernel_when_checked_then_req_a1_violation`, `given_core_with_only_external_dep_when_checked_then_no_violation` (R) | passing |
| AT-3 | REQ-A-2 | unit | `given_kernel_depends_on_server_when_checked_then_req_a2_violation`, `given_kernel_depends_on_net_dis_when_checked_then_req_a2_violation`, `given_kernel_depends_on_core_when_checked_then_no_violation` (R) | passing |
| AT-4 | REQ-A-3, section 4 | unit | `given_sdk_depends_on_core_when_checked_then_req_a3_violation`, `given_sdk_depends_on_plugin_api_when_checked_then_no_violation`, `given_plugin_api_has_external_dep_when_checked_then_violation`, `..._external_build_dep_...`, `..._external_dev_dep_...no_violation` (R) | passing |
| AT-5 | ASM-005, ASM-006 | unit + fixture | `given_forbidden_dev_dep...`, `given_forbidden_build_dep...`, `given_forbidden_target_specific_dep...`, `given_renamed_dep_resolved_to_kernel...`, `given_unknown_workspace_crate_when_checked_then_unknown_crate_violation`, `given_unknown_member_without_edges...` (R); `given_violating_fixture_when_loaded_*`, `given_registry_dep_when_loaded_*`, `given_*_launder_*`, `given_same_name_impostor_*` (D); `given_*_when_resolved_checked_*` (R) | passing |
| AT-6 | M0 acceptance 2 | integration | `given_violating_fixture_when_check_deps_runs_then_exit_code_1_and_lists_each_violation`, `..._runs_twice_then_output_is_identical`, `given_conforming_fixture_when_check_deps_runs_then_exit_code_0`, `given_real_workspace_when_check_deps_runs_then_exit_code_0`, `given_missing_manifest_when_check_deps_runs_then_exit_code_2`, `given_conforming_fixture_when_loaded_and_checked_then_no_violations` (D) | passing |
| AT-7 | REQ-A-1..3 | unit | `rules_table_is_acyclic_and_closed`, `kernel_closure_excludes_server_net_and_api_types`, `sdk_closure_is_plugin_api_only`, `core_closure_is_empty`, `check_output_is_sorted_and_stable`, `rules_table_matches_plan_table`, `rules_table_only_plugin_api_forbids_external_deps` (R) | passing |
| AT-8 | REQ-C-1, D-5 | unit + proptest | all tests in `time.rs` incl. `next_strictly_increases_tick_and_never_decreases_seconds`, `as_seconds_is_bitwise_tick_over_hz` | passing (18 tests, WI-6) |
| AT-9 | REQ-C-2, ADR 0002 | unit + proptest | all tests in `ids.rs` incl. `display_then_parse_round_trips`, `validator_matches_reference_grammar` | passing (29 tests, WI-7) |
| AT-10 | REQ-C-4 | unit + proptest | all tests in `units.rs` incl. `degrees_to_radians_and_back_round_trips`; 3 `compile_fail` doctests | passing (14 tests + 3 doctests, WI-5) |
| AT-11 | NFR-2, NFR-4, REQ-K-4 | manual mutation | clippy fails on `Instant::now`/`HashMap` (WI-1, done); `cargo deny` fails on GPL dep (WI-4); ~~fmt job fails on bad format (WI-2)~~ dropped (ADR 0004) | done; evidence in the M0 running log (incl. dev-dep GPL with `include-dev`) |

Uncovered in M0 (by design): REQ-A-4 (M8, ASM-021), REQ-K-4 determinism gate (M1, ASM-019), NFR-1 benchmarks (later).

## M1

Written against `todo!()` stubs (step 4); every row is `failing (stub until WI-n)` until that work item lands. All tests were also run green against a throwaway reference implementation outside the repo (see the M1 running log).

| AT | REQ-ID | Type | Test file(s) | Status |
|---|---|---|---|---|
| AT-1 | M1 acceptance 1, REQ-K-3, REQ-K-4, D-5 | integration (gate 4) | `crates/cgf-engine/tests/determinism.rs` (8 tests: twice AFAP, std wall clock, every clock mode with and without pause at tick 100, mode switch, seeds differ, length, run-until semantics) | passing |
| AT-2 | REQ-X-2, REQ-X-3, REQ-K-4 | process | `crates/cgf-cli/tests/cli.rs` (11 tests: cross-process hash, state lines, JSON Lines, exit codes 2) | passing |
| AT-3 | REQ-C-3, REQ-C-4, D-15 | unit + proptest | `crates/cgf-core/tests/{coords,geo,coords_prop}.rs` (32, 15, 8 tests; IOGP 373-7-2 9602 and 9836 examples, analytic points, axes, orientation) | passing (WI-3, WI-4); proptests also run at 30000 cases |
| AT-4 | ADR 0003, ASM-034 | unit + proptest | `crates/cgf-core/tests/math.rs` (7) | passing (WI-2); clippy ban mutation recorded in the M1 running log |
| AT-5 | REQ-K-4, ADR 0006 | unit + proptest | `crates/cgf-core/tests/rng.rs` (18; reference vectors from the reference C code) | passing (WI-6) |
| AT-6 | REQ-K-4, ADR 0007 | unit | `crates/cgf-core/tests/hash.rs` (14); `crates/cgf-kernel/tests/state_hash.rs` (8) | passing |
| AT-7 | REQ-K-3, REQ-K-8, REQ-C-1 | unit + proptest | `crates/cgf-kernel/tests/clock.rs` (21), `clock_prop.rs` (1) | passing |
| AT-8 | REQ-M-2, REQ-M-1, REQ-M-4 | unit + proptest | `crates/cgf-motion/tests/ground_kinematic.rs` (20) | passing |
| AT-9 | M1 acceptance 3 | integration | `crates/cgf-engine/tests/waypoints.rs` (8); route stand-in in `crates/cgf-kernel/tests/pipeline.rs` | passing |
| AT-10 | REQ-K-2, REQ-K-5, REQ-C-2, REQ-K-6, REQ-K-8 | unit / integration | `crates/cgf-kernel/tests/{pipeline,world}.rs` (8 + 8) | passing |
| WI-7 | REQ-M-1, REQ-K-5, REQ-C-6, REQ-B-2 | unit | `crates/cgf-core/tests/{motion_contract,terrain_ownership}.rs` (8 + 7) | passing (WI-7) |
| AT-11 | NFR-1 | build | `cargo bench -p cgf-kernel --no-run --locked` | passing (gate 14: `cargo bench -p cgf-kernel --bench tick --no-run --locked`, WI-12) |
| AT-12 | gates | manual | mutations recorded in WI PRs | manual |
| AT-15 | ASM-044, D-15 | integration | `crates/cgf-core/tests/coords.rs` (`position_*`), `crates/cgf-engine/tests/placement.rs` (5) | passing |

Gaps: the archetype-order variant of `hash_walks_entities_in_id_order` cannot be expressed through the public kernel API (no despawn); `state_hash_equals_the_reference_encoding` (hand-built ADR 0007 encoding, mixed archetypes) pins the walk order instead, and `entity_rng_stream_is_independent_of_other_spawns` is a unit test in `crates/cgf-kernel/src/world.rs` (no public rng accessor). Intents are not directly observable in the hash test.

## M2

Written against `todo!()` stubs (step 4, commit 4adedb6), all passing at M2 close-out (HEAD 0e9839a+, 905 tests workspace-wide). Review rounds added further tests (e.g. `tests/review_templates.rs`, `tests/review_scenarios.rs`, validate/run agreement over every scenario fixture). Counts are tests per file. `crates/cgf-templates/tests/toml_pin.rs` (12) passes already: it pins the behaviour of `toml` 1.1.8 that ASM-054 depends on (message text, spans, partial tree) and fails loudly on an upgrade. Every fixture documents its provenance in its header comment; the ASM-054 fixtures under `crates/cgf-templates/tests/fixtures/syntax/` were verified against the real `toml` 1.1.8 `DeTable::parse_recoverable`.

| AT | REQ-ID | Type | Test file(s) | Status |
|---|---|---|---|---|
| AT-1 | REQ-T-1, ASM-055 | unit | `crates/cgf-templates/tests/resolve.rs` (registered ID, empty registry, two stub factories, unknown motion model, behavior, sensors, terrain via `scenario_file.rs`) | passing |
| AT-2 | REQ-T-2, ASM-054, ASM-056 | unit + proptest | `crates/cgf-templates/tests/{source (14), value (19), template_file (35), resolve (set rows), resolve_prop (3)}.rs`; fixtures `tests/fixtures/syntax/`; ASM-054 cases: two bad values in one inline table, two bad array elements, duplicate key plus bad value, unclosed array and inline table (one error at the opening bracket, independent error before it kept), recovered id (no `unknown_template`), unrecoverable id (note variant) | passing |
| AT-3 | REQ-T-3, ADR 0009 | unit | `crates/cgf-templates/tests/schema.rs` (55), `resolve.rs` (`..._schema_outside_subset_...`), `crates/cgf-engine/tests/builtin_schemas.rs` (6), `crates/cgf-motion/tests/factory.rs` (14; also run green against a throwaway reference factory) | passing |
| AT-4 | REQ-T-4 | unit | `crates/cgf-templates/tests/resolve.rs` (network preserved, replaced per adapter, inherited, must be table, datetime, adapter name; ADR 0010 merge rules) | passing |
| AT-5 | REQ-T-5, ASM-057 | unit | `crates/cgf-templates/tests/{template_file,scenario_file}.rs` (accepted, newer, missing, zero, negative, string, hides-nothing) | passing |
| AT-6 | REQ-T-6 | integration + process | `crates/cgf-templates/tests/discover.rs` (13; fixtures `dirs/mixed`, `scenarios/relative`), `crates/cgf-cli/tests/cli_validate.rs` (18; fixtures `tests/fixtures/bad/`, exact expected list of 7 diagnostics, `--json`) | passing |
| AT-7 | REQ-C-3, ASM-044, ASM-060 | unit + integration | `crates/cgf-templates/tests/scenario_file.rs` (64; fixtures `scenarios/{relative,missing_template_dir}`), `crates/cgf-engine/tests/scenario.rs` (16; fixture `bad_scenario/`) | passing |
| AT-8 | M2 acceptance 2, REQ-S-1, REQ-K-4 | integration (gate 4 successor) + process | `crates/cgf-engine/tests/{determinism,waypoints}.rs` (11 + 11; renamed from `example_*` in WI-11, gate 4 target unchanged), `crates/cgf-cli/tests/cli_run.rs` (incl. the 4-process byte-identical test; the demo-based `cli.rs` was deleted); example files `examples/scenarios/walking_skeleton/` | passing |
| AT-9 | REQ-K-4, NFR-2 | unit + proptest | `resolve.rs` (`shuffled_source_order_...` all 24 permutations), `resolve_prop.rs` (file order), `discover.rs` (`discovery_returns_sorted_paths`), `value.rs` (`parse_never_panics`, 2 proptests) | passing |
| AT-10 | D-14, REQ-X-2 `schema` | unit + process | `crates/cgf-templates/tests/file_schemas.rs` (8; fixtures `templates/{valid,invalid_structure}`, `scenarios/{valid,invalid_structure}`), `crates/cgf-cli/tests/cli_schema.rs` (9) | passing |
| AT-11 | gates | manual | mutations recorded in the M2 running log and review reports (e.g. HashMap iteration fails 6/11 determinism and 5/20 cli_run tests; nested unclosed containers; validate/run scope) | done |

Added in their work items: `RunSummary.stopped_early` on `ControlFlow::Break` (WI-9), the closed-pipe CLI write-error test (WI-10). REQ-T-7 (hot reload) is deferred to M5.

Gaps and notes: the plan's `validate_path_never_builds_an_engine` is `validate_path_builds_exactly_as_often_as_load_scenario` (a registry stub cannot observe an engine; the property that validation and loading share one dry-run pipeline is observable). Tests that pin only code and file (location of an absent key is not specified): missing `schema_version`, `id`, motion model; `io` for a missing `template_paths` directory; `not_a_scenario`.


## M3

Written against `todo!()` stubs (step 4, commits e2561bc, 1827784, 039f434, f4c3edc); all of it stays green through `#[ignore = "WI-n"]` on the 252 tests that cannot pass yet (the ignore names the work item that makes them pass; un-ignore in that WI). 14 of the 266 new tests pass already (marked "live"). Counts are `tests (ignored)` per file. Plan table: `docs/plans/M3.md`, "Acceptance tests for the test-engineer". Gate 4 runs the M2 `determinism` target and `behavior_determinism` since WI-16 (ASM-087); no test is ignored any more (all 14 WI-16 tests are live). `scripts/m2-compat.sh` runs the compatibility checklist items 1-3 (AT-18).

| AT | REQ-ID | Type | Test file(s) | Status |
|---|---|---|---|---|
| AT-1 | REQ-B-6, REQ-B-14, REQ-T-2, REQ-T-5 | unit + proptest | `crates/cgf-templates/tests/graph_file.rs` 23 (23 WI-12; `graph_parse_never_panics` 256 cases) | failing (stub until WI-12) |
| AT-2 | REQ-B-7 | unit | `crates/cgf-behavior/tests/{composites 14, decorators 10, fsm 15, subgraph 10}.rs`, `graph_file.rs` (`every_node_type_is_readable`) | failing (WI-8, WI-9, WI-10, WI-12) |
| AT-3 | REQ-B-8, REQ-B-1 | unit + proptest | `cgf-behavior/tests/{composites, decorators, halt 5, order_prop 1}.rs`; `cgf-kernel/tests/behavior_phase.rs` (`behavior_cannot_change_state_except_through_intents`) | failing (WI-8; kernel row WI-6) |
| AT-4 | REQ-B-7 (fsm), REQ-B-12 | unit | `cgf-behavior/tests/fsm.rs` (15, WI-9); `cgf-templates/tests/graph_resolve.rs` (5 FSM rows, WI-13) | failing (WI-9, WI-13) |
| AT-5 | REQ-B-9, REQ-B-12, REQ-B-14 | unit | `cgf-behavior/tests/subgraph.rs` (`file_builtin_and_plugin_targets_are_indistinguishable`), `register_graphs.rs` 7; `cgf-templates/tests/graph_resolve.rs` (subgraph and version rows) | failing (WI-10, WI-13) |
| AT-6 | REQ-B-10, REQ-B-12 | unit | `cgf-templates/tests/graph_resolve.rs` (call site and binding rows), `template_behavior.rs` (`call_site_*`) | failing (WI-13, WI-14) |
| AT-7 | REQ-B-11 | unit + integration | `cgf-behavior/tests/isolation.rs` 6 (WI-10); `cgf-engine/tests/behavior_scenario.rs` 1 (WI-15) | failing (WI-10, WI-15) |
| AT-8 | REQ-B-12, REQ-T-2 | unit + proptest | `cgf-templates/tests/graph_resolve.rs` (cycle, depth, shape, five errors, shuffle), `graph_resolve_prop.rs` 2 (1000 cases) | failing (WI-13) |
| AT-9 | REQ-B-13 | integration | `cgf-engine/tests/catalog.rs` (4 provenance tests) | failing (WI-15) |
| AT-10 | REQ-B-14 | unit | `graph_file.rs` (`version_is_required`, `version_must_be_semver`), `catalog.rs` (`graph_version_reaches_catalog`) | failing (WI-12, WI-15) |
| AT-11 | REQ-B-15, REQ-T-3 | unit | `cgf-behavior/tests/leaves.rs` 17 (WI-11); `graph_resolve.rs` (`bad_leaf_params_located_in_graph_file`, `expression_string_is_param_wrong_type`) | failing (WI-11, WI-13) |
| AT-12 | REQ-B-16, REQ-K-4, section 15 | integration (gate 4, second target) + process | `cgf-engine/tests/behavior_determinism.rs` 7 (WI-16); `cgf-cli/tests/cli_behavior.rs` (`patrol_and_engage_in_four_processes_is_byte_identical`) | passing (WI-16) |
| AT-13 | REQ-B-17 | integration | `cgf-engine/tests/catalog.rs` 12 (WI-15) | failing (WI-15) |
| AT-14 | REQ-B-1, REQ-B-2, REQ-K-5, ADR 0016 | unit | `cgf-kernel/tests/{agent_spawn 5, behavior_phase 9, perception_phase 8}.rs` (WI-6; 2 live), `cgf-motion/tests/set_heading.rs` 9 (4 WI-7, 5 live), `cgf-core/tests/intent.rs` (`intent_set_version_is_one`, live since WI-2) | failing (WI-6, WI-7) |
| AT-15 | Perception, REQ-K-5, ADR 0015 | integration | `cgf-engine/tests/visual_sensor.rs` 10 (WI-15) | failing (WI-15) |
| AT-16 | M3 acceptance 1 | integration + process | `cgf-engine/tests/patrol_and_engage.rs` 6 (WI-16); `cgf-cli/tests/cli_behavior.rs` (`validate_example_ok`, `run_example_prints_states_and_hash`, WI-15); example `examples/scenarios/patrol_and_engage/` | passing (WI-15, WI-16) |
| AT-17 | M3 acceptance 2 | integration | `cgf-engine/tests/behavior_swap.rs` 2 (1 live: the byte comparison of fixtures `tests/fixtures/behavior_swap/{a,b}`; run WI-16) | passing (WI-16) |
| AT-18 | M3 acceptance 4, A-001 | process + script | `cgf-cli/tests/m2_compat.rs` 3 (2 live: the M2 hash `849026656b7b42d2` on Linux, `ok: 4 files checked`; the spec 7.1 template validates, WI-15); `scripts/m2-compat.sh` | passing / failing (WI-15) |
| AT-19 | gates are real | manual mutations | recorded in the WI PRs | manual |
| AT-20 | REQ-B-11, REQ-B-10, REQ-B-12, ADR 0014 | unit + integration | `cgf-behavior/tests/input_binding.rs` 9 (WI-10); `graph_resolve.rs` (8 binding rows); `template_behavior.rs` (`key_form_in_template_behavior_params_is_invalid_binding`, `graph_with_required_binding_only_param_as_template_behavior_is_missing_param`); `patrol_and_engage.rs` (`attack_subgraph_receives_target_through_binding`) | failing (WI-10, WI-13, WI-14, WI-16) |
| WI-14 rows | REQ-B-6, REQ-T-1, ADR 0015, ASM-079 | unit | `cgf-templates/tests/{template_behavior 13 (3 live), scenario_force 6 (1 live)}.rs`; fixtures `tests/fixtures/graphs/` | failing (WI-14, WI-12) |

Gaps and notes: the ignored tests were not run against a reference implementation (M1 and M2 were); they are checked for the right failure (a `todo!()` panic, or an assertion for the tests that go through code that exists) with `cargo test -- --ignored`, and every graph text in them was parsed with the real TOML parser. The first WI that un-ignores a file should expect to fix test bugs as well as code. Tests that pin only code and file (location of an absent key is not specified): missing `schema_version`, `id`, `version`, `root`, node settings, `no_initial_state` without `initial`. AT-19 is manual.
