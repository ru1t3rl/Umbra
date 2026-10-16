---
name: verify-and-build
description: Run the full CGF verification and build pipeline locally (format, lint, tests, determinism gate, dependency rules, deny/audit, OpenAPI drift, benchmarks, plugin build, GUI build). Use before declaring any task or milestone done and when CI fails.
---

# Verify and build

Run in this order and stop at the first failure; fix the cause, never weaken a gate.

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo test -p cgf-engine --test determinism --locked        # determinism gate
cargo xtask check-deps                                       # REQ-A-1..4 dependency rules
cargo deny check && cargo audit                              # licenses and advisories
cargo xtask gen-api --check                                  # OpenAPI/schema drift (from M5)
cargo xtask gen-header --check                               # cgf_plugin_api.h drift (from M4)
cargo build --workspace --release --locked
cargo xtask build-plugins --release                          # examples/plugins/* (from M4)
cargo test -p cgf-plugin-host --test plugin_failures         # plugin failure matrix (from M4)
cargo bench -p cgf-kernel --no-run                           # benches compile; full run in CI job
# GUI (from M8; details in the verify-frontend skill)
(cd gui && bun install --frozen-lockfile && bun run gen:api --check && bun run typecheck && bun run lint && bun run check:deps && bun test && bun run build)
(cd gui && bun run tauri build)                              # Windows bundle on Windows runner
```

## Report
Summarize pass/fail per step, the first failure with its message, probable cause and the owning agent to fix it (e.g. clippy in `cgf-kernel` -> `rust-engineer`). On Windows verify path handling and `.dll` plugin loading; on Linux `.so`.

Steps marked "from Mn" are skipped (and stated as skipped) before that milestone exists.
