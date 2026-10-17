# check-deps fixtures

Provenance: hand-written by the test-engineer for M0 (docs/plans/M0.md, AT-5, AT-6).
Each directory is a self-contained mini-workspace (own `[workspace]` table) so the parent
workspace does not absorb it. All dependencies are path dependencies, so tests work offline.
`fake_external` stands for any non-member ("external") dependency (ASM-006) and is excluded
from the fixture workspace. Package names reuse the real crate names so the real rule table applies.

## violating (expected violations, see xtask/tests/check_deps.rs)
| Edge | Kind | Rule |
|---|---|---|
| cgf-core -> cgf-kernel | normal | REQ-A-1 |
| cgf-kernel -> cgf-server | normal | REQ-A-2 |
| cgf-kernel -> cgf-net-dis | dev | REQ-A-2 |
| cgf-kernel -> cgf-api-types | build | REQ-A-2 |
| cgf-sdk -> cgf-core | normal, `cfg(windows)` target table | REQ-A-3 |
| cgf-plugin-api -> fake_external | normal and build | section 4 / REQ-A-3 |
| cgf-motion -> cgf-kernel | normal, renamed `k = { package = "cgf-kernel" }` | section 4 |
| cgf-mystery | not in rule table | unknown crate |
| cgf-cli -> shim | normal, path dependency outside the workspace (`shim` depends on `cgf-kernel`, which would launder the edge) | non-member path dependency (ASM-006) |
| cgf-net-dis | no `[lints] workspace = true` | ASM-024 |

## conforming
Only allowed edges, including a dev edge, a build edge, a target-specific edge and a renamed edge
that are all allowed, plus an external dev-dependency on `cgf-plugin-api` (only normal and build
externals are forbidden there).

## unsafe-forbid and lints checks (ASM-024)
Every crate root except `cgf-plugin-api`, `cgf-plugin-host` and `cgf-sdk` carries
`#![forbid(unsafe_code)]`, with one seeded exception in `violating`: `crates/cgf-api-types` has an
empty `lib.rs` (expected `MissingForbidUnsafe`). The FFI crates present in each fixture
(`cgf-plugin-api` and `cgf-sdk` in both, `cgf-plugin-host` only in `conforming`) stay empty to prove
they are exempt. Every member has `[lints] workspace = true`, except `violating/crates/cgf-net-dis`.

## single-violation fixtures
- `unsafe_only`: one crate, `cgf-core`, whose only problem is a missing `#![forbid(unsafe_code)]`.
- `lints_only`: one crate, `cgf-core`, whose only problem is a missing `[lints] workspace = true`.
They prove the binary exits 1 for each of these checks on its own.
