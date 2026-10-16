---
name: add-feature
description: Create a new feature library under gui/features with correct Bun workspace metadata, tier, public API, tests and dependency rules. Use whenever a new domain or foundation feature is introduced in the GUI.
---

# Add a feature (library)

Run from `gui/`. Owner: `frontend-engineer`; plan approved by `frontend-architect`.

1. **Justify**: name, single responsibility, tier (`foundation` or `domain`), public API sketch. Check the inventory in `docs/frontend-architecture.md`; prefer extending an existing feature.
2. **Scaffold** `features/<name>/`:
   ```
   package.json  tsconfig.json  README.md
   src/index.ts  src/components/  src/hooks/  src/model/
   ```
   `package.json`:
   ```json
   {
     "name": "@cgf/<name>",
     "version": "0.0.0",
     "private": true,
     "type": "module",
     "exports": "./src/index.ts",
     "cgf": { "type": "feature", "tier": "domain" },
     "dependencies": { "@cgf/ui-kit": "workspace:*" },
     "peerDependencies": { "react": "^18 || ^19" }
   }
   ```
   `tsconfig.json`: `{ "extends": "../../tsconfig.base.json", "include": ["src"], "references": [] }`, with a reference for each workspace dependency.
3. **Dependencies**: only allowed tier targets (domain -> foundation; foundation -> foundation). Never another domain feature. Declare with `workspace:*`.
4. **Public API**: export only what shells need from `src/index.ts` (components, hooks, types). Everything else stays internal. Document it in `README.md` with a usage example.
5. **Tests first**: failing tests for `model/` logic and the main component through the public API (see `write-frontend-tests`).
6. **Implement**: pure logic in `model/`, stateful logic in hooks, small components. Text through the `ui-kit` i18n helper.
7. Run `bun install` (links the workspace), then the `verify-frontend` skill. `bun run check:deps` must pass.
8. Add the package to the inventory table in `docs/frontend-architecture.md`.
