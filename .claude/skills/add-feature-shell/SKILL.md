---
name: add-feature-shell
description: Create a new feature shell (page) library under gui/feature-shells that composes features and exports a route definition. Use whenever a new page or top-level view is added to the GUI.
---

# Add a feature shell (page)

Run from `gui/`. Owner: `frontend-engineer`; plan approved by `frontend-architect`.

1. **Define the page**: path, purpose, which features it composes, navigation label. If the page needs logic, that logic belongs in a feature (add or extend one first with `add-feature`).
2. **Scaffold** `feature-shells/<name>/`:
   ```
   package.json  tsconfig.json
   src/index.ts  src/components/<Name>Page/<Name>Page.tsx  src/components/<Name>Page/<Name>Page.test.tsx
   ```
   `package.json`:
   ```json
   {
     "name": "@umbra/shell-<name>",
     "version": "0.0.0",
     "private": true,
     "type": "module",
     "exports": "./src/index.ts",
     "umbra": { "type": "shell" },
     "dependencies": { "@umbra/ui-kit": "workspace:*" }
   }
   ```
   Add each composed feature as `workspace:*` plus a tsconfig reference.
3. **Route export** in `src/index.ts` (code-split; `lazy` returns `{ Component }` so the shell does not import react-router, which stays confined to `src/`, ASM-344):
   ```ts
   export const route = {
     path: "/<path>",
     label: "<Nav label>",
     lazy: async () => ({ Component: (await import("./components/<Name>Page/<Name>Page.tsx")).default }),
   } as const;
   ```
   The page component is the default export of `components/<Name>Page/<Name>Page.tsx` (the only allowed default export; the biome override glob is `feature-shells/*/src/components/*Page/*Page.tsx`). Page parts are further components, one per file under `src/components/<Part>/<Part>.tsx`, and hooks go in `src/hooks/` (ADR 0046, `bun run check:layout`). The app builds its `RouteObject` as `{ path: route.path, lazy: route.lazy }`.
4. **Compose**: layout and wiring only. Connect features through props, callbacks and selectors from the stream store. No `fetch`, no generated-client calls, no business logic, no importing other shells.
5. **Register** the route in `src/` where routes are composed; that is the only place `src` references the shell.
6. **Tests first**: render the page with a mocked engine client; assert features appear and wiring works (e.g. selecting an entity on the map updates the inspector).
7. Run `verify-frontend`, add the shell to `docs/frontend-architecture.md`, and add an e2e step in `e2e/` when the page is user-facing.
