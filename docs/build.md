# Build and verify

Toolchain is pinned in `rust-toolchain.toml` (Rust 1.99.1, edition 2024, resolver 3); rustup installs it automatically.

## Prerequisites
```
cargo install --locked cargo-deny@0.20.2 cargo-audit@0.22.2
cargo install --locked cbindgen@0.29.4
```
From M4 also a C compiler (gcc or clang on Linux and macOS, MSVC on Windows), found by the `cc` crate
(`CC` overrides it). `cbindgen` (MPL-2.0) is a pinned tool, never a dependency (ADR 0023 item 2, ASM-009):
`cargo xtask gen-header` refuses any other `cbindgen --version` and runs `cbindgen` from `PATH` or the
program named by the `CBINDGEN` environment variable. Gate 3 needs both too (`xtask/tests/gen_header.rs`,
`crates/umbra-plugin-host/tests/c_header.rs`, `c_example.rs`).

Which gates need what (from M4):

| Prerequisite | Gates |
|---|---|
| `cbindgen` 0.29.4 | 3 (`gen_header.rs`), 11 |
| C compiler | 3 (`c_header.rs`, `c_example.rs`, the no-entry and null-entry fixtures of `plugin_failures.rs`), 12 (`c-circle`), 13 |
| Offline cargo cache (`cargo fetch --locked` once online) | 3, 4, 13: tests build plugin libraries with a nested `cargo build --locked --offline` into `CARGO_TARGET_TMPDIR` (ADR 0023 item 4) |

Plugin builds inside tests use about 630 MB under `target/tmp/plugin-build` (M4 WI-10). Windows (MSVC)
and macOS paths of gates 3, 12 and 13 were not run in M4 (NFR-3 unverified, human decision 2026-10-19).

## Local gate (canonical, ADR 0004 / D-19)
`scripts/gates.sh` runs this list in order with one summary line per gate, plus `scripts/m2-compat.sh` and the golden state hashes, and keeps each gate's log in `target/gates/`. `scripts/gates.sh --quick CRATE...` (fmt, clippy and tests of those crates) is for use during a work item; it is not the definition of done.

There is no CI gate: this list, run in this order, is the definition of done (CLAUDE.md rule 8, NFR-3/4/5). Stop at the first failure; fix the cause, never weaken a gate. The `verify-and-build` skill follows this list. Commands are identical on Linux and Windows; on Windows also check path handling and `.dll` plugin loading, on Linux `.so`.

| # | Gate | Command | Active |
|---|---|---|---|
| 1 | Format | `cargo fmt --all -- --check` | M0 |
| 2 | Lint | `cargo clippy --workspace --all-targets --locked -- -D warnings` | M0 |
| 3 | Tests | `cargo test --workspace --locked` | M0 |
| 4 | Determinism (REQ-K-4) | `cargo test -p umbra-engine --test determinism --test behavior_determinism --test plugin_determinism --test handle_determinism --test net_determinism --locked` | M1 (skipped in M0, ASM-019); from M2 `determinism` runs `examples/scenarios/walking_skeleton`; from M3 (ASM-087) `behavior_determinism` also runs `examples/scenarios/patrol_and_engage`; from M4 (WI-13, ASM-115) `plugin_determinism` runs `examples/scenarios/plugin_wander` with the `example-wander` plugin, built by the tests' build helper; from M5 (WI-10, ASM-155) `handle_determinism` runs the three reference scenarios through `EngineHandle` and compares every tick with an `Engine` run; from M6 (WI-9, AT-14) `net_determinism` steps two engines over one loopback hub and compares the per-tick hashes of both across runs, every clock mode and a mid-run pause |
| 5 | Dependency rules (REQ-A-1..3, NFR-2) | `cargo xtask check-deps` | M0 |
| 6 | Licenses, bans, sources, advisories (NFR-4) | `cargo deny check` | M0 |
| 7 | Vulnerabilities in `Cargo.lock` | `cargo audit` | M0 |
| 8 | Docs, warnings are errors | `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked` | M0 |
| 9 | Release build | `cargo build --workspace --release --locked` | M0 |
| 10 | Schema drift | `cargo xtask gen-api --check` | from M5 (active from WI-12) |
| 11 | C header drift (REQ-SDK-2) | `cargo xtask gen-header --check` | from M4 (active from WI-4); needs `cbindgen` 0.29.4 |
| 12 | Plugin build | `cargo xtask build-plugins --release` | from M4 (active from WI-13); packages `wander` and `c-circle` (C compiler) |
| 13 | Plugin failure matrix (REQ-P-7..9) | `cargo test -p umbra-plugin-host --test plugin_failures --locked` | from M4 (active from WI-10); builds the `umbra-test-plugin` variants and two C fixtures |
| 14 | Benchmarks compile | `cargo bench -p umbra-kernel --bench tick --no-run --locked` | from M1 |
| 15 | GUI | `(cd gui && bun install --frozen-lockfile && bun run gen:api --check && bun run typecheck && bun run lint && bun run check:deps && bun test && bun run build)`, then `bun run tauri build` on Windows | from M8 (see the `verify-frontend` skill) |
| 16 | Engine-only CLI (REQ-S-1, REQ-G-1, D-1, D-10) | `cargo build -p umbra-cli --no-default-features --locked` and `cargo clippy -p umbra-cli --no-default-features --all-targets --locked -- -D warnings` | from M5 (WI-17, ASM-218) |

