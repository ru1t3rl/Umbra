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
   src/index.ts  src/components/  src/hooks/  src/context/  src/model/
   ```
   `package.json`:
   ```json
   {
     "name": "@umbra/<name>",
     "version": "0.0.0",
     "private": true,
     "type": "module",
     "exports": "./src/index.ts",
     "umbra": { "type": "feature", "tier": "domain" },
     "dependencies": { "@umbra/ui-kit": "workspace:*" },
     "peerDependencies": { "react": "^18 || ^19" }
   }
   ```
   `tsconfig.json`: `{ "extends": "../../tsconfig.base.json", "include": ["src"], "references": [] }`, with a reference for each workspace dependency.
3. **Dependencies**: only allowed tier targets (domain -> foundation; foundation -> foundation). Never another domain feature. Declare with `workspace:*`.
4. **Public API**: export only what shells need from `src/index.ts` (components, hooks, types). Everything else stays internal. Document it in `README.md` with a usage example.
   A feature with component tests also includes `"../../test/jest-dom.d.ts"` in `include` (types for the jest-dom matchers on `bun:test`, needs bun types in that package's tsconfig; never put it in `tsconfig.base.json`).
5. **Tests first**: failing tests for `model/` logic and the main component through the public API (see `write-frontend-tests`).
6. **Implement**: pure logic in `model/`, stateful logic in hooks, small components. Text through the `ui-kit` i18n helper. File layout (ADR 0046): one component per file at `src/components/<Name>/<Name>.tsx` (CSS and test beside it), one hook per file at `src/hooks/use<Name>.ts(x)` (test beside it), React context objects at `src/context/<Name>Context.ts`; `bun run check:layout` fails otherwise. CSS (ADR 0043, amended 2026-10-22): a component imports its own co-located `./<Name>.css` by relative path; it never imports another package's CSS. Add a `./styles.css` export only for styles that must load globally; the app then imports it once in `src/main.tsx`.
7. Run `bun install` (links the workspace), then the `verify-frontend` skill. `bun run check:deps` must pass.
8. Add the package to the inventory table in `docs/frontend-architecture.md`.
