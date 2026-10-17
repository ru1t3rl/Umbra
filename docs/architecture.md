# Architecture

Source of truth: `docs/SPEC.md` section 4. This file is started in M0 WI-1 and grows with each milestone.

## Crates (current)
| Crate | Path | Role | Workspace deps |
|---|---|---|---|
| `cgf-core` | `crates/cgf-core` | IDs, time, units (empty skeleton in WI-1) | none (REQ-A-1) |

Planned crates (section 4) are added in their own milestones; no placeholders (ASM-010).

## Dependency rules
- REQ-A-1: `cgf-core` depends on no workspace crate.
- REQ-A-2: the kernel never depends on server, network adapters or API types.
- REQ-A-3: `cgf-sdk` depends only on `cgf-plugin-api`; `cgf-plugin-api` depends on nothing.
- REQ-A-4: GUI rule, enforced in `gui/` (M8, ASM-021).
- Enforcement: `cargo xtask check-deps` (`xtask/src/deps/`), the only gate (no CI, ADR 0004). Pass 1 checks every declared direct edge (normal, build, dev, target-specific, renamed) of every workspace member against the rule table in `xtask/src/deps/rules.rs` (ASM-005..007); membership is by path, and non-member path dependencies are rejected (ASM-006). Pass 2 checks the resolved graph by package id (all features, all platforms, `--offline --locked`), so a forbidden member cannot be reached through `[patch]`, git, registry or same-name packages, and no non-member path package (whose code would escape the unsafe and lints checks) can enter the build (ASM-005, ASM-006). It also checks that every target root outside `cgf-plugin-api`, `cgf-plugin-host` and `cgf-sdk` has `#![forbid(unsafe_code)]` (parsed with `syn`) and that every member inherits the workspace lints (parsed with `toml`) (ASM-024). Negative tests run it against `xtask/tests/fixtures/` (`violating`, `launder_*`, `unsafe_*`, `lints_only`).

## Code-level guards (workspace-wide)
- `unsafe_code = "deny"` in `[workspace.lints]`; non-FFI crates also `#![forbid(unsafe_code)]` (NFR-2, ASM-012).
- `clippy.toml` bans `SystemTime::now`/`elapsed`, `Instant::now`/`elapsed`, `thread::sleep`, `HashMap`, `HashSet` and `RandomState` (REQ-K-4, ASM-013). A `HashMap` from a third-party API iterated without naming the type remains a review item.
- `allow_attributes` and `allow_attributes_without_reason` are `warn`: use `#[expect(..., reason = "...")]` (ASM-012).
- Binaries and tests may print (`print_stdout`/`print_stderr` are `warn`; tests are exempt, ASM-023).
