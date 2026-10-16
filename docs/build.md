# Build and verify

Toolchain is pinned in `rust-toolchain.toml` (Rust 1.99.1, edition 2024, resolver 3); rustup installs it automatically.

## Commands (Linux and Windows)
| Purpose | Command |
|---|---|
| Format check | `cargo fmt --all -- --check` |
| Lint | `cargo clippy --workspace --all-targets --locked -- -D warnings` |
| Tests | `cargo test --workspace --locked` |

`--locked` requires `Cargo.lock` to be committed and current. Text files use LF (`.gitattributes`, ASM-014).

Scripts and `cargo xtask` subcommands will be documented here as they are added (WI-3 onward).
