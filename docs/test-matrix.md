# Test matrix

REQ-ID | test type | test name | status. Status values: `failing (not implemented)`, `passing`, `CI evidence`, `manual`.
Files: `xtask/tests/check_rules.rs` (R), `xtask/tests/check_deps.rs` (D), `crates/cgf-core/tests/{time,ids,units}.rs`; M1 files are listed in the M1 table.

## M0

| AT | REQ-ID | Type | Test names (file) | Status |
|---|---|---|---|---|
| AT-1 | M0 acceptance 1, NFR-3 | CI evidence | CI run URL on M0 head: fmt, clippy, test, check-deps, deny, audit on ubuntu and windows | waived (ADR 0004, no CI); replaced by the local gate passing on Linux at e785518 |
| AT-2 | REQ-A-1 | unit | `given_core_depends_on_kernel_when_checked_then_req_a1_violation`, `given_core_with_only_external_dep_when_checked_then_no_violation` (R) | failing (WI-3) |
| AT-3 | REQ-A-2 | unit | `given_kernel_depends_on_server_when_checked_then_req_a2_violation`, `given_kernel_depends_on_net_dis_when_checked_then_req_a2_violation`, `given_kernel_depends_on_core_when_checked_then_no_violation` (R) | failing (WI-3) |
| AT-4 | REQ-A-3, section 4 | unit | `given_sdk_depends_on_core_when_checked_then_req_a3_violation`, `given_sdk_depends_on_plugin_api_when_checked_then_no_violation`, `given_plugin_api_has_external_dep_when_checked_then_violation`, `..._external_build_dep_...`, `..._external_dev_dep_...no_violation` (R) | failing (WI-3) |
| AT-5 | ASM-005, ASM-006 | unit + fixture | `given_forbidden_dev_dep...`, `given_forbidden_build_dep...`, `given_forbidden_target_specific_dep...`, `given_renamed_dep_resolved_to_kernel...`, `given_unknown_workspace_crate_when_checked_then_unknown_crate_violation`, `given_unknown_member_without_edges...` (R); `given_violating_fixture_when_loaded_*`, `given_registry_dep_when_loaded_*`, `given_*_launder_*`, `given_same_name_impostor_*` (D); `given_*_when_resolved_checked_*` (R) | failing (WI-3) |
| AT-6 | M0 acceptance 2 | integration | `given_violating_fixture_when_check_deps_runs_then_exit_code_1_and_lists_each_violation`, `..._runs_twice_then_output_is_identical`, `given_conforming_fixture_when_check_deps_runs_then_exit_code_0`, `given_real_workspace_when_check_deps_runs_then_exit_code_0`, `given_missing_manifest_when_check_deps_runs_then_exit_code_2`, `given_conforming_fixture_when_loaded_and_checked_then_no_violations` (D) | failing (WI-3) |
| AT-7 | REQ-A-1..3 | unit | `rules_table_is_acyclic_and_closed`, `kernel_closure_excludes_server_net_and_api_types`, `sdk_closure_is_plugin_api_only`, `core_closure_is_empty`, `check_output_is_sorted_and_stable`, `rules_table_matches_plan_table`, `rules_table_only_plugin_api_forbids_external_deps` (R) | failing (WI-3) |
| AT-8 | REQ-C-1, D-5 | unit + proptest | all tests in `time.rs` incl. `next_strictly_increases_tick_and_never_decreases_seconds`, `as_seconds_is_bitwise_tick_over_hz` | passing (18 tests, WI-6) |
| AT-9 | REQ-C-2, ADR 0002 | unit + proptest | all tests in `ids.rs` incl. `display_then_parse_round_trips`, `validator_matches_reference_grammar` | passing (29 tests, WI-7) |
| AT-10 | REQ-C-4 | unit + proptest | all tests in `units.rs` incl. `degrees_to_radians_and_back_round_trips`; 3 `compile_fail` doctests | passing (14 tests + 3 doctests, WI-5) |
| AT-11 | NFR-2, NFR-4, REQ-K-4 | manual mutation | clippy fails on `Instant::now`/`HashMap` (WI-1, done); `cargo deny` fails on GPL dep (WI-4); ~~fmt job fails on bad format (WI-2)~~ dropped (ADR 0004) | done; evidence in the M0 running log (incl. dev-dep GPL with `include-dev`) |

