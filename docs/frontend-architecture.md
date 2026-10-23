# Frontend architecture

Companion to spec sections 12-13 and `gui/CLAUDE.md`. Defines how the GUI is organized so agents can extend it consistently.

## 1. Concepts
- **Feature**: a library (`features/<name>`, package `@umbra/<name>`) that does one domain thing well and exposes a small public API via `src/index.ts`. Examples: `ui-kit`, `data`, `engine-stream`, `map-view`, `entity-inspector`, `scenario-control`, `template-browser`, `plugin-status`, `log-view`.
- **Feature shell**: a library (`feature-shells/<name>`, package `@umbra/shell-<name>`) representing a **page**. It composes features into a layout and exports a route definition. Examples: `dashboard` (live map + run controls + inspector), `templates`, `plugins`, `settings`.
- **App** (`src/`): bootstraps providers (query client, engine connection, theme), builds the router from shell route definitions, reads runtime config (engine URL). Almost no code.

## 2. Initial inventory (plan, adjust via ADR)
Milestone column (A-002, D-36): M5b (live map) is the first GUI slice and delivers the workspace scaffold, the `src/` app (providers, router, runtime `/config.json`) and the packages marked M5b; M8 extends that app instead of replacing it. Status after M5b (WI-17, 2026-10-21): the packages marked M5b below exist and are covered by `bun run verify`; the rows marked M8 or M8b do not exist yet. M8 plan (accepted by the human 2026-10-22): `docs/plans/M8.md`; the rows marked "planned" follow its ADRs 0070-0078 and are finalised at M8 close-out.

| Package | Type | Tier | Purpose | Milestone |
|---|---|---|---|---|
| `@umbra/ui-kit` | feature | foundation | Design tokens, primitives (Button, Panel, Table, Toast), i18n helper | M5b, complete (WI-1, WI-3..WI-6): tokens, `ThemeProvider`, all 14 reference components plus `Icon`, `ChipGroup`, `EntityList`, `ThemeToggle`, `defineMessages`; second entry `./styles.css`. Table and Toast are not part of the Vantage set and are not built (M8 if needed) |
| `@umbra/runtime-config` | feature | foundation | `RuntimeConfig`, `loadRuntimeConfig`, `parseRuntimeConfig`, `RuntimeConfigProvider`, `useRuntimeConfig` | M5b (WI-7) |
| `@umbra/data` | feature | foundation | The REST data layer: orval-generated client and TanStack Query `useQuery`/`useMutation` hooks for every operation in `api/openapi.json`, query keys, the fetch mutator (runtime base URL, `ApiProblem` from problem+json), `DataProvider` (QueryClient); the only package features use for REST data (human decision 2026-10-20) | M5b (WI-9) |
| `@umbra/engine-stream` | feature | foundation | WebSocket connection, encoding negotiation (json/msgpack), subscription filters, entity store | M5b (WI-9, WI-10): `connectEngineStream`, JSON decoder behind a `Decoder` interface (the negotiation hook), the Zustand store of full snapshots, `EngineStreamProvider`, narrow selector hooks, `EntitySource` for the map |
| `@umbra/settings` | feature | foundation | Versioned settings model, per-platform backends (localStorage, desktop file), `useAffiliationOf` (the ASM-305 table moves here), setting editors (editors arrive with the settings shell) | M8 (WI-4): `SettingsProvider`, `useSettings`, `useUpdateSettings`, `useAffiliationOf`, `defaultAffiliationOf`, `parseSettings`, `DEFAULT_SETTINGS`, `createLocalStorageBackend`, `useThemeStorage` (ADR 0074 Accepted) |
| `@umbra/schema-form` | feature | foundation | Read-only parameter forms from JSON Schema (subset plus JSON fallback); editable in M8b | M8 (WI-6): `SchemaForm`, `SchemaFormProps` (ADR 0070 Accepted) |
| `@umbra/desktop` | feature | foundation | Tauri bridge (engine process status, restart, output, desktop settings backend, engine connection settings); the only package that may import `@tauri-apps/*`; no-op in the web build | M8 (ADRs 0075, 0076 Accepted): WI-11 stub `useEngineOutputSource()` (returns undefined); the bridge itself is WI-22 |
| `@umbra/map-view` | feature | domain | `MapView` interface + implementation (library chosen at M5b by ADR), entity layer; public API `LiveMap`, `LiveMapProps`, `./styles.css`; MapLibre is confined to this package; the bundled Natural Earth basemap lives in its `assets/` | M5b (WI-11..WI-13) |
| `@umbra/entity-inspector` | feature | domain | Entity list/detail panel | M8 |
| `@umbra/scenario-control` | feature | domain | M5b read-only slice (WI-14, ASM-316): `RunStatusBar` (sim-time, tick and entity-count Readouts, run-state Badge as CommandBar children), `ConnectionAlert`; M8 adds load scenario, start/pause/step/stop, clock mode | M5b (read-only), M8 |
| `@umbra/template-browser` | feature | domain | Browse templates, render param forms from JSON Schema | M8 |
| `@umbra/plugin-status` | feature | domain | Plugins, capabilities, adapters, faults | M8 (WI-13): `PluginList`, `AdapterList`, `FaultSummary`, `ComponentList` (no props; each reads `@umbra/data` and the run state; adapters poll every 2 s while running) |
| `@umbra/log-view` | feature | domain | Engine log stream | M8 |
| `@umbra/behavior-editor` | feature | domain | Node-and-arrow graph canvas (xyflow/React Flow behind an interface), property forms from catalog JSON Schemas, validation display, subgraph library panel, convert-selection-to-subgraph, `[editor]` layout persistence (A-001, M8b) | M8b |
| `@umbra/shell-showcase` | shell | | Design-system reference page at `/showcase` (tokens and all components, screenshot target) | M5b (WI-8) |
| `@umbra/shell-dashboard` | shell | | Map + controls + inspector. M5b (WI-15, ASM-507): `route` (`/`, "Live map", lazy `DashboardPage`): full-viewport `LiveMap`, floating CommandBar with `RunStatusBar` and `ThemeToggle`, `ConnectionAlert`; composes ui-kit, engine-stream, runtime-config, map-view, scenario-control | M5b (map only); M8 adds controls and inspector |
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
  src/components/<Name>/<Name>.tsx          # one component per file (+ <Name>.css, <Name>.test.tsx beside it)
  src/hooks/use<Name>.ts(x)                 # one hook per file (+ use<Name>.test.ts(x) beside it)
  src/context/<Name>Context.ts              # React context objects shared by a provider and its hook
  src/model/                                # pure logic/types
