---
name: verify-frontend
description: Run the full GUI verification and build pipeline with Bun: install, typecheck, lint, tests, architecture check, API drift check, production build, design-system and licence checks, e2e. Use before declaring any frontend task done and when gate 15 fails.
---

# Verify the frontend

Run from `gui/` (this is gate 15 of `docs/build.md`; `scripts/gates.sh --gui` runs it). Stop at the first failure and fix the cause; never weaken a gate.

```bash
bun install --frozen-lockfile
bun run verify                   # the ordered list below, stops at the first failure
```

`bun run verify` runs, in order (ADR 0044). A step whose input does not exist yet is not faked; `verify` grows work item by work item, and the WI that adds a step updates this list in the same commit:
1. `gen:api --check`: generated client matches `../api/openapi.json` (added in M5b WI-9)
2. `gen:tokens --check`: tokens.css/tokens.ts match tokens.json (A-003)
3. `check:tokens`: no hard-coded design values outside ui-kit tokens (A-003)
4. `check:contrast`: WCAG claims of the tokens in both themes (A-003)
5. `check:licenses`: npm licences within the allow list (ADR 0043)
6. `typecheck`: `tsc -b` across all workspaces, strict
7. `lint`: biome check (lint + format)
8. `check:layout`: one component per file in `components/<Name>/<Name>.tsx`, one hook per file in `hooks/use<Name>.ts(x)`, contexts in `context/<Name>Context.ts` (ADR 0046, added in M5b WI-12b)
9. `check:deps`: dependency-cruiser layering, tiers, no deep imports, no cycles (added in M5b WI-2; also runs the ADR 0045 plain-request scan)
10. `test`: bun test, unit + component + shell tests
11. `build`: vite production build
12. `test:e2e`: playwright, two projects (`ui` in M5b WI-7, `engine` in WI-16). Prerequisites: Chromium (`bunx playwright install chromium`); for the `engine` project only, the release binary `target/release/umbra` or `UMBRA_BIN` (`cargo build -p umbra-cli --release --locked`; gate 9 of `scripts/gates.sh` builds it before gate 15; e2e never builds it, ASM-311). Global setup builds the e2e bundle and serves it engine-free for `ui`; the worker-scoped `engineServer` fixture (`e2e/support/fixtures.ts`) starts `umbra serve` and a proxied preview for `e2e/engine/**` (ASM-514)

Currently active (M5b WI-12b): steps 2 to 12 (all but 1). Then, on a Windows runner only (M8): `bun run tauri build`.

## Report
Pass/fail per step, first failure with message and probable cause, and the owner: layering or structure -> `frontend-architect`; implementation -> `frontend-engineer`; contract mismatch -> `api-engineer`; flaky test -> `frontend-test-engineer`; pipeline/tooling -> `build-engineer`.

Confirm `bun.lock` is the only lockfile (no `package-lock.json`, `yarn.lock`, `pnpm-lock.yaml`).