Rows marked "from Mn" are skipped, and reported as skipped, before that milestone exists. `--locked` is used on every cargo command that supports it (`cargo deny` and `cargo audit` read the lockfile as is; the `cargo xtask` alias builds with `--locked`, and `check-deps` runs `cargo metadata --locked`). Any non-zero exit fails the gate; in particular exit 2 from `cargo xtask check-deps` (load error, for example no resolved graph) fails it and is not a skip. `--locked` requires `Cargo.lock` to be committed and current. Text files use LF (`.gitattributes`, ASM-014).

## `cargo xtask` subcommands
`cargo xtask` is an alias (`.cargo/config.toml`) for `cargo run --locked --package xtask --`. It works the same on Windows and Linux.

| Command | Purpose |
|---|---|
| `cargo xtask check-deps [--manifest-path <path>]` | Verifies the dependency rules REQ-A-1..3 in two passes against the table in `xtask/src/deps/rules.rs`: (1) every declared direct edge (`cargo metadata --no-deps`; all kinds, targets, renames; unknown workspace crates and non-member path dependencies are violations, ASM-006); (2) the resolved graph by package id (`cargo metadata --offline --locked --all-features`, all platforms), which catches laundering through `[patch]`, `[replace]`, git, registry or same-name packages and rejects every non-member path package, however it entered the graph (ASM-005, ASM-006). Protocol neutrality (M6, ADR 0030, ASM-226, ASM-256): `umbra-core`, `umbra-kernel`, `umbra-net` and `umbra-engine`, and every workspace crate they may depend on (`protocol_scope`, so a protocol crate cannot enter through `umbra-templates`), must not depend on a crate whose name matches the anchored protocol patterns (`PROTOCOL_NEUTRAL`, `is_protocol_crate` in `rules.rs`), directly or through non-member packages in the resolved graph; the source half of the rule is `crates/umbra-net/tests/protocol_neutral.rs` (code, `build.rs`, `[features]` keys; ASM-259). Declared dependencies that use a workspace crate name without being a path dependency on that member are violations too. Also checks that every member sets `lints.workspace = true` (parsed with `toml`) and that every target root (lib, bin, test, example, bench, build script) outside `umbra-plugin-api`, `umbra-plugin-host`, `umbra-sdk` has `#![forbid(unsafe_code)]` (parsed with `syn`; NFR-2, ASM-024). With `--manifest-path`, cargo runs in that manifest's directory so its `.cargo/config.toml` applies. Prints one line per violation, sorted. Exit 0 clean, 1 violations, 2 load or usage error (also: missing or stale `Cargo.lock`, or a dependency not in the local cache; run `cargo fetch --locked` once online). The declared-pass violations are printed before the resolved graph is loaded, so they are visible even when it then fails. Exit 2 fails the gate (it is not a skip). |
| `cargo xtask gen-api [--check] [--api-dir <dir>]` | Runs the `gen_api` example of `umbra-server` (`cargo run --locked --offline -q -p umbra-server --example gen_api`; xtask itself keeps no workspace dependency, ASM-002) and writes `api/openapi.json` (OpenAPI 3.1.0, `info.version` = `API_VERSION`), `api/stream.schema.json` (JSON Schema 2020-12, `$id` `urn:umbra:schema:stream:v1`, root `StreamMessage`) and `api/asyncapi.json` (AsyncAPI 3.0) to `api/` (or `<dir>`); with `--check` compares instead, with line endings normalised to LF (REQ-X-4, REQ-X-6, gate 10, ADR 0025). The files are LF, pretty, with sorted keys; two generations are byte-identical. A missing file is drift. Exit 0 current or written, 1 drift (prints each differing file and the first differing line), 2 on an unknown or repeated argument, a failed example build or run, or an I/O error. **Every work item that changes a route, a DTO or a stream message runs `cargo xtask gen-api` and commits `api/*.json` with it**; gate 10 and `umbra-server/tests/contract.rs` (`served_openapi_equals_committed_file`) fail otherwise. Then hand the contract change to `frontend-engineer` (skill `generate-api-client`). `docs/api.md` is checked against these files, `ProblemCode::ALL` and `config.v1.schema.json` by `crates/umbra-server/tests/docs_api.rs` (gate 3); update its tables in the same change. `--check` also reports a `.json` file in the directory that no document generates (delete it). An explicit relative `--api-dir` is relative to the current directory; the default `api/` is relative to the workspace root. |
| `cargo xtask gen-header [--check] [--crate-dir <dir>]` | Runs the pinned `cbindgen` (0.29.4) on `crates/umbra-plugin-api` (or `<dir>`) with its `cbindgen.toml` and writes `include/umbra_plugin_api.h`; with `--check` compares instead, with line endings normalised to LF (REQ-SDK-2, gate 11, ADR 0023 item 2). The crate metadata comes from `cargo metadata --locked --offline --all-features` run by xtask (written under `target/gen-header/`, passed as `cbindgen --metadata`), so cbindgen never resolves or updates `Cargo.lock`. Exit 0 current or written, 1 drift (prints the first differing line; run `cargo xtask gen-header` and commit the header), 2 if `cbindgen` is missing or another version (prints the install line), if the locked offline metadata fails (missing or stale `Cargo.lock`), or on a usage or I/O error. |
| `cargo xtask build-plugins [--release]` | Builds the example plugins (`cargo build --locked -p example-wander`, release profile with `--release`; the C example `examples/plugins/c-circle/plugin.c` with the `cc` crate against the generated header: GCC/Clang `-std=c11 -pedantic -Wall -Wextra -Werror -ffp-contract=off -fvisibility=hidden -shared -fPIC`, MSVC `/std:c11 /W4 /WX /fp:precise /LD`, plus `-O2`/`/O2` with `--release`, objects under `<target dir>/<profile>/c-plugins/c-circle/`; ADR 0023 item 3, ASM-148) and writes one package per plugin to `<cargo target dir>/plugins/<name>/` (`wander`, `c-circle`): the library plus the example's `plugin.toml` (ADR 0022 layout). Every library is built before any package is written. The target directory is the one `cargo metadata` reports, so `CARGO_TARGET_DIR` and `build.target-dir` are honoured; each package directory is replaced as a whole (gate 12, ASM-125 d, ASM-147). It assumes the default target layout (`<target dir>/<debug|release>/`): with a cargo config `build.target = "<triple>"` the library lands under `<target dir>/<triple>/` and `build-plugins` fails with `MissingLibrary`. Run the examples with `umbra run examples/scenarios/plugin_wander/scenario.toml --plugin-path target/plugins` and `umbra run examples/scenarios/plugin_c_circle/scenario.toml --plugin-path target/plugins` (ASM-113). Exit 0 built, 2 on an unknown argument (nothing is built or written), a failed build or C compile (the compiler output is printed), a missing C compiler, or an I/O error. |

