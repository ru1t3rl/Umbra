# Milestone tracker

Mark acceptance criteria with evidence (test names / command output summary). Source: spec section 17.

- [x] M0 Foundations (2026-10-17, tag `m0-done`; plan and log in `docs/plans/M0.md`)
  - [x] ~~CI green on Windows + Linux~~: **waived** by the human (ADR 0004 / D-19, no CI). Replaced by: full local gate (`docs/build.md`) passes on Linux at `e785518`: fmt, clippy `-D warnings`, test (187 passed, 0 failed), `cargo xtask check-deps` ok, `cargo deny check` ok, `cargo audit` no findings, `cargo doc -D warnings` ok, release build ok; determinism and from-Mn gates skipped (ASM-019). Windows not run (NFR-3 unverified).
  - [x] Dependency-rule check fails when a rule is violated (tested): `xtask/tests/check_rules.rs` (AT-2..AT-5, AT-7), `xtask/tests/check_deps.rs` (AT-5, AT-6: `given_violating_fixture_when_check_deps_runs_then_exit_code_1_and_lists_each_violation`, laundering fixtures `launder_patch`, `launder_config_patch`, `launder_same_name`, `launder_fake_external`, `launder_unsafe_patch` exit 1), `xtask/tests/check_unsafe.rs`, `xtask/tests/check_lints.rs`, `xtask/tests/repo_config.rs`. Mutation checks recorded in the M0 running log.
  - Deliverables: workspace and lints (WI-1), dormant manual-only CI reference (WI-2), `cargo xtask check-deps` (WI-3), MIT OR Apache-2.0 + `deny.toml` (WI-4, ADR 0001), `cgf-core` units/time/IDs (WI-5..7, AT-8..AT-10).
- [ ] M1 Walking skeleton
- [ ] M2 Templates and scenarios
- [ ] M3 Declarative behavior
- [ ] M4 Plugin ABI v0 and SDK
- [ ] M5 Engine facade and control (REST + WebSocket)
- [ ] M6 Networking and DIS
- [ ] M7 More plugin kinds and replay
- [ ] M8 GUI and Windows app
- [ ] M9 HLA and hardening
