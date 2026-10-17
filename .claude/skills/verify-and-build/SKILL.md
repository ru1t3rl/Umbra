---
name: verify-and-build
description: Run the full CGF verification and build pipeline locally (format, lint, tests, determinism gate, dependency rules, deny/audit, OpenAPI drift, benchmarks, plugin build, GUI build). Use before declaring any task or milestone done. There is no CI gate (ADR 0004); this is the gate.
---

# Verify and build

`docs/build.md` ("Local gate") is the canonical list and order of gates and which milestone activates each. If this file and `docs/build.md` differ, `docs/build.md` wins; update this file.

Run in that order and stop at the first failure; fix the cause, never weaken a gate. Prerequisites: `cargo install --locked cargo-deny@0.20.2 cargo-audit@0.22.2`.

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo test -p cgf-engine --test determinism --locked        # determinism gate (from M1; skipped in M0, ASM-019)
cargo xtask check-deps                                       # REQ-A-1..4 dependency rules
cargo deny check                                             # licenses, bans, sources, advisories
cargo audit                                                  # vulnerabilities in Cargo.lock
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
cargo xtask gen-api --check                                  # OpenAPI/schema drift (from M5)
cargo xtask gen-header --check                               # cgf_plugin_api.h drift (from M4)
cargo build --workspace --release --locked
cargo xtask build-plugins --release                          # examples/plugins/* (from M4)
cargo test -p cgf-plugin-host --test plugin_failures         # plugin failure matrix (from M4)
cargo bench -p cgf-kernel --no-run                           # benches compile; full runs are manual
# GUI (from M8; details in the verify-frontend skill)
(cd gui && bun install --frozen-lockfile && bun run gen:api --check && bun run typecheck && bun run lint && bun run check:deps && bun test && bun run build)
(cd gui && bun run tauri build)                              # Windows bundle, on Windows
```

## Report
Summarize pass/fail per step, the first failure with its message, probable cause and the owning agent to fix it (e.g. clippy in `cgf-kernel` -> `rust-engineer`). On Windows verify path handling and `.dll` plugin loading; on Linux `.so`.

Steps marked "from Mn" are skipped (and stated as skipped) before that milestone exists.
