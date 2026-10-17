# ADR 0017: Semantic versions and ranges with the `semver` crate

- **Status**: Accepted (decided by the human project owner on 2026-10-17; one online `cargo fetch` approved; M3 plan `docs/plans/M3.md`). Spec decision D-34.
- **Requirement IDs**: REQ-B-9, REQ-B-12, REQ-B-14, REQ-B-17, REQ-P-3, NFR-4, REQ-A-1
- **Affected**: `cgf-core` (`graph`, `catalog`), `cgf-templates`, `deny.toml`, `Cargo.lock`

## Context
Graphs carry a semantic version (REQ-B-14); `subgraph` nodes carry an optional range such as `^1.0`
(REQ-B-9) whose mismatch must be rejected (REQ-B-12); catalog entries carry versions (REQ-B-17); plugin
descriptors carry a semantic version (REQ-P-3, M4). Version types must be in `cgf-core`.

## Options
1. **`semver` 1.x (dtolnay; MIT OR Apache-2.0; no dependencies; Cargo's own range semantics).**
   Pros: correct, well tested, the semantics Rust users already know. Cons: one more dependency in
   `cgf-core`; not yet in `Cargo.lock` (one `cargo fetch` online).
2. **Hand-rolled subset** (`MAJOR.MINOR.PATCH`, `^`, `~`, `=`, comparisons, comma lists). Pros: no
   dependency. Cons: about 300 lines plus property tests for a solved problem; pre-release rules are easy
   to get subtly wrong.

## Decision
Option 1, `default-features = false, features = ["std"]` (no serde). `cgf-core` re-exports
`semver::{Version, VersionReq}`. Matching uses `VersionReq::matches` (pre-releases match only ranges that
name a pre-release of the same version, as in Cargo). A `subgraph` node without `version` accepts any
version. Invalid strings are located `invalid_version` diagnostics.

## Consequences
- Easier: M4 plugin version checks reuse the same types.
- Harder: our public API exposes `semver` types (1.x has been stable for years).
- Run `cargo deny check` and `cargo audit` in the WI that adds it (no new licence expected).
- Revisit if `cgf-core` must become dependency-free.
