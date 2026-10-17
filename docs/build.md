# Build and verify

Toolchain is pinned in `rust-toolchain.toml` (Rust 1.99.1, edition 2024, resolver 3); rustup installs it automatically.

## Prerequisites
```
cargo install --locked cargo-deny@0.20.2 cargo-audit@0.22.2
```

## Local gate (canonical, ADR 0004 / D-19)
There is no CI gate: this list, run in this order, is the definition of done (CLAUDE.md rule 8, NFR-3/4/5). Stop at the first failure; fix the cause, never weaken a gate. The `verify-and-build` skill follows this list. Commands are identical on Linux and Windows; on Windows also check path handling and `.dll` plugin loading, on Linux `.so`.

| # | Gate | Command | Active |
|---|---|---|---|
| 1 | Format | `cargo fmt --all -- --check` | M0 |
| 2 | Lint | `cargo clippy --workspace --all-targets --locked -- -D warnings` | M0 |
| 3 | Tests | `cargo test --workspace --locked` | M0 |
| 4 | Determinism (REQ-K-4) | `cargo test -p cgf-engine --test determinism --locked` | M1 (active from WI-13; skipped in M0, ASM-019); from M2 the subject is `examples/scenarios/walking_skeleton` |
| 5 | Dependency rules (REQ-A-1..3, NFR-2) | `cargo xtask check-deps` | M0 |
| 6 | Licenses, bans, sources, advisories (NFR-4) | `cargo deny check` | M0 |
| 7 | Vulnerabilities in `Cargo.lock` | `cargo audit` | M0 |
| 8 | Docs, warnings are errors | `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked` | M0 |
| 9 | Release build | `cargo build --workspace --release --locked` | M0 |
| 10 | Schema drift | `cargo xtask gen-api --check` | from M5 |
| 11 | C header drift | `cargo xtask gen-header --check` | from M4 |
| 12 | Plugin build | `cargo xtask build-plugins --release` | from M4 |
| 13 | Plugin failure matrix | `cargo test -p cgf-plugin-host --test plugin_failures` | from M4 |
| 14 | Benchmarks compile | `cargo bench -p cgf-kernel --bench tick --no-run --locked` | from M1 |
| 15 | GUI | `(cd gui && bun install --frozen-lockfile && bun run gen:api --check && bun run typecheck && bun run lint && bun run check:deps && bun test && bun run build)`, then `bun run tauri build` on Windows | from M8 (see the `verify-frontend` skill) |

Rows marked "from Mn" are skipped, and reported as skipped, before that milestone exists. `--locked` is used on every cargo command that supports it (`cargo deny` and `cargo audit` read the lockfile as is; the `cargo xtask` alias builds with `--locked`, and `check-deps` runs `cargo metadata --locked`). Any non-zero exit fails the gate; in particular exit 2 from `cargo xtask check-deps` (load error, for example no resolved graph) fails it and is not a skip. `--locked` requires `Cargo.lock` to be committed and current. Text files use LF (`.gitattributes`, ASM-014).

## `cargo xtask` subcommands
`cargo xtask` is an alias (`.cargo/config.toml`) for `cargo run --locked --package xtask --`. It works the same on Windows and Linux.

| Command | Purpose |
|---|---|
| `cargo xtask check-deps [--manifest-path <path>]` | Verifies the dependency rules REQ-A-1..3 in two passes against the table in `xtask/src/deps/rules.rs`: (1) every declared direct edge (`cargo metadata --no-deps`; all kinds, targets, renames; unknown workspace crates and non-member path dependencies are violations, ASM-006); (2) the resolved graph by package id (`cargo metadata --offline --locked --all-features`, all platforms), which catches laundering through `[patch]`, `[replace]`, git, registry or same-name packages and rejects every non-member path package, however it entered the graph (ASM-005, ASM-006). Declared dependencies that use a workspace crate name without being a path dependency on that member are violations too. Also checks that every member sets `lints.workspace = true` (parsed with `toml`) and that every target root (lib, bin, test, example, bench, build script) outside `cgf-plugin-api`, `cgf-plugin-host`, `cgf-sdk` has `#![forbid(unsafe_code)]` (parsed with `syn`; NFR-2, ASM-024). With `--manifest-path`, cargo runs in that manifest's directory so its `.cargo/config.toml` applies. Prints one line per violation, sorted. Exit 0 clean, 1 violations, 2 load or usage error (also: missing or stale `Cargo.lock`, or a dependency not in the local cache; run `cargo fetch --locked` once online). The declared-pass violations are printed before the resolved graph is loaded, so they are visible even when it then fails. Exit 2 fails the gate (it is not a skip). |

Example violation line: `REQ-A-1: cgf-core -> cgf-kernel (normal) not allowed; cgf-core may depend on: (none)`.
Negative tests: `cargo test -p xtask` (fixtures in `xtask/tests/fixtures/`, see the README there).

More `cargo xtask` subcommands will be documented here as they are added.

## Continuous integration (dormant reference)
`.github/workflows/ci.yml` mirrors the gate list above but is manual-only (`workflow_dispatch`) and is not a gate (ADR 0004 / D-19). Nothing runs it automatically; do not rely on it.

## Supply chain (NFR-4, NFR-6, ADR 0001)
- Licenses: `LICENSE-MIT` and `LICENSE-APACHE` at the root; every crate inherits `license = "MIT OR Apache-2.0"` from `[workspace.package]`.
- `deny.toml`: license allow-list limited to what the current tree uses (MIT, Apache-2.0, Unicode-3.0; ASM-020), copyleft denied; wildcards denied (path wildcards allowed); unknown registries and git sources denied; yanked crates denied; duplicate versions warn. Run `cargo deny check` and `cargo audit` locally before done; a new advisory can fail them with no code change: triage it, never silence it.
- Adding a dependency: run `cargo deny check`; if it brings a new license, add it to `deny.toml` in the same PR with a note (ASM-020).
- Dev-dependency licenses are checked too (`include-dev = true` in `[licenses]`); the WI-4 mutation check showed that without it a GPL `[dev-dependencies]` entry passes.
