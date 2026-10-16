# ADR 0001: Project license (MIT OR Apache-2.0 for all crates)

- **Status**: Accepted (decided by the human project owner on 2026-10-17)
- **Requirement IDs**: NFR-6, NFR-4, spec section 19 open question 1, REQ-SDK-2, REQ-SDK-3
- **Affected**: every crate, `deny.toml`, root `LICENSE-*` files, generated `cgf_plugin_api.h` (M4), `gui/` package metadata (M8)

## Context
NFR-6 requires the project license to be decided before M1. Spec section 19 Q1 also asked whether the
plugin API and SDK (`cgf-plugin-api`, `cgf-sdk`) should get a more permissive license than the engine,
because plugins are linked against them and run in-process (REQ-P-11).

## Options
1. **MIT OR Apache-2.0 for everything.** The Rust ecosystem default. Permissive, so plugin authors have no
   obligations beyond attribution. Apache-2.0 adds a patent grant. One license set means no per-crate overrides.
   Con: there is no copyleft protection for the engine.
2. **Copyleft engine (GPL/LGPL/MPL-2.0) with a permissive SDK/API.** This protects engine modifications,
   but it needs per-crate license fields and a stricter dependency allow-list, and it raises "derivative
   work" questions for in-process plugins.
3. **Proprietary/closed engine with a permissive SDK.** This was not chosen by the owner.

## Decision
Option 1. The workspace uses `license = "MIT OR Apache-2.0"`. `cgf-plugin-api` and `cgf-sdk` use the same
expression (there is no separate SDK license). The repository root has `LICENSE-MIT` and `LICENSE-APACHE`.

## Consequences
- `[workspace.package] license = "MIT OR Apache-2.0"` is inherited by every crate. No per-crate overrides.
- The `deny.toml` license allow-list holds only licenses compatible with redistributing a MIT/Apache-2.0
  binary: permissive licenses (MIT, Apache-2.0, Apache-2.0 WITH LLVM-exception, BSD-2/3-Clause, ISC,
  Unicode-3.0, Zlib), added only when a real dependency needs one. Strong copyleft (GPL, AGPL) is denied.
  Weak copyleft (MPL-2.0, LGPL) needs an ADR for each case.
- This affects later choices: the DIS codec crate reuse in M6 (REQ-N-6, Q5) must have a compatible license.
  The HLA RTI in M9 (REQ-N-7) stays an optional, separately built plugin, so its license does not touch the
  core build.
- The generated C header (M4) and the example plugins carry an `SPDX-License-Identifier: MIT OR Apache-2.0` header.
- Done 2026-10-17: row D-18 added to the spec decision log and section 19 Q1 marked resolved (approved by the human).
