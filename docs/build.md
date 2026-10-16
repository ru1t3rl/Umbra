# Build and verify

Toolchain is pinned in `rust-toolchain.toml` (Rust 1.99.1, edition 2024, resolver 3); rustup installs it automatically.

## Commands (Linux and Windows)
| Purpose | Command |
|---|---|
| Format check | `cargo fmt --all -- --check` |
| Lint | `cargo clippy --workspace --all-targets --locked -- -D warnings` |
| Tests | `cargo test --workspace --locked` |
| Docs (warnings are errors) | `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked` |

`--locked` requires `Cargo.lock` to be committed and current. Text files use LF (`.gitattributes`, ASM-014).

## `cargo xtask` subcommands
`cargo xtask` is an alias (`.cargo/config.toml`) for `cargo run --package xtask --`. It works the same on Windows and Linux.

| Command | Purpose |
|---|---|
| `cargo xtask check-deps [--manifest-path <path>]` | Verifies the dependency rules REQ-A-1..3 (direct edges of all kinds against the table in `xtask/src/deps/rules.rs`; unknown workspace crates are violations) and that all crate roots except `cgf-plugin-api`, `cgf-plugin-host`, `cgf-sdk` have `#![forbid(unsafe_code)]` (NFR-2, ASM-024). Prints one line per violation, sorted. Exit 0 clean, 1 violations, 2 load or usage error. |

Example violation line: `REQ-A-1: cgf-core -> cgf-kernel (normal) not allowed; cgf-core may depend on: (none)`.
Negative tests: `cargo test -p xtask` (fixtures in `xtask/tests/fixtures/`, see the README there).

More `cargo xtask` subcommands will be documented here as they are added.

## Continuous integration (`.github/workflows/ci.yml`, NFR-3)
Triggers: `push`, `pull_request` (and a weekly schedule, see Supply chain). `permissions: contents: read`; a new run cancels a superseded run of the same ref. The toolchain comes from `rust-toolchain.toml` (`rustup show active-toolchain` installs it); `Swatinem/rust-cache` caches builds keyed on the lockfile; all cargo calls use `--locked`.

| Job | OS | Command |
|---|---|---|
| `fmt` | ubuntu-latest | `cargo fmt --all -- --check` |
| `clippy` | ubuntu-latest, windows-latest | `cargo clippy --workspace --all-targets --locked -- -D warnings` |
| `test` | ubuntu-latest, windows-latest | `cargo test --workspace --locked` |
| `doc` | ubuntu-latest, windows-latest | `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked` |
| `check-deps` | ubuntu-latest, windows-latest | `cargo xtask check-deps` |

The determinism gate is reported as skipped in M0 (ASM-019); M1 adds it to the `test` job. Third-party actions are pinned by commit SHA with the tag in a comment. Branch protection (required checks) is a manual admin step on the remote.
