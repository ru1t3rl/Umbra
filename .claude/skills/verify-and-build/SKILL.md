---
name: verify-and-build
description: Run the full Umbra verification and build pipeline locally (format, lint, tests, determinism gate, dependency rules, deny/audit, OpenAPI drift, benchmarks, plugin build, GUI build). Use before declaring any task or milestone done. There is no CI gate (ADR 0004); this is the gate.
---

# Verify and build

`docs/build.md` ("Local gate") is the canonical list and order of gates and which milestone activates each. If this file and `docs/build.md` differ, `docs/build.md` wins; update this file.

Shortcut: `scripts/gates.sh` runs the whole list in order (plus m2-compat and the goldens) with compact output and logs in `target/gates/`; `scripts/gates.sh --quick CRATE...` (fmt, clippy, tests of those crates) is for use during a work item and is not the definition of done. Run in that order and stop at the first failure; fix the cause, never weaken a gate. Prerequisites: `cargo install --locked cargo-deny@0.20.2 cargo-audit@0.22.2`.

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo test -p umbra-engine --test determinism --test behavior_determinism --test plugin_determinism --test handle_determinism --locked  # determinism gate (from M1; skipped in M0, ASM-019; behavior_determinism from M3, ASM-087; plugin_determinism from M4 WI-13, ASM-115; handle_determinism from M5 WI-10, ASM-155)
cargo xtask check-deps                                       # REQ-A-1..3 dependency rules + NFR-2 (REQ-A-4 is the GUI check, from M8)
cargo deny check                                             # licenses, bans, sources, advisories
cargo audit                                                  # vulnerabilities in Cargo.lock
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
cargo xtask gen-api --check                                  # gate 10: OpenAPI, stream schema and AsyncAPI drift (from M5, active from WI-12)
cargo xtask gen-header --check                               # umbra_plugin_api.h drift (from M4)
cargo build --workspace --release --locked
cargo xtask build-plugins --release                          # examples/plugins/* (from M4, active from WI-13)
cargo test -p umbra-plugin-host --test plugin_failures --locked  # plugin failure matrix (from M4)
cargo bench -p umbra-kernel --bench tick --no-run --locked                         # benches compile; full runs are manual
cargo build -p umbra-cli --no-default-features --locked      # gate 16: engine-only CLI, no server (from M5 WI-17, ASM-218)
cargo clippy -p umbra-cli --no-default-features --all-targets --locked -- -D warnings
# GUI (from M8; details in the verify-frontend skill)
(cd gui && bun install --frozen-lockfile && bun run gen:api --check && bun run typecheck && bun run lint && bun run check:deps && bun test && bun run build)
(cd gui && bun run tauri build)                              # Windows bundle, on Windows
```

## Report
Summarize pass/fail per step, the first failure with its message, probable cause and the owning agent to fix it (e.g. clippy in `umbra-kernel` -> `rust-engineer`). On Windows verify path handling and `.dll` plugin loading; on Linux `.so`.

Steps marked "from Mn" are skipped (and stated as skipped) before that milestone exists.
