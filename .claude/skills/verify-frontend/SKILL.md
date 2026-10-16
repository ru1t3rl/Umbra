---
name: verify-frontend
description: Run the full GUI verification and build pipeline with Bun: install, typecheck, lint, tests, architecture check, API drift check, production build, optional Tauri bundle and e2e. Use before declaring any frontend task done and when frontend CI fails.
---

# Verify the frontend

Run from `gui/`. Stop at the first failure and fix the cause; never weaken a gate.

```bash
bun install --frozen-lockfile
bun run gen:api --check          # generated client matches ../api/openapi.json
bun run typecheck                # tsc -b across all workspaces, strict
bun run lint                     # biome check (lint + format)
bun run check:deps               # dependency-cruiser: layering, tiers, no deep imports, no cycles
bun test                         # unit + component + shell tests
bun run build                    # vite production build
# slower gates (CI, or before closing M8)
bun run test:e2e                 # playwright against headless engine + example scenario
bun run tauri build              # Windows bundle (on a Windows runner)
```

## Report
Pass/fail per step, first failure with message and probable cause, and the owner: layering or structure -> `frontend-architect`; implementation -> `frontend-engineer`; contract mismatch -> `api-engineer`; flaky test -> `frontend-test-engineer`; pipeline/tooling -> `build-engineer`.

Confirm `bun.lock` is the only lockfile (no `package-lock.json`, `yarn.lock`, `pnpm-lock.yaml`).