Uncovered in M0 (by design): REQ-A-4 (M8, ASM-021), REQ-K-4 determinism gate (M1, ASM-019), NFR-1 benchmarks (later).

## M1

Written against `todo!()` stubs (step 4); every row is `failing (stub until WI-n)` until that work item lands. All tests were also run green against a throwaway reference implementation outside the repo (see the M1 running log).

| AT | REQ-ID | Type | Test file(s) | Status |
|---|---|---|---|---|
| AT-1 | M1 acceptance 1, REQ-K-3, REQ-K-4, D-5 | integration (gate 4) | `crates/cgf-engine/tests/determinism.rs` (8 tests: twice AFAP, std wall clock, every clock mode with and without pause at tick 100, mode switch, seeds differ, length, run-until semantics) | failing (WI-13) |
| AT-2 | REQ-X-2, REQ-X-3, REQ-K-4 | process | `crates/cgf-cli/tests/cli.rs` (11 tests: cross-process hash, state lines, JSON Lines, exit codes 2) | failing (WI-14) |
| AT-3 | REQ-C-3, REQ-C-4, D-15 | unit + proptest | `crates/cgf-core/tests/{coords,geo,coords_prop}.rs` (30+2, 15, 8 tests; IOGP 373-7-2 9602 and 9836 examples, analytic points, axes, orientation) | failing (WI-3, WI-4) |
| AT-4 | ADR 0003, ASM-034 | unit + proptest | `crates/cgf-core/tests/math.rs` (7) | failing (WI-2) |
| AT-5 | REQ-K-4, ADR 0006 | unit + proptest | `crates/cgf-core/tests/rng.rs` (18; reference vectors from the reference C code) | failing (WI-6) |
| AT-6 | REQ-K-4, ADR 0007 | unit | `crates/cgf-core/tests/hash.rs` (14); `crates/cgf-kernel/tests/state_hash.rs` (8) | failing (WI-5, WI-10) |
| AT-7 | REQ-K-3, REQ-K-8, REQ-C-1 | unit + proptest | `crates/cgf-kernel/tests/clock.rs` (21), `clock_prop.rs` (1) | failing (WI-9) |
| AT-8 | REQ-M-2, REQ-M-1, REQ-M-4 | unit + proptest | `crates/cgf-motion/tests/ground_kinematic.rs` (20) | failing (WI-8) |
| AT-9 | M1 acceptance 3 | integration | `crates/cgf-engine/tests/waypoints.rs` (8); route stand-in in `crates/cgf-kernel/tests/pipeline.rs` | failing (WI-11, WI-13) |
| AT-10 | REQ-K-2, REQ-K-5, REQ-C-2, REQ-K-6, REQ-K-8 | unit / integration | `crates/cgf-kernel/tests/{pipeline,world}.rs` (8 + 8) | failing (WI-10, WI-11); `phase_order_is_spec_order` passes (data constant) |
| WI-7 | REQ-M-1, REQ-K-5, REQ-C-6, REQ-B-2 | unit | `crates/cgf-core/tests/{motion_contract,terrain_ownership}.rs` (8 + 7) | failing (WI-7) |
| AT-11 | NFR-1 | build | `cargo bench -p cgf-kernel --no-run --locked` | not written here (WI-12) |
| AT-12 | gates | manual | mutations recorded in WI PRs | manual |
| AT-15 | ASM-044, D-15 | integration | `crates/cgf-core/tests/coords.rs` (`position_*`), `crates/cgf-engine/tests/placement.rs` (5) | failing (WI-4, WI-13) |

Gaps: plan test `entity_rng_stream_is_independent_of_other_spawns` and the archetype-order variant of `hash_walks_entities_in_id_order` cannot be expressed through the public kernel API (no rng accessor, no despawn); covered by `drawing_from_one_stream_does_not_affect_another` (core) and the seed/RNG hash sensitivity test. Intents are not directly observable in the hash test.
