# Architecture

Source of truth: `docs/SPEC.md` section 4. This file is started in M0 WI-1 and grows with each milestone.

## Crates (current)
| Crate | Path | Role | Workspace deps |
|---|---|---|---|
| `cgf-core` | `crates/cgf-core` | IDs, time, units; `math` (single site for non-exact float functions, ADR 0003), `geo` (Vec3, Quaternion, Euler), `coords` (WGS-84, ECEF, geodetic, ENU/NED, LocalFrame, Position), `hash` (canonical state hasher, ADR 0007), `rng` (SimRng, ADR 0006), and the motion domain `intent`, `ownership`, `terrain`, `motion` (M1), `params` (`ParamValue` = `serde_json::Value`, `ParamSchema`) and `registry` (`Registry`, `Factory<T>`, `ComponentKind`; M2, ADR 0008) | none (REQ-A-1); external: `thiserror`, `serde_json` |
| `cgf-kernel` | `crates/cgf-kernel` | Clock (four modes, wall-clock site), hecs-backed world, fixed six-phase tick pipeline, state hash, snapshot (M1) | `cgf-core` (REQ-A-2) |
| `cgf-motion` | `crates/cgf-motion` | Built-in motion models, `builtin.ground.kinematic` (M1) | `cgf-core` |
| `cgf-templates` | `crates/cgf-templates` | Template and scenario reading, inheritance, schema validation, located diagnostics (M2; WI-1 scaffold, no modules yet) | `cgf-core` only (section 4) |
| `cgf-engine` | `crates/cgf-engine` | Embeddable synchronous facade, placement by ECEF or lat/lon/alt, the M1 hard-coded demo is replaced by `examples/scenarios/walking_skeleton/` (M2); built-in registry, `builtin.terrain.flat`, scenario load/validate/instantiate, `ControlFlow` run callbacks (M2) | `cgf-core`, `cgf-kernel`, `cgf-motion`, `cgf-templates` |
| `cgf-cli` | `crates/cgf-cli` | Binary; `run`, `validate`, `schema` commands (M2; `demo` removed) | `cgf-engine` |

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
- Math (ADR 0003, ASM-034): every non-exact float function (trig, inverse trig, hyperbolic, exp/log, `powi`, `powf`, `cbrt`, `hypot`) goes through `cgf_core::math`, the one place that may call the `std` methods and so the one swap point for `libm`. `clippy.toml` bans the direct `f64`/`f32` methods (and `mul_add`) workspace-wide with the reason "ADR 0003: use cgf_core::math"; `sqrt`, `abs`, `floor`, `to_radians` and the basic operators stay allowed because IEEE 754 makes them exact.
- Randomness (ASM-042): `clippy.toml` also bans the `rand` entry points `random`, `thread_rng`, `rng`, `OsRng`, `ThreadRng`, `SeedableRng::from_entropy` and `from_os_rng`, the `getrandom` functions and the `fastrand` entry points (paths that do not resolve use `allow-invalid`), `RandomState` and `DefaultHasher` (ADR 0007: use `StateHasher`), and the types `hashbrown::HashMap`/`HashSet`. Simulation randomness comes from `SimRng` streams (ADR 0006).
- Wall time (ASM-035): `Instant::now` is allowed in exactly one place, a private module of `crates/cgf-kernel/src/clock/wall.rs` (`StdWallClock`), through one `#[expect(clippy::disallowed_methods, reason = "REQ-K-4: ...")]`; everything else reads time through `WallClock` or `Clock`.
- `allow_attributes` and `allow_attributes_without_reason` are `warn`: use `#[expect(..., reason = "...")]` (ASM-012).
- Binaries and tests may print (`print_stdout`/`print_stderr` are `warn`; tests are exempt, ASM-023).
