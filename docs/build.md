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
