# check-deps fixtures

Provenance: hand-written by the test-engineer for M0 (docs/plans/M0.md, AT-5, AT-6); the `launder_*`
and `unsafe_targets` fixtures were added by the build-engineer from the reviewer's probes (review of
0a25b26). Each directory is a self-contained mini-workspace (own `[workspace]` table) so the parent
workspace does not absorb it. Package names reuse the real crate names so the real rule table applies.

Every fixture has a committed `Cargo.lock`, because `check-deps` also runs
`cargo metadata --offline --locked --all-features` (resolved pass, ASM-005). Everything is offline:
non-member packages are local directories excluded from the fixture workspace. To regenerate a lockfile
after editing a fixture, run `cargo generate-lockfile --offline` inside the fixture directory.

`fake_external` stands for any external dependency (ASM-006). Members declare it as a **registry**
dependency (`fake_external = "0.1"`), so the `--no-deps` pass sees an external edge. The fixture root
has `[patch.crates-io] fake_external = { path = "fake_external" }` only so the resolved pass can
resolve it offline; it has no dependencies, so the resolved pass finds nothing wrong with it. There is
no allow-list for path dependencies in the production rule table.

## violating (expected violations, see xtask/tests/check_deps.rs)
| Edge | Kind | Rule |
|---|---|---|
| cgf-core -> cgf-kernel | normal | REQ-A-1 |
| cgf-kernel -> cgf-server | normal | REQ-A-2 |
| cgf-kernel -> cgf-net-dis | dev | REQ-A-2 |
| cgf-kernel -> cgf-api-types | build | REQ-A-2 |
| cgf-sdk -> cgf-core | normal, `cfg(windows)` target table | REQ-A-3 |
| cgf-plugin-api -> fake_external | normal and build (registry dependency) | section 4 / ASM-009 |
| cgf-motion -> cgf-kernel | normal, renamed `k = { package = "cgf-kernel" }` | section 4 |
| cgf-mystery | not in rule table | unknown crate |
| cgf-cli -> shim | normal, path dependency outside the workspace (`shim` depends on `cgf-kernel`, which is in `cgf-cli`'s allowed closure, so only the direct pass reports it) | non-member path dependency (ASM-006) |
| cgf-net-dis | no `[lints] workspace = true` | ASM-024 |

The resolved pass reports the same direct member edges; `check_workspace` removes the duplicates.

## conforming
Only allowed edges, including a dev edge, a build edge, a target-specific edge and renamed edges
(`c = { package = "cgf-core" }` as normal and build dependency; cargo rejects one crate under two names)
that are all allowed, plus an external dev-dependency on `cgf-plugin-api` (only normal and build
externals are forbidden there).

## laundering (resolved pass, ASM-005, ASM-006)
| Fixture | Bypass | Expected |
|---|---|---|
| `launder_patch` | root `[patch.crates-io]`: `cgf-sdk -> shim = "0.1"` is patched to `./shim`, which depends on `cgf-kernel`; `cgf-net-dis -> cgf-kernel = "0.1"` is patched to the member itself | `REQ-A-3: cgf-sdk -> shim -> cgf-kernel (resolved)`, `section 4: cgf-net-dis -> cgf-kernel (normal)` |
| `launder_config_patch` | the same `shim` patch in `.cargo/config.toml` of the fixture (applied because cargo runs in the fixture directory) | `REQ-A-3: cgf-sdk -> shim -> cgf-kernel (resolved)` |
| `launder_same_name` | `cgf-sdk -> cgf-plugin-api = { path = "../../impostor" }`, a non-member named `cgf-plugin-api` 9.0.0 that depends on `cgf-kernel` | `NonMemberPathDep`, `ImpostorPackage`, `cgf-sdk -> cgf-plugin-api -> cgf-kernel (resolved)` |
| `launder_fake_external` | `cgf-sdk -> fake_external = { path = ... }` that depends on `cgf-kernel` (the former allow-list name) | `NonMemberPathDep`, `cgf-sdk -> fake_external -> cgf-kernel (resolved)` |

Git-source laundering is covered by the same resolved check (it is source-agnostic) but has no
fixture, because a git dependency cannot be resolved offline without an absolute `file://` URL.

## unsafe-forbid and lints checks (ASM-024)
Every target root except those of `cgf-plugin-api`, `cgf-plugin-host` and `cgf-sdk` carries
`#![forbid(unsafe_code)]`, with one seeded exception in `violating`: `crates/cgf-api-types` has an
empty `lib.rs` (expected `MissingForbidUnsafe`). The FFI crates present in each fixture
(`cgf-plugin-api` and `cgf-sdk` in both, `cgf-plugin-host` only in `conforming`) stay empty to prove
they are exempt. Every member has `[lints] workspace = true`, except `violating/crates/cgf-net-dis`.
Non-member packages (`shim`, `impostor`, `fake_external`) are not checked.

## single-violation fixtures
- `unsafe_only`: one crate, `cgf-core`, whose only problem is a missing `#![forbid(unsafe_code)]`.
- `lints_only`: one crate, `cgf-core`, whose only problem is a missing `[lints] workspace = true`.
- `unsafe_targets`: `cgf-core` has the attribute in `src/lib.rs` but not in `build.rs`,
  `src/bin/tool.rs`, `tests/it.rs`, `examples/ex.rs`, `benches/bench.rs` (one violation each);
  `cgf-sdk/tests/it.rs` lacks it too and is exempt.
They prove the binary exits 1 for each of these checks on its own.
