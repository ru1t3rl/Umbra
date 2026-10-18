# check-deps fixtures

Provenance: hand-written by the test-engineer for M0 (docs/plans/M0.md, AT-5, AT-6); the `launder_*`
and `unsafe_targets` fixtures were added by the build-engineer from the reviewer's probes (review of
0a25b26). Each directory is a self-contained mini-workspace (own `[workspace]` table) so the parent
workspace does not absorb it. Package names reuse the real crate names so the real rule table applies.

Every fixture except `resolve_fails` has a committed `Cargo.lock`, because `check-deps` also runs
`cargo metadata --offline --locked --all-features` (resolved pass, ASM-005). Everything is offline:
non-member packages are local directories excluded from the fixture workspace. To regenerate a lockfile
after editing a fixture, run `cargo generate-lockfile --offline` inside the fixture directory.

`fake_external` stands for any external dependency (ASM-006). Members of `violating` and `conforming`
declare it as a **registry** dependency (`fake_external = "0.1"`), so the `--no-deps` pass sees an
external edge. Their `.cargo/config.toml` replaces crates.io with the directory source `vendor/`, and
`vendor/fake_external/.cargo-checksum.json` is `{"files":{},"package":null}`. So the resolved pass
sees a package with a crates.io source (not a path package) and resolves it offline. Do not use
`[patch]` for this: a patched path package is itself a violation (`NonMemberPathPackage`). There is no
allow-list for path packages in the production rule table.

