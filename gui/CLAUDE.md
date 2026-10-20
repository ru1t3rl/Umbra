# GUI project rules (React + TypeScript, Bun)

Applies to everything under `gui/`. Full architecture: `docs/frontend-architecture.md`. These rules supersede spec section 13 where they differ. The root `CLAUDE.md` still applies.

## Stack (decisions F-1..F-8)
| ID | Decision | Why |
|---|---|---|
| F-1 | **Bun** as package manager, workspace manager, script runner and test runner; commit `bun.lock`, always `bun install --frozen-lockfile` in CI | Fast, one tool, native workspaces |
| F-2 | **One Bun monorepo** rooted at `gui/` (root `package.json` with `workspaces`, one `bun.lock`). Every library created, `features/*`, `feature-shells/*` and any later shared library, is its own workspace package (`@umbra/<name>`, `@umbra/shell-<name>`) with `"private": true`: never published, consumed only inside the monorepo through `workspace:*` dependencies. The app in `src/` is the root package, not a library. The scaffold is created in M5b's first work item (human decision 2026-10-20) | Real library boundaries, enforced public APIs, nothing leaks to a registry |
| F-3 | **TypeScript strict** (`strict`, `noUncheckedIndexedAccess`, `exactOptionalPropertyTypes`) | Catch errors early; generated types stay trustworthy |
| F-4 | **React + Vite + TypeScript** for the app and libraries (confirmed by the human 2026-10-20 over Angular), **Tauri 2** for the Windows app (`src-tauri/`) | Same web build for web and desktop; live-map speed comes from the imperative map layer (see Performance), not the UI framework; React keeps the xyflow/React Flow editor (M8b) and the map bindings |
| F-5 | **TanStack Query** for all REST data, through **orval**-generated `useQuery`/`useMutation` hooks exposed only by the foundation package **`@umbra/data`** (human decision 2026-10-20); **Zustand** for client and live stream state (`@umbra/engine-stream`) | One generated, typed data layer; features never call `fetch` or a generated function directly; server cache separate from live state |
| F-6 | **Biome** for lint + format, **dependency-cruiser** for architecture rules | One fast tool plus an enforcer for layer rules |
| F-7 | **`bun test`** (+ happy-dom, Testing Library) for unit/component tests, **Playwright** for e2e | KISS; fewer tools |
| F-8 | Map library hidden behind a `MapView` interface in a feature (MapLibre vs Cesium decided at M5b, A-002) | Swappable, keeps decision cheap |

## Structure
```
gui/
  features/            # libraries: reusable, domain-focused building blocks
    <feature>/
  feature-shells/      # libraries: page-level compositions ("pages") of features
    <shell>/
  src/                 # the app: entry, providers, router composition, config only
  src-tauri/           # Tauri shell (Rust)
  e2e/                 # Playwright tests
  package.json         # workspaces: ["features/*", "feature-shells/*"]
  bun.lock  bunfig.toml  tsconfig.base.json  tsconfig.json  vite.config.ts
  biome.json  .dependency-cruiser.cjs  index.html
```

## Layering rules (CI-enforced by `bun run check:deps`)
1. `src` may import shells and features. Nothing imports `src`.
2. **Shells** import features only (and other shells never). Shells hold layout, routing definition and wiring; **no business logic, no fetch calls, no direct generated-client use**.
3. **Features** never import shells or `src`.
4. Features come in two tiers (set `"umbra": {"tier": "foundation" | "domain"}` in each `package.json`):
   - `foundation` (e.g. `ui-kit`, `data`, `engine-stream`): may import only other foundation features.
   - `domain` (e.g. `entity-inspector`, `scenario-control`): may import foundation features, **never other domain features**. Cross-feature composition happens in shells via props/callbacks.
5. Import another package only through its package name (`@umbra/x`) which resolves to `src/index.ts`. Deep imports (`@umbra/x/src/...`) are forbidden.
6. No cyclic dependencies. No `any`, no `@ts-ignore` without a linked reason.
7. Generated code is never hand-edited and is produced by `bun run gen:api`: the REST client and its TanStack Query hooks by **orval** from `../api/openapi.json` into `features/data/src/generated/` (config `features/data/orval.config.ts`: `client: 'react-query'`, `httpClient: 'fetch'`, a custom mutator in `features/data/src/fetcher.ts` that applies the runtime base URL and turns `application/problem+json` into a typed `ApiProblem`), and the stream types from `../api/stream.schema.json` into `features/engine-stream/src/generated/` (`json-schema-to-typescript`; orval does not cover WebSocket/JSON Schema). Other features import REST data only from `@umbra/data`'s `src/index.ts` (re-exported hooks, query keys, `ApiProblem`, the `DataProvider`), never from `generated/`.

## Code standards
- Components are small and presentational where possible; logic in hooks and pure functions in `model/`.
- Props typed explicitly; no default exports except route components that shells lazy-load.
- The live entity layer must not re-render React per entity: keep entity state in a store, render the map layer imperatively, batch updates per animation frame; virtualize long lists.
- All user-visible text through one place (`ui-kit` i18n helper) even if only English at first.
- Accessibility: semantic HTML, keyboard navigation, labelled controls; check with Testing Library role queries.
- The GUI never contains engine logic. It only calls the generated client and decodes the stream.

## Scripts (root package.json, run with bun)
`dev`, `build`, `preview`, `typecheck` (tsc -b), `lint` (biome check), `format`, `test` (bun test), `test:e2e` (playwright), `check:deps`, `gen:api`, `tauri`.
