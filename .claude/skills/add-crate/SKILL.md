---
name: add-crate
description: Create a new crate in the Umbra Cargo workspace with correct dependency direction, lints, docs and tests. Use whenever a new crate from spec section 4 is introduced.
---

# Add a workspace crate

1. Confirm the crate exists in spec section 4 (name, responsibility, allowed dependencies). If not, request an ADR first.
2. `cargo new --lib crates/<name>` (binaries only for `umbra-cli`). Register in the root `[workspace] members`.
3. `Cargo.toml`: use `[workspace.package]` and `[workspace.dependencies]` inheritance; add ONLY the dependencies allowed by the table in section 4 (REQ-A-1..4). Add `[lints] workspace = true`.
4. `lib.rs` header: crate-level docs stating the single responsibility, the REQ-IDs it serves, and `#![forbid(unsafe_code)]` (except `umbra-plugin-api`, `umbra-plugin-host`, `umbra-sdk`).
5. Define the public API as small traits and plain types; keep internals private. No `pub` by default.
6. Add a first test proving the crate builds and its core contract works (contract tests for traits live next to the trait).
7. Update the dependency-rule check configuration so the new crate is covered, and verify the check fails if a forbidden dependency is added (temporarily add one, confirm failure, remove it).
8. Run the `verify-and-build` skill. Update `docs/architecture.md` crate table.