## violating (expected violations, see xtask/tests/check_deps.rs)
| Edge | Kind | Rule |
|---|---|---|
| umbra-core -> umbra-kernel | normal | REQ-A-1 |
| umbra-kernel -> umbra-server | normal | REQ-A-2 |
| umbra-kernel -> umbra-net-dis | dev | REQ-A-2 |
| umbra-kernel -> umbra-api-types | build | REQ-A-2 |
| umbra-sdk -> umbra-core | normal, `cfg(windows)` target table | REQ-A-3 |
| umbra-plugin-api -> fake_external | normal and build (registry dependency) | section 4 / ASM-009 |
| umbra-motion -> umbra-kernel | normal, renamed `k = { package = "umbra-kernel" }` | section 4 |
| umbra-mystery | not in rule table | unknown crate |
| umbra-cli -> shim | normal, path dependency outside the workspace (`shim` depends on `umbra-kernel`, which is in `umbra-cli`'s allowed closure, so there is no indirect violation) | `NonMemberPathDep` and `NonMemberPathPackage` (ASM-006) |
| umbra-net-dis | no `[lints] workspace = true` | ASM-024 |

The resolved pass reports the same direct member edges; `check_workspace` removes the duplicates.

## conforming
Only allowed edges, including a dev edge, a build edge, a target-specific edge and renamed edges
(`c = { package = "umbra-core" }` as normal and build dependency; cargo rejects one crate under two names)
that are all allowed, plus an external dev-dependency on `umbra-plugin-api` (only normal and build
externals are forbidden there).

## laundering (resolved pass, ASM-005, ASM-006)
| Fixture | Bypass | Expected |
|---|---|---|
| `launder_patch` | root `[patch.crates-io]`: `umbra-sdk -> shim = "0.1"` is patched to `./shim`, which depends on `umbra-kernel`; `umbra-net-dis -> umbra-kernel = "0.1"` is patched to the member itself | `REQ-A-3: umbra-sdk -> shim -> umbra-kernel (resolved)`, `section 4: umbra-net-dis -> umbra-kernel (normal)` |
| `launder_config_patch` | the same `shim` patch in `.cargo/config.toml` of the fixture (applied because cargo runs in the fixture directory) | `REQ-A-3: umbra-sdk -> shim -> umbra-kernel (resolved)` |
| `launder_same_name` | `umbra-sdk -> umbra-plugin-api = { path = "../../impostor" }`, a non-member named `umbra-plugin-api` 9.0.0 that depends on `umbra-kernel` | `NonMemberPathDep`, `ImpostorPackage`, `umbra-sdk -> umbra-plugin-api -> umbra-kernel (resolved)` |
| `launder_fake_external` | `umbra-sdk -> fake_external = { path = ... }` that depends on `umbra-kernel` (the former allow-list name) | `NonMemberPathDep`, `umbra-sdk -> fake_external -> umbra-kernel (resolved)` |
| `launder_unsafe_patch` | root `[patch.crates-io]`: `umbra-core -> shim = "1"` is patched to `./shim`, a non-member with `unsafe` code and no `forbid` | `NonMemberPathPackage` for `shim` only |

Every `launder_*` fixture also reports its non-member path package (`shim`, `impostor`,
`fake_external`) as `NonMemberPathPackage`, and `launder_patch` reports the registry-named
`umbra-kernel = "0.1"` of `umbra-net-dis` as `ImpostorDep`.

## resolve_fails (no `Cargo.lock` on purpose)
`umbra-sdk -> umbra-kernel = { git = "https://example.invalid/..." }` cannot be resolved offline. The
binary prints the declared-pass violations (`REQ-A-1: umbra-core -> umbra-kernel`, `ImpostorDep` for the
git `umbra-kernel`), then the resolved-graph error, and exits 2.

Git-source laundering is covered by the same resolved check (it is source-agnostic) but has no
fixture, because a git dependency cannot be resolved offline without an absolute `file://` URL.

## unsafe-forbid and lints checks (ASM-024)
Every target root except those of `umbra-plugin-api`, `umbra-plugin-host` and `umbra-sdk` carries
`#![forbid(unsafe_code)]`, with one seeded exception in `violating`: `crates/umbra-api-types` has an
empty `lib.rs` (expected `MissingForbidUnsafe`). The FFI crates present in each fixture
(`umbra-plugin-api` and `umbra-sdk` in both, `umbra-plugin-host` only in `conforming`) stay empty to prove
they are exempt. Every member has `[lints] workspace = true`, except `violating/crates/umbra-net-dis`.
Non-member packages (`shim`, `impostor`, `fake_external`) are not scanned; as path packages they are
violations themselves.

## single-violation fixtures
- `unsafe_only`: one crate, `umbra-core`, whose only problem is a missing `#![forbid(unsafe_code)]`.
- `lints_only`: one crate, `umbra-core`, whose only problem is a missing `[lints] workspace = true`.
- `unsafe_targets`: `umbra-core` has the attribute in `src/lib.rs` but not in `build.rs`,
  `src/bin/tool.rs`, `tests/it.rs`, `examples/ex.rs`, `benches/bench.rs` (one violation each);
  `umbra-sdk/tests/it.rs` lacks it too and is exempt.
They prove the binary exits 1 for each of these checks on its own.

## lint facts (M4 WI-2, R-M5, ASM-116; `xtask/tests/lint_facts.rs`)
Not `check-deps` fixtures: `lint_facts.rs` builds them with `cargo build --offline --locked` and
`RUSTFLAGS="-D warnings"`, each in its own target directory under `CARGO_TARGET_TMPDIR`. Both are
two-crate workspaces (path dependency only, committed `Cargo.lock`, workspace `unsafe_code = "deny"` as
in the real repo): `entry_macro` stands in for `umbra-sdk`, `plugin` (a `cdylib` with
`#![forbid(unsafe_code)]`) for a plugin crate that invokes `entry_macro::export_entry!()`.
- `forbid_external_no_mangle`: the macro emits only `#[unsafe(no_mangle)] pub extern "C" fn` with a safe
  body (the `export_plugin!` shape). Expected: builds cleanly.
- `forbid_external_expect`: the macro also emits `#[expect(unsafe_code, reason = ..)]`. Expected: error
  E0453 (an inner `allow`/`expect` cannot override `forbid`), which is why `export_plugin!` emits no lint
  attribute.

## gen_header (M4 WI-4, AT-2; `xtask/tests/gen_header.rs`)
Not a `check-deps` fixture: a one-struct crate (own `[workspace]`, no dependencies, committed `Cargo.lock`)
with its own `cbindgen.toml` and the header the pinned `cbindgen` 0.29.4 generates for it
(`include/umbra_plugin_api.h`, written by `cargo xtask gen-header --crate-dir xtask/tests/fixtures/gen_header`).
The tests check it as is (current), and copies under `CARGO_TARGET_TMPDIR` with a field appended to
`src/lib.rs` (drift, exit 1), with a CRLF header (not drift) and with a hand-edited header (write mode
restores it). Regenerate the committed header only when the pinned `cbindgen` version changes.
