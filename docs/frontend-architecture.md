# Frontend architecture

Companion to spec sections 12-13 and `gui/CLAUDE.md`. Defines how the GUI is organized so agents can extend it consistently.

## 1. Concepts
- **Feature**: a library (`features/<name>`, package `@umbra/<name>`) that does one domain thing well and exposes a small public API via `src/index.ts`. Examples: `ui-kit`, `data`, `engine-stream`, `map-view`, `entity-inspector`, `scenario-control`, `template-browser`, `plugin-status`, `log-view`.
- **Feature shell**: a library (`feature-shells/<name>`, package `@umbra/shell-<name>`) representing a **page**. It composes features into a layout and exports a route definition. Examples: `dashboard` (live map + run controls + inspector), `templates`, `plugins`, `settings`.
- **App** (`src/`): bootstraps providers (query client, engine connection, theme), builds the router from shell route definitions, reads runtime config (engine URL). Almost no code.

## 2. Initial inventory (plan, adjust via ADR)
Milestone column (A-002, D-36): M5b (live map) is the first GUI slice and delivers the workspace scaffold, the `src/` app (providers, router, runtime `/config.json`) and the packages marked M5b; M8 extends that app instead of replacing it.

| Package | Type | Tier | Purpose | Milestone |
|---|---|---|---|---|
| `@umbra/ui-kit` | feature | foundation | Design tokens, primitives (Button, Panel, Table, Toast), i18n helper | M5b (minimum: i18n helper, Panel); rest M8 |
| `@umbra/runtime-config` | feature | foundation | `RuntimeConfig`, `loadRuntimeConfig`, `parseRuntimeConfig`, `RuntimeConfigProvider`, `useRuntimeConfig` | M5b (WI-7) |
| `@umbra/data` | feature | foundation | The REST data layer: orval-generated client and TanStack Query `useQuery`/`useMutation` hooks for every operation in `api/openapi.json`, query keys, the fetch mutator (runtime base URL, `ApiProblem` from problem+json), `DataProvider` (QueryClient); the only package features use for REST data (human decision 2026-10-20) | M5b |
| `@umbra/engine-stream` | feature | foundation | WebSocket connection, encoding negotiation (json/msgpack), subscription filters, entity store | M5b (JSON encoding with the negotiation hook) |
| `@umbra/map-view` | feature | domain | `MapView` interface + implementation (library chosen at M5b by ADR), entity layer; public API `LiveMap`, `LiveMapProps`, `./styles.css` | M5b |
| `@umbra/entity-inspector` | feature | domain | Entity list/detail panel | M8 |
| `@umbra/scenario-control` | feature | domain | Load scenario, start/pause/step/stop, clock mode | M8 |
| `@umbra/template-browser` | feature | domain | Browse templates, render param forms from JSON Schema | M8 |
| `@umbra/plugin-status` | feature | domain | Plugins, capabilities, adapters, faults | M8 |
| `@umbra/log-view` | feature | domain | Engine log stream | M8 |
| `@umbra/behavior-editor` | feature | domain | Node-and-arrow graph canvas (xyflow/React Flow behind an interface), property forms from catalog JSON Schemas, validation display, subgraph library panel, convert-selection-to-subgraph, `[editor]` layout persistence (A-001, M8b) | M8b |
| `@umbra/shell-showcase` | shell | | Design-system reference page at `/showcase` (tokens and all components, screenshot target) | M5b (WI-8) |
| `@umbra/shell-dashboard` | shell | | Map + controls + inspector | M5b (map only); M8 adds controls and inspector |
| `@umbra/shell-templates` | shell | | Template browser page | M8 |
| `@umbra/shell-plugins` | shell | | Plugin/adapter status page | M8 |
| `@umbra/shell-settings` | shell | | Engine connection and app settings | M8 |
| `@umbra/shell-behaviors` | shell | | Behavior list, editor, validation and (later) live preview page (A-001, M8b) | M8b |

## 3. Dependency graph
```
src  ->  feature-shells  ->  domain features  ->  foundation features
 \_____________________________________________/
```
Domain features never depend on each other; shells connect them (props, callbacks, selectors from the shared stream store in `engine-stream`).

## 4. Package layout
Feature:
```
features/<name>/
  package.json        # name @umbra/<name>, "exports": "./src/index.ts", umbra.tier
  tsconfig.json       # extends ../../tsconfig.base.json
  README.md           # purpose, public API, example
  src/index.ts        # the ONLY public surface
  src/components/  src/hooks/  src/model/   # model = pure logic/types
  src/*.test.ts(x)    # colocated tests
```
Shell:
```
feature-shells/<name>/
  package.json        # name @umbra/shell-<name>
  src/index.ts        # exports `route` (path, lazy page, nav metadata)
  src/<Name>Page.tsx  # layout + wiring only
  src/*.test.tsx
```

## 5. Data flow
- Commands and config: the orval-generated TanStack Query hooks of `@umbra/data` (`useQuery` for reads, `useMutation` for commands, invalidation by generated query keys).
- Live state: `engine-stream` owns the WebSocket, decodes frames, applies deltas to a Zustand store; features read through narrow selectors.
- Contract source of truth: `api/openapi.json` + stream schema from the Rust server. Regenerate with `bun run gen:api`; never hand-write API types.

## 6. Runtime targets
- Web: `bun run build` produces static assets served anywhere; engine URL from runtime config (`/config.json`), not baked in.
- Windows: Tauri bundles the same build; may spawn `umbra` as sidecar or connect to a remote engine URL.

## 7. Testing pyramid
Pure `model/` logic (bun test) > component tests (Testing Library + happy-dom) > shell tests with mocked engine client > Playwright e2e against a real headless engine running the example scenario.
