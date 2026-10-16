# Test matrix

REQ-ID | test type | test name | status. Status values: `failing (not implemented)`, `passing`, `CI evidence`, `manual`.
Files: `xtask/tests/check_rules.rs` (R), `xtask/tests/check_deps.rs` (D), `crates/cgf-core/tests/{time,ids,units}.rs`.

## M0

| AT | REQ-ID | Type | Test names (file) | Status |
|---|---|---|---|---|
| AT-1 | M0 acceptance 1, NFR-3 | CI evidence | CI run URL on M0 head: fmt, clippy, test, check-deps, deny, audit on ubuntu and windows | pending (needs remote, WI-2) |
| AT-2 | REQ-A-1 | unit | `given_core_depends_on_kernel_when_checked_then_req_a1_violation`, `given_core_with_only_external_dep_when_checked_then_no_violation` (R) | failing (WI-3) |
| AT-3 | REQ-A-2 | unit | `given_kernel_depends_on_server_when_checked_then_req_a2_violation`, `given_kernel_depends_on_net_dis_when_checked_then_req_a2_violation`, `given_kernel_depends_on_core_when_checked_then_no_violation` (R) | failing (WI-3) |
| AT-4 | REQ-A-3, section 4 | unit | `given_sdk_depends_on_core_when_checked_then_req_a3_violation`, `given_sdk_depends_on_plugin_api_when_checked_then_no_violation`, `given_plugin_api_has_external_dep_when_checked_then_violation`, `..._external_build_dep_...`, `..._external_dev_dep_...no_violation` (R) | failing (WI-3) |
| AT-5 | ASM-005, ASM-006 | unit + fixture | `given_forbidden_dev_dep...`, `given_forbidden_build_dep...`, `given_forbidden_target_specific_dep...`, `given_renamed_dep_resolved_to_kernel...`, `given_unknown_workspace_crate_when_checked_then_unknown_crate_violation`, `given_unknown_member_without_edges...` (R); `given_violating_fixture_when_loaded_*`, `given_path_dep_outside_workspace_when_loaded_*` (D) | failing (WI-3) |
| AT-6 | M0 acceptance 2 | integration | `given_violating_fixture_when_check_deps_runs_then_exit_code_1_and_lists_each_violation`, `..._runs_twice_then_output_is_identical`, `given_conforming_fixture_when_check_deps_runs_then_exit_code_0`, `given_real_workspace_when_check_deps_runs_then_exit_code_0`, `given_missing_manifest_when_check_deps_runs_then_exit_code_2`, `given_conforming_fixture_when_loaded_and_checked_then_no_violations` (D) | failing (WI-3) |
| AT-7 | REQ-A-1..3 | unit | `rules_table_is_acyclic_and_closed`, `kernel_closure_excludes_server_net_and_api_types`, `sdk_closure_is_plugin_api_only`, `core_closure_is_empty`, `check_output_is_sorted_and_stable`, `rules_table_matches_plan_table`, `rules_table_only_plugin_api_forbids_external_deps` (R) | failing (WI-3) |
| AT-8 | REQ-C-1, D-5 | unit + proptest | all tests in `time.rs` incl. `next_strictly_increases_tick_and_never_decreases_seconds` | failing (WI-6) |
| AT-9 | REQ-C-2, ADR 0002 | unit + proptest | all tests in `ids.rs` incl. `display_then_parse_round_trips` | failing (WI-7) |
| AT-10 | REQ-C-4 | unit + proptest | all tests in `units.rs` incl. `degrees_to_radians_and_back_round_trips` | failing (WI-5) |
| AT-11 | NFR-2, NFR-4, REQ-K-4 | manual mutation | clippy fails on `Instant::now`/`HashMap` (WI-1, done); `cargo deny` fails on GPL dep (WI-4); fmt job fails on bad format (WI-2) | manual, recorded in PRs |

Uncovered in M0 (by design): REQ-A-4 (M8, ASM-021), REQ-K-4 determinism gate (M1, ASM-019), NFR-1 benchmarks (later).