Example violation line: `REQ-A-1: umbra-core -> umbra-kernel (normal) not allowed; umbra-core may depend on: (none)`.
Negative tests: `cargo test -p xtask` (fixtures in `xtask/tests/fixtures/`, see the README there). The C layout of the header is checked by `crates/umbra-plugin-host/tests/c_header.rs` (gate 3): `_Static_assert`s generated from the Rust layout, compiled with `-std=c11 -pedantic -Wall -Wextra -Werror` (MSVC `/std:c11 /W4 /WX`).

More `cargo xtask` subcommands will be documented here as they are added.

## Continuous integration (dormant reference)
`.github/workflows/ci.yml` mirrors the gate list above but is manual-only (`workflow_dispatch`) and is not a gate (ADR 0004 / D-19). Nothing runs it automatically; do not rely on it.

## Supply chain (NFR-4, NFR-6, ADR 0001)
- Licenses: `LICENSE-MIT` and `LICENSE-APACHE` at the root; every crate inherits `license = "MIT OR Apache-2.0"` from `[workspace.package]`.
- `deny.toml`: license allow-list limited to what the current tree uses (MIT, Apache-2.0, Unicode-3.0, Zlib for `foldhash` (ADR 0005), ISC for `libloading` (ADR 0023 item 1, ASM-133), BSD-3-Clause for `matchit`, axum's router (M5 WI-1, ADR 0025); ASM-020), copyleft denied; `cbindgen` (MPL-2.0) is an installed tool and never enters `Cargo.lock`; wildcards denied (path wildcards allowed); unknown registries and git sources denied; yanked crates denied; duplicate versions warn. Run `cargo deny check` and `cargo audit` locally before done; a new advisory can fail them with no code change: triage it, never silence it.
- Adding a dependency: run `cargo deny check`; if it brings a new license, add it to `deny.toml` in the same PR with a note (ASM-020).
- Server dependencies (M5 WI-1, ADR 0025, ASM-166), used only by `umbra-api-types` and `umbra-server` (dev entries in `umbra-server`; the engine crates gain none). Direct dependencies and the versions in `Cargo.lock` on 2026-10-19 (`tokio` 1.53.2, `hyper` 1.11.1 and `smallvec` 1.16.2 were pinned one release back at fetch time; there is no age rule, ASM-166):

  | Crate | Version | Used by | Licence |
  |---|---|---|---|
  | `axum` (features `http1`, `json`, `query`, `ws`, `tokio`) | 0.8.9 | `umbra-server` | MIT |
  | `tokio` (`rt-multi-thread`, `net`, `sync`, `time`, `macros`, `signal`; dev `test-util`) | 1.53.2 | `umbra-server` | MIT |
  | `utoipa` (`macros` only; never `preserve_order`) | 6.0.0 (`utoipa-gen` 6.0.1) | `umbra-api-types`, `umbra-server` | MIT OR Apache-2.0 |
  | `serde` (derive) | 1.0.229 | `umbra-api-types`, `umbra-server` | MIT OR Apache-2.0 |
  | `futures-util` | 0.3.34 | `umbra-server` | MIT OR Apache-2.0 |
  | `tower` (`util`), `tokio-tungstenite` (`connect`) | 0.5.3, 0.29.0 | dev, `umbra-server` | MIT |
  | `tungstenite` (`handshake`) | 0.29.0 | pulled by `tokio-tungstenite` (also the planned sync test client of `umbra-cli`) | MIT OR Apache-2.0 |
  | `matchit` (via axum) | 0.8.4 | `umbra-server` | MIT AND BSD-3-Clause |

  New in `Cargo.lock`: 57 external packages (direct and transitive) plus the two new workspace crates. Every one can be used under MIT or Apache-2.0 (some are offered as `... OR BSL-1.0`, `... OR LGPL-2.1-or-later` or `... WITH LLVM-exception`, and cargo-deny takes the allowed alternative) except `matchit` (`MIT AND BSD-3-Clause`); `cargo deny list` is the full inventory. New duplicate versions in `Cargo.lock` (warnings only): `rand` 0.9/0.10 with `rand_core`, `getrandom` 0.3/0.4, `cpufeatures` 0.2/0.3 (via `tungstenite`'s `rand` 0.9), alongside the existing `syn` 2/3. `cargo deny check` and `cargo audit` pass with the new tree (`syn` 2 and 3 appear as a duplicate warning, as before). The default `umbra-cli` build (feature `server`) compiles the whole server stack; `umbra-cli --no-default-features` builds without it. Workspace `tungstenite` is unused until the stream WI (WI-16/17 test clients).
- Release packaging must ship third-party licence notices for the shipped dependency tree, including `matchit`'s BSD-3-Clause text (derived from julienschmidt/httprouter) (NFR-4).
- Dev-dependency licenses are checked too (`include-dev = true` in `[licenses]`); the WI-4 mutation check showed that without it a GPL `[dev-dependencies]` entry passes.