```
Components, hooks and contexts follow the file layout of ADR 0046, enforced by `bun run check:layout` (see `gui/CLAUDE.md`, "File layout").
Shell:
```
feature-shells/<name>/
  package.json        # name @umbra/shell-<name>
  src/index.ts        # exports `route` (path, label, icon, nav, lazy page)
  src/components/<Name>Page/<Name>Page.tsx  # layout + wiring only (+ <Name>Page.test.tsx; default export, lazy-loaded)
  src/components/<Part>/<Part>.tsx          # page parts, one component per file (ADR 0046)
```

### Entry points a package may export besides `.`
CSS rule (ADR 0043, amended 2026-10-22, human decision; supersedes ASM-302):
- **Components.** A component imports its own co-located CSS (`./<Name>.css`, ADR 0046) and package-internal shared CSS (`../shared/*.css`) by relative path; it never imports another package's CSS. CSS stays token-only. Shell pages load their CSS with the lazy page chunk.
- **Global styles.** A package with styles that must load globally may declare one extra export `./styles.css`, and the app imports it once in `src/main.tsx`. Today there are two:
  - `@umbra/ui-kit`: fonts, tokens, base; its component `@import`s duplicate the components' own imports and are harmless;
  - `@umbra/map-view`: the MapLibre CSS, container and error rules.
- **Optional.** Other packages need no `./styles.css`.

This is the only second entry point; it is not a deep import, because it is declared in `exports`. Anything else is imported from `@umbra/<name>` alone.

## 5. Data flow
- Commands and config: the orval-generated TanStack Query hooks of `@umbra/data` (`useQuery` for reads, `useMutation` for commands, invalidation by generated query keys).
- Live state (ADR 0045, D-61): `engine-stream` owns one WebSocket (`/api/v1/stream`, JSON), sends `subscribe` after `hello`, decodes into GUI domain types (unknown values become `"unknown"`), keeps the newest frame in a pending slot and moves it into a Zustand store at most once per animation frame. The store holds full snapshots: entities are replaced, not merged, and a new `session` drops the old set. React reads only narrow selectors (`useStreamStatus`, `useRunStatus`; scenario-control derives `useEntityCount`); `@umbra/map-view` reads the entity set through `EntitySource.subscribe` outside React and gives MapLibre at most one `setData` per animation frame, so no React render happens per entity. Reconnect uses a capped backoff (0.5 to 8 s).
- Provider order in the app: `RuntimeConfigProvider > ThemeProvider > DataProvider > EngineStreamProvider > RouterProvider`; `main.tsx` loads `/config.json` (`@umbra/runtime-config`), calls `configureEngineUrl` once and creates the stream connection. A configuration error renders a separate error app without the data layer.
- Shells compose: `@umbra/shell-dashboard` (route `/`) puts `LiveMap`, a floating `CommandBar` with `RunStatusBar` (scenario-control) and `ThemeToggle`, and `ConnectionAlert` on one full-viewport page; `@umbra/shell-showcase` (route `/showcase`) renders the design system.
- Contract source of truth: `api/openapi.json` + stream schema from the Rust server. Regenerate with `bun run gen:api`; never hand-write API types.

## 6. File layout and checks (ADR 0046, D-62)
One component or hook per file under `components/<Name>/` or `hooks/`, shared contexts in `context/`, enforced by `bun run check:layout`. The other architecture rules (layering, tiers, public API only, no cycles, no engine source, MapLibre only in `map-view`, no plain request functions in features) are dependency-cruiser rules with one failing fixture each (`gui/test/architecture.test.ts`). The ordered list of checks is `bun run verify` (gate 15 of `scripts/gates.sh --gui`, ADR 0044, D-60).

## 7. Runtime targets
- Web: `bun run build` produces static assets served anywhere; engine URL from runtime config (`/config.json`), not baked in.
- Windows: Tauri bundles the same build; may spawn `umbra` as sidecar or connect to a remote engine URL.

## 8. Testing pyramid
Pure `model/` logic (bun test) > component tests (Testing Library + happy-dom) > shell tests with mocked engine client > Playwright e2e against a real headless engine running the example scenario.
