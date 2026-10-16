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
- Enforcement: `cargo xtask check-deps` (WI-3, not yet implemented).

## Code-level guards (workspace-wide)
- `unsafe_code = "deny"` in `[workspace.lints]`; non-FFI crates also `#![forbid(unsafe_code)]` (NFR-2, ASM-012).
- `clippy.toml` bans `SystemTime::now`, `Instant::now`, `HashMap` and `HashSet` (REQ-K-4, ASM-013).
