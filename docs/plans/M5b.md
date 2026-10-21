# M5b Live map, the first GUI slice: implementation plan

Status: **Accepted** by the human on 2026-10-20 (frontend-architect proposed it the same day). Every
recommendation H-1..H-10 was accepted as written: MapLibre GL JS with the offline token style plus Natural
Earth (option a), the full npm dependency set, licence allow-list and prerequisites, gate 15 from M5b, the
five GUI rule changes, the A-003 agent/skill edits, self-drawn APP-6D symbols, Linux-only screenshot
baselines reviewed by the human at WI-8 and WI-15.

- ADRs 0040-0045 are Accepted (2026-10-20).
- Decision rows D-56..D-61 are proposed (D-62 unused).
- Assumptions are ASM-300..ASM-321 in `docs/assumptions/M5b.md`.

Base: branch `m5b` (worktree `/home/ru1t3rl/Documents/cgf/umbra-m5b`) from `develop` at `ac9e01e`, tag
`m5-done`.

Parallel track: M6 (engine networking) runs at the same time on another branch. Number blocks are
reserved in `docs/assumptions.md`.

## Goal
Umbra gets its real GUI, not a throwaway viewer. M8 will extend it.

1. A private Bun monorepo in `gui/`, with the full Vantage design system (A-003) built first: tokens,
   theme, checks, all 14 reference components and a showcase in Ops and Day.
2. On top of that:
   - a typed REST layer generated from the M5 contract (`@umbra/data`, orval + TanStack Query);
   - a live stream store (`@umbra/engine-stream`);
   - a MapLibre map behind `MapView` (`@umbra/map-view`) with an imperative APP-6D entity layer;
   - a dashboard page with the map only.
3. A Playwright test drives a real headless `umbra serve` in stepped mode. It proves that the markers on
   the map are where the engine says the entities are.
4. M5b changes no engine crate, no engine behavior and no engine acceptance criterion.

**Gate check (implement-milestone step 1):** M5 is ticked in `docs/milestones.md`, with tag `m5-done` and
the full local gate on Linux. A-002 allows M5b after M5.

There is no CI (ADR 0004). The local gate list in `docs/build.md` is the definition of done. Gate 15
becomes active in this milestone (ADR 0044).

## Requirements in scope
| ID | What M5b does with it |
|---|---|
| REQ-G-6, D-1, D-13 | The GUI sits on top of the engine as a client only |
| REQ-A-4 | The GUI imports no engine source. It uses only the generated client and the stream schema. dependency-cruiser enforces this (WI-2) |
| REQ-X-4, REQ-X-5 | Consumed through orval hooks (`gen:api --check` = no drift, WI-9) |
| REQ-X-6, REQ-X-7 | Stream client: JSON with a negotiation hook, a subscription, and tolerance for drops (WI-10) |
| REQ-K-5 | Owned and remote entities are styled differently (WI-11) |
| NFR-1 | 10,000 entities: imperative layer, per-frame batching, count-based performance tests (WI-10, WI-12) |
| NFR-4 | npm licence policy (`check:licenses`, WI-1) |
| D-36, A-002 M5b | Every deliverable and acceptance criterion (mapping below) |
| A-003 | Every step and acceptance criterion, as the first work items (WI-1..WI-8) |
| F-1..F-8 | The stack as fixed in `gui/CLAUDE.md` |
| Human decision 2026-10-20 (icon) | `umbra_icon.svg` as favicon (WI-7) |

## Acceptance mapping
Each criterion in `docs/milestones.md` (M5b), with the tests that prove it.

| # | Criterion | Evidence (tests, commands) | WI |
|---|---|---|---|
| 1 | Playwright against a real headless `umbra` serving `walking_skeleton` in stepped mode: all entities appear; after `step(n)` the layer positions equal the engine's list within tolerance, and the markers have moved | `gui/e2e/live-map.spec.ts`: `all walking_skeleton entities appear on the map`, `after step(600) the entity layer equals the engine list for that tick and every marker moved`, `a reload of the scenario replaces the entities` (tolerances below) | WI-16 |
| 2 | `typecheck`, `lint`, `test`, `check:deps`, `test:e2e` pass; `gen:api` gives no diff | `bun run verify` (ADR 0044), run by `scripts/gates.sh --gui` as gate 15; `gen:api --check` is its first step | WI-1 (script), WI-17 (evidence) |
| 3 | No engine source imported (REQ-A-4) | dependency-cruiser rule `no-engine-source` (nothing resolves outside `gui/` except `node_modules`); `gui/test/architecture.test.ts` `engine source import is rejected` (fixture imports `../../crates/...` and `../api/openapi.json`) | WI-2 |
| 4 | No engine crate behavior or acceptance criterion changed | `git diff m5-done -- crates xtask examples api Cargo.toml Cargo.lock` is empty at close-out (allowed: `scripts/gates.sh` and `docs/`); the full engine gate `scripts/gates.sh` passes with goldens unchanged | WI-17 |
| 5 | Umbra icon: `index.html` links `umbra_icon.svg`; the served asset is byte-identical to the root file | `gui/e2e/app.spec.ts`: `the page links umbra_icon.svg as an svg icon`, `the served icon is byte-identical to the repository root file`; `gui/src/index-html.test.ts` (the `<link rel="icon" type="image/svg+xml" href="/umbra_icon.svg">` tag) | WI-7 |
| 6a | A-003: `gen:tokens --check`, `check:tokens`, `check:contrast` in verify, passing | `bun run verify` steps 2-4 | WI-1 |
| 6b | All 14 reference components with the reference props, and tests | `gui/features/ui-kit/src/components/<Name>/<Name>.test.tsx` for each of the 14, plus `props.test-d.ts` (type-level check that each component's props are assignable to and from the reference `index.d.ts`) | WI-4, WI-5, WI-6 |
| 6c | The showcase renders every token and component in Ops and Day (screenshots) | `gui/e2e/showcase.spec.ts`: `showcase tokens (ops|day)`, `showcase components (ops|day)` with committed baselines | WI-8 |
| 6d | A hex literal in a feature fails `check:tokens` (tested) | `gui/scripts/check-tokens.test.ts` `a hex literal in a feature fails` (temporary fixture file under a temp copy of the roots, or the `--root` argument added in WI-1) and `umbra-allow suppresses one line` | WI-1 |
| 6e | The app starts in Ops and switches to Day without a reload | `gui/e2e/app.spec.ts` `starts in ops and switches to day without a reload` (asserts `data-theme`, and that the navigation count and a `window` marker survive); `ThemeProvider.test.tsx` | WI-3, WI-8 |

**e2e tolerances (ASM-310):**
- *Data layer.* MapView's source positions (what was handed to MapLibre) equal the engine's
  `listEntities` items for the reached tick within 1e-9 degrees. In practice they are equal: the same
  JSON doubles pass through unchanged.
- *Rendered layer.* At zoom 12, `queryRenderedFeatures` positions are within 2.0 m (haversine) of the
  engine positions. MapLibre's tile quantization at z12 is about 0.75 m at 52 N.
- *Moved.* After `step(600)` (20 s of simulated time at 30 Hz), every entity's engine position and
  rendered position moved at least 50 m, and its projected pixel position moved at least 2 px.
  walking_skeleton reaches roughly 250 m in that time.

## Compatibility and engine isolation
- No file under `crates/`, `xtask/`, `examples/`, `api/`, `Cargo.toml`, `Cargo.lock` changes in M5b.
  WI-17 verifies this with a diff.
- Files outside `gui/` that M5b changes, all docs or tooling:
  - `scripts/gates.sh`: only the gate 15 block;
  - `docs/build.md`: row 15 and prerequisites;
  - `.gitignore`;
  - `docs/frontend-architecture.md`, `docs/test-matrix.md`, `docs/milestones.md`, `docs/assumptions.md`
    (index row), `docs/assumptions/M5b.md`;
  - `docs/design-system/tactical-display.md`: one rule, if ADR 0042's remote marker is accepted;
  - `docs/adr/0040..0045`, `docs/SPEC.md` (D-rows on acceptance), `.claude/agents/frontend-*.md`, the
    skills `verify-frontend` and `review-frontend-change`, `gui/CLAUDE.md`.
- **Contract gaps:** none found. M5b needs status, load, start, step and entities (REST), and hello,
  subscribe and frame (stream). All of them exist. `force` and `ownership` are in `EntitySummary`.
  - If an implementer finds a gap, they stop and report it. The gap becomes a separate, additive
    api-engineer WI through `update-openapi` with gate 10. It is never fixed by changing the contract to
    suit the generator, and it never reopens M5.
  - There is one known future candidate, not needed in M5b: `GET /api/v1/scenarios`.
- **Merge with M6.** The two tracks may touch the same files: `scripts/gates.sh`, `docs/build.md`,
  `.gitignore`, `docs/assumptions.md` (index rows), `docs/SPEC.md` (D-rows).
  - Every M5b edit to these files is local (one block or row) to keep conflicts trivial.
  - Whichever track merges second rebases.

## Design

### Packages after M5b
| Package | Type | Tier | Public API (`src/index.ts`) | Depends on | WI |
|---|---|---|---|---|---|
| `@umbra/ui-kit` | feature | foundation | the 14 components `Button`, `Chip`, `Panel`, `Card`, `TextField`, `Toggle`, `Slider`, `Tabs`, `CommandBar`, `Readout`, `Badge`, `AffiliationTag`, `EntityRow`, `Alert`; helpers `Icon`, `ChipGroup`, `EntityList`; `ThemeProvider`, `useTheme`, `ThemeToggle`; `defineMessages`; token lists and types re-exported from `tokens.ts` (`colorTokens`, `spacingTokens`, `radiusTokens`, `typeStyles`, `cssVar`, `Affiliation`, `ColorToken`, ...); prop types `<Name>Props`, `Tone`, `ThemePreference`, `ResolvedTheme`. Second entry `./styles.css` (fonts, tokens, base, all component CSS) | (none; npm: react, fonts, icons) | 1, 3-6 |
| `@umbra/runtime-config` | feature | foundation | `RuntimeConfig`, `DEFAULT_CONFIG`, `ConfigError`, `parseRuntimeConfig`, `loadRuntimeConfig(fetchImpl, baseUrl)`, `RuntimeConfigProvider`, `useRuntimeConfig` (ASM-339) | (none; npm: react peer) | 7 |
| `@umbra/data` | feature | foundation | `DataProvider`, `configureEngineUrl`, `ApiProblem`, `isApiProblem`; re-export of the orval output: `use<Op>` hooks, `get<Op>QueryKey`, model types, and plain request functions (e2e and scripts only, ADR 0045) | npm: @tanstack/react-query | 9 |
| `@umbra/engine-stream` | feature | foundation | `connectEngineStream`, `EngineStreamProvider`, `useStreamStatus`, `useRunStatus`, `useEntitySource`; types `EngineStreamConnection`, `EngineStreamOptions`, `Subscription`, `StreamStatus`, `EntityFrame`, `EntitySource`, `EntitySummary`, `RunStatus`, `WebSocketLike` | npm: zustand | 9 (skeleton), 10 |
| `@umbra/map-view` | feature | domain | `LiveMap`, `LiveMapProps`; second entry `./styles.css` (MapLibre CSS + container) | ui-kit, engine-stream (types and `EntitySource`); npm: maplibre-gl | 11, 12, 13 |
| `@umbra/scenario-control` | feature | domain | M5b read-only slice: `RunStatusBar` (sim-time Readout, tick Readout, entity-count Readout, run-state Badge), `ConnectionAlert`. M8 adds the controls | ui-kit, engine-stream | 14 |
| `@umbra/shell-showcase` | shell | | `route` (`/showcase`, "Design system", lazy `ShowcasePage`) | ui-kit | 8 |
| `@umbra/shell-dashboard` | shell | | `route` (`/`, "Live map", lazy `DashboardPage`) | ui-kit, engine-stream, map-view, scenario-control | 15 |
| app (`src/`) | app | | none (root package) | the shells, ui-kit, runtime-config, data, engine-stream (bootstrap and providers only) | 7 |

Two changes to the planned inventory (in `docs/frontend-architecture.md`, WI-17):
1. **ui-kit is complete in M5b** (A-003 human decision), not "minimum".
2. **Two packages start early.**
   - `scenario-control` begins with a read-only slice (ASM-316). Without it, the dashboard shell would
     need run-state formatting logic, which shells may not hold.
   - `shell-showcase` is new (ASM-312). `UMBRA-IMPLEMENTATION.md` allows "a dev-only shell".

Dependency rules for these packages:
- **Tier and layering:** domain packages import only foundation packages; no domain package imports
  another; shells import features, never other shells.
- **Wiring:** the dashboard shell connects map-view and scenario-control only through the stream source
  it passes as props.
- **Library confinement:**
  - `maplibre-gl` only in map-view;
  - `zustand` only in engine-stream;
  - `@tanstack/react-query` only in data;
  - `react-router` only in `src/`.
  dependency-cruiser enforces all four.

### Data flow
```
umbra serve ──REST──> @umbra/data (orval hooks; M5b UI uses none yet, e2e uses plain functions)
            ──WS────> @umbra/engine-stream: socket -> Decoder(json) -> pending slot (latest wins)
                        -> rAF flush -> Zustand store {status, run, frame}
                             ├─ useRunStatus / useStreamStatus (narrow selectors) -> scenario-control
                             └─ EntitySource.subscribe (imperative) -> map-view -> MapView.setEntities
                                  -> at most one GeoJSON setData per animation frame -> MapLibre worker
```
- **Store shape:** `{ status: StreamStatus; hello?: Hello; run?: RunStatus; frame?: EntityFrame;
  errors: number }`. `EntityFrame = { session; tick; time_s; seq; dropped; entities: readonly
  EntitySummary[] }`, and its entities array is frozen. Frames replace each other; a new `session` drops
  the old frame (ADR 0045).
- **Subscription:** `{ max_rate_hz: 30 }`. No filter in M5b; `setSubscription` exists for M8's
  viewport filter.
- **Runtime config:** `/config.json` is `{ "engineUrl": "", "basemap"?: { "styleUrl": string } }` (ASM-304).
  - An empty `engineUrl` means same origin: dev and preview reach the engine through the Vite proxy
    (`/api` with `ws: true`), whose target is `UMBRA_ENGINE_URL` (default `http://127.0.0.1:7878`).
  - The stream URL is derived from it (`http`->`ws`, plus `/api/v1/stream?encoding=json`).
- **Providers in `src/App.tsx`** (outermost first): `ThemeProvider`, then `DataProvider`, then
  `EngineStreamProvider`, then `RouterProvider`. `main.tsx` imports `@umbra/ui-kit/styles.css` and
  `@umbra/map-view/styles.css` once.

### Performance plan (10,000 entities)
1. Server side: the subscription rate is 30 Hz (`max_rate_hz`). Slow clients are handled by the server
   (REQ-X-7).
2. Client side: decode on message, keep only the newest frame, flush once per animation frame (ADR 0045).
3. Map: one GeoJSON source, rebuilt by a pure `toFeatureCollection(frame, affiliationOf)` into a reused
   buffer, `setData` at most once per animation frame. Symbols are a handful of `addImage` images; there
   are no per-entity DOM nodes and no React render per entity.
4. React renders only on status, run-state and count changes. The sim-time Readout re-renders once per
   flushed frame. That is one small component.
5. Tests assert counts, not wall time (no flakiness):
   - store notifications are at most one per flush for N socket messages;
   - `setData` is called at most once per animation frame;
   - the `LiveMap` render count stays constant over 100 frames of 10,000 entities.

   Timings are recorded in the running log: decode of a 10,000-entity frame, `toFeatureCollection`
   (10,000), and `setData`-to-idle in headless Chromium (a WI-12 measurement script, not a gate).
6. Escalation if decode plus apply exceeds 16 ms per frame at 10,000 entities on the reference machine:
   - first, a Worker decoder (ADR 0045 option 3);
   - second, a custom layer or deck.gl behind `MapView` (ADR 0040).

   Either one needs a new ADR, in M8.

### Map (ADRs 0040-0042)
- **Internal interface (not exported):**
  `interface MapView { setEntities(frame: EntityFrame): void; setTheme(colors: MapColors): void;
  fitTo(bounds: GeoBounds): void; inspect(): MapInspection; destroy(): void }`.
  `createMapLibreView(container, options)` implements it.
- **`LiveMap({ source, label?, basemapStyleUrl? })`**:
  - creates the view once and subscribes to `source`;
  - follows `useTheme().resolved`, resolving token colours with `getComputedStyle`;
  - fits the camera to the first frame of each session (padding `space-16`, max zoom 15; ASM-307);
  - gives the canvas the accessible name "Live map" and announces the entity count in a polite live
    region that updates only when the count changes.
- **Styles and symbols** come from pure functions in `model/`, unit-tested without WebGL:
  - `buildMapStyle(colors, basemap)`;
  - `affiliationOf(force)`;
  - `symbolSvg(affiliation, ownership, colors)`;
  - `directionSvg(colors)`;
  - `toFeatureCollection(frame, affiliationOf)`.

### e2e harness (ADR 0044)
- **`gui/playwright.config.ts`:**
  - Projects: `ui` (favicon, theme, showcase screenshots) and `engine` (live map), Chromium only.
  - Launch args `--use-angle=swiftshader --enable-unsafe-swiftshader`.
  - Viewport 1280x800, `reducedMotion: 'reduce'`, `workers: 1`, `retries: 0`.
- **`gui/e2e/global-setup.ts`:**
  1. Resolves `UMBRA_BIN` (default `<repo>/target/release/umbra`). If it is missing, it fails with
     "build it with `cargo build -p umbra-cli --release --locked`" (ASM-311).
  2. Spawns `umbra serve --bind 127.0.0.1:0 --scenario-root examples` with cwd = repo root, and parses
     `listening on http://127.0.0.1:<port>` (timeout 30 s).
  3. Runs `vite build --mode e2e` into `gui/dist-e2e` (ignored), then `vite preview` on a free loopback
     port with `UMBRA_ENGINE_URL` set to the engine.
  4. Exports both URLs through `process.env` to the tests.

  `global-teardown.ts` stops both processes (SIGTERM, then SIGKILL after 5 s).
- **Engine test flow** (`e2e/live-map.spec.ts`):
  1. `configureEngineUrl(engine)`.
  2. Through the generated client: `loadScenario({ path: "scenarios/walking_skeleton/scenario.toml",
     clock: { mode: "stepped" }, seed: "42" })`, then `startRun()`.
  3. `page.goto("/")`. Wait for `window.__UMBRA_E2E__.map.sourceEntities().length === 3` and map idle.
  4. Read `listEntities({ limit: 1000 })`, which is at tick 0. Compare data and rendered positions.
  5. Record the rendered positions and pixels. Call `stepRun({ count: 600 })` and expect
     `completed: true, reached: 600`.
  6. Poll until the inspected frame tick is 600 and the map is idle. Then compare again with
     `listEntities` (asserting its `tick` is 600) and assert the movement thresholds.

  The server publishes the frame of the last stepped tick before it replies (docs/api.md, "Step"), so
  the poll is bounded (10 s timeout).
- The hook `window.__UMBRA_E2E__` exists only in the e2e build (ASM-309). `e2e/app.spec.ts`
  `production build has no e2e hook` builds normally and greps `dist/`.

### How `verify-frontend` becomes runnable locally (no CI, ADR 0004)
1. **WI-1** creates `gui/package.json` with every script from `gui/CLAUDE.md` plus `gen:tokens`,
   `check:tokens`, `check:contrast`, `check:licenses` and `verify`.
   - `verify` runs the ordered list from ADR 0044.
   - A step whose input does not exist yet is not faked. `verify` grows WI by WI:
     - WI-1 has gen:tokens, check:tokens, check:contrast, check:licenses, typecheck, lint, test and build;
     - WI-2 adds check:deps;
     - WI-7 adds test:e2e (the `ui` project);
     - WI-9 adds `gen:api --check` at the front.

     Each WI that adds a step updates the skill text in the same commit.
2. **Gate 15** (`docs/build.md` row and `scripts/gates.sh`), changed in WI-1:
   - active "from M5b (WI-1)", with the command `cd gui && bun install --frozen-lockfile && bun run verify`;
   - run by `scripts/gates.sh --gui`;
   - mandatory for any change under `gui/` or `api/`;
   - plain `scripts/gates.sh` prints `skip 15-gui (pass --gui)`, so the M6 track needs no Bun;
   - `gates.sh` keeps gate 15 after gate 9 so the release binary exists for e2e.

   **Human decision H-6.**
3. **Prerequisites** in `docs/build.md`, added in WI-1:
   - Bun at the pinned version;
   - Node.js LTS (22 or 24);
   - `bunx playwright install chromium` once, online;
   - a release build of `umbra` before `test:e2e`.
4. The dormant `.github/workflows/ci.yml` is not touched (ADR 0004).
5. **Engineer loop during a WI:** `bun test <path>`, `bun run typecheck`, then `bun run verify` before
   handing to review.

## Work items (each at most one commit; execution order = numbering)
Owner: frontend-engineer (implementation) after frontend-test-engineer writes the failing tests
(write-frontend-tests). Every UI WI uses `apply-design-system`. Every WI ends with `bun run verify` as it
exists at that WI, then a code-reviewer review (`review-frontend-change`). WI-1..WI-8 are the A-003 block
and come before `map-view` and the dashboard shell (human decision 2026-10-19).

### WI-1: Workspace scaffold, tooling, design-system scripts, gate 15
- **Package:** root `gui/` (app package `umbra-gui`, `private: true`, `workspaces: ["features/*",
  "feature-shells/*"]`), plus `features/ui-kit/package.json` (foundation; deps `@fontsource/archivo`,
  `@fontsource/jetbrains-mono`, `material-symbols`; `exports` `.` and `./styles.css`).
- **Files:**
  - `gui/package.json`, `bun.lock`, `bunfig.toml`, `tsconfig.base.json` (F-3: `strict`,
    `noUncheckedIndexedAccess`, `exactOptionalPropertyTypes`, `verbatimModuleSyntax`,
    `moduleResolution: "bundler"`, `jsx: "react-jsx"`), `tsconfig.json` (references), `biome.json`
    (ignores `**/generated/**`, `reference/**`, `tokens.css`, `tokens.ts`; `noExplicitAny` error;
    `noDefaultExport` except `feature-shells/*/src/*Page.tsx`), `vite.config.ts` (React plugin only for
    now), `index.html` (no icon yet), `src/main.tsx` (placeholder render), `test/setup.ts` (happy-dom
    registrator, jest-dom matchers).
  - `scripts/check-licenses.ts`, `scripts/check-tokens.test.ts`.
  - `features/ui-kit/{package.json,tsconfig.json,README.md,src/index.ts}` (empty exports).
  - `.gitignore` entries: `gui/node_modules`, `gui/dist`, `gui/dist-e2e`, `gui/test-results`,
    `gui/playwright-report`, `gui/**/.tsbuild`.
  - Docs: `docs/build.md` (row 15 and prerequisites), `scripts/gates.sh` (gate 15 block), skill
    `verify-frontend`.
- **A-003 apply steps 1-3:**
  - the three scripts in `package.json` (run with `bun scripts/...`);
  - the "Design system" block appended to `gui/CLAUDE.md`, plus the rule-5 extension (`./styles.css`
    entry, ADR 0043);
  - one line each in `.claude/agents/frontend-architect.md`, `frontend-engineer.md`,
    `frontend-test-engineer.md`;
  - the compliance line in skill `review-frontend-change`.
- **Change to `check-tokens.mjs`:** an optional `--root DIR` argument, so the test can point it at a
  fixture tree. The `try {} catch {}` around `walk` stays for missing roots, but a missing root is no
  longer silent: it is counted, and zero scanned files is a failure. This guards against the check
  passing vacuously.
- **Tests:**
  - `check-tokens.test.ts`: `a hex literal in a feature fails`, `an rgb literal fails`,
    `umbra-allow suppresses one line`, `the repository tree passes`, `an empty tree fails`;
  - `check-licenses.test.ts`: a fixture `node_modules` with `GPL-3.0` fails, a dev-only `MPL-2.0` passes,
    a runtime `MPL-2.0` fails, a missing licence fails;
  - `bun run gen:tokens --check` passes on the committed tokens;
  - `check:contrast` passes (WARN lines allowed, as documented).
- **Risks:**
  - First online install (H-3).
  - `tsc -b` with composite projects needs declaration output: use `emitDeclarationOnly` into an
    ignored `.tsbuild/` (ASM-318).
  - Bun and Node versions differ between machines: pin Bun with `packageManager`.

### WI-2: Architecture rules (dependency-cruiser) with negative fixtures
- **Files:** `gui/.dependency-cruiser.cjs`, `gui/test/architecture.test.ts`,
  `gui/test/arch-fixtures/<case>/` (each a minimal workspace).
- **Rules:**
  - `no-src-import`, `shell-imports-features-only`, `shell-not-shell`, `feature-not-shell-or-src`;
  - `foundation-only-foundation` and `domain-not-domain`, with tiers read from each `package.json`
    `umbra.tier` when the config loads;
  - `no-deep-import`: only `@umbra/<x>` and declared `./styles.css` entries; no relative path crossing a
    package root;
  - `no-circular`;
  - `no-engine-source` (REQ-A-4): nothing resolves outside `gui/` except `node_modules`, so no
    `../crates`, `../api`, `../examples`;
  - `no-generated-from-outside` (only the owning package imports its `src/generated/`);
  - the library-confinement rules: `maplibre-only-in-map-view`, `zustand-only-in-engine-stream`,
    `react-query-only-in-data`, `router-only-in-src`;
  - `not-to-unresolvable`, and `no-non-package-json` (every import is a declared dependency).
- **Tests:** one fixture per rule asserts `check:deps` fails with that rule's name, and the real tree
  passes. Also `plain request functions are not imported by features or shells` (ADR 0045), a source
  scan with a fixture.
- **Risks:**
  - dependency-cruiser resolution of Bun workspace symlinks and TS `exports`: configure
    `tsConfig` and `enhancedResolveOptions.exportsFields`, and prove it with the fixtures.
  - Engine-source detection on Windows paths (M8).

### WI-3: ui-kit foundations: styles entry, base styles, theme, i18n, Icon
- **Public API added:**
  - `ThemeProvider({ defaultPreference?: ThemePreference = "dark", storage?: Pick<Storage,"getItem"|"setItem">, children })`;
  - `useTheme(): { preference, resolved: ResolvedTheme, setPreference }`;
  - `defineMessages<const M extends Record<string,string>>(en: M): (key: keyof M, params?: Record<string,string|number>) => string`;
  - `Icon({ name })`;
  - the token re-exports.
- **Behaviour:** `ThemeProvider` sets `document.documentElement.dataset.theme`, persists the preference
  under `umbra.theme`, and follows `prefers-color-scheme` only for `system`. The default is Ops (dark),
  ASM-301.
- **Files:** `src/styles.css` (imports `fonts.css`, `tokens/tokens.css`, `base.css`, then each component
  CSS as components land), `src/base.css` (body on `surface-0`, `ink`, `.type-body`, focus ring rule,
  reduced motion), `src/theme/*`, `src/i18n/*`, `src/components/Icon/*`.
- **Tests:**
  - `ThemeProvider.test.tsx`: `starts in dark without a stored preference`,
    `restores a stored preference`, `setPreference switches data-theme without remount`,
    `system follows the media query`;
  - `defineMessages.test.ts`: interpolation, unknown param left visible;
  - `Icon.test.tsx`: `aria-hidden` glyph by ligature.
- **Risks:** a font import path differs after install. Verify the paths, as `fonts.css` says.

### WI-4: Components A: Button, Chip and ChipGroup, Badge, AffiliationTag, Alert
- **Porting rule (all component WIs):** port from `reference/vantage/<Name>/README.md`, `preview.html`,
  the `.vt-<name>` rules in `bundle.css`, the behaviour in `bundle.js` and the props in `index.d.ts`.
  Keep the same names, props and class names. CSS uses tokens only.
- **Files:** `src/components/<Name>/<Name>.tsx`, `<Name>.css`, `<Name>.test.tsx`, plus
  `src/components/props.test-d.ts`. Each component's CSS is added to `styles.css`.
- **Tests:**
  - role and label queries; keyboard operation (Enter and Space on Button and Chip, arrow keys in
    ChipGroup if the reference implements them);
  - disabled state;
  - icon-only Button requires `aria-label` (a type-level test plus a runtime dev warning test);
  - `ChipGroup` exposes `role="group"` with its label;
  - Badge tone classes;
  - AffiliationTag default words;
  - Alert `role` (`alert` for danger and warning, `status` otherwise, if the reference says so;
    otherwise ASM).
- **Risks:**
  - Reference behaviour in `bundle.js` may rely on globals: re-implement it as React state.
  - Class-name drift: the tests assert the `vt-*` classes.

### WI-5: Components B: Panel, Card, TextField, Toggle, Slider, Tabs, plus ThemeToggle
- **Tests:**
  - TextField: label association, `help` via `aria-describedby`, `invalid` -> `aria-invalid`;
  - Toggle: `role="switch"`, `aria-checked`, Space toggles;
  - Slider: label, the formatted value shown, arrow keys change it;
  - Tabs: `tablist`/`tab`/`aria-selected`, arrow-key roving focus, `count` rendered;
  - Panel: a `region` labelled by its title, width 320 by default and 400 when given;
  - Card: levels;
  - `ThemeToggle` (a composition of `Toggle` and `useTheme`, labelled "Day theme"): toggling sets
    `data-theme`.
- **Risks:** glass on glass. A Panel inside a floating CommandBar is a review check.

### WI-6: Components C: CommandBar, Readout, EntityRow and EntityList
- **Tests:**
  - CommandBar: `banner` landmark (docked), clock in cyan while running and `ink-muted` while paused
    (class assertions), `floating` adds the glass class, children slot;
  - Readout: label/value association, `live` class, unit with a thin space, trend icon;
  - EntityRow: `option` in the `listbox` of `EntityList`, `aria-selected`, `stale` word, status Badge,
    `onSelect` on click and Enter.
- **Risks:** EntityList virtualization is not in the reference. M8 adds it when the list exists; it is
  noted, not built.

### WI-7: App shell: providers, router, runtime config, favicon, Playwright `ui` project
- **Files:**
  - `gui/src/{main.tsx,App.tsx,router.tsx,config.ts,config.test.ts,index-html.test.ts}`;
  - `gui/public/config.json` (`{"engineUrl":""}`);
  - `gui/index.html` (`<link rel="icon" type="image/svg+xml" href="/umbra_icon.svg">`, title "Umbra");
  - `gui/vite.config.ts`: the `umbra-icon` plugin (below), the `/api` proxy with `ws: true` to
    `UMBRA_ENGINE_URL`, and the same proxy under `preview`; e2e mode defines `VITE_UMBRA_E2E`;
  - `gui/playwright.config.ts`, `gui/e2e/global-setup.ts`, `global-teardown.ts`, `gui/e2e/app.spec.ts`.
- **The `umbra-icon` plugin** serves `../umbra_icon.svg` at `/umbra_icon.svg` in dev and preview, and
  emits it byte-for-byte (`emitFile` with `fileName: "umbra_icon.svg"`) in build. There is no committed
  copy and nothing is redrawn.
- **Routing and providers:** routes come from shell `route` objects (`createBrowserRouter`, `lazy`).
  Until WI-8 and WI-15 the router has a single placeholder route; the 404 route reads "Page not found."
  Providers: `ThemeProvider` now; `DataProvider` and `EngineStreamProvider` are wired in WI-9 and WI-10.
- **`loadRuntimeConfig(fetchImpl)`** validates the shape. Bad JSON shows an `Alert`: "Configuration
  could not be read. Check /config.json." This is the app's only fetch; it is not engine REST.
- **Global setup in this WI** starts only `vite preview`. The engine part comes in WI-16.
- **Tests:**
  - `index-html.test.ts` (the link tag);
  - `config.test.ts` (defaults, empty URL means same origin, invalid shape rejected, `basemap.styleUrl`
    passed through);
  - `e2e/app.spec.ts`: `the page links umbra_icon.svg as an svg icon`,
    `the served icon is byte-identical to the repository root file` (compares
    `GET /umbra_icon.svg` bytes with `fs.readFile("../umbra_icon.svg")`), `unknown route shows page not found`.
- **Risks:** a Vite preview proxy with WebSocket upgrade must pass the server's `Origin` check (loopback
  origin, docs/api.md) and its `Host` check (no `changeOrigin`, so `Host` stays loopback). WI-16 proves
  this.

### WI-8: Showcase shell and screenshot tests (closes A-003)
- **Package:** `@umbra/shell-showcase` (shell), route `/showcase`.
- **`ShowcasePage` (layout only):**
  - a CommandBar (docked) with `ThemeToggle`;
  - token sections: colour swatches from `colorTokens`, each labelled with its name in `data`; type
    specimens from `typeStyles`; spacing and radius scales; shadows and glass on a sample surface;
  - every one of the 14 components in its reference states (from each `preview.html`), with at most one
    filled azure Button per view section.
- **Tests:**
  - `ShowcasePage.test.tsx`: every token name and every component's accessible name are present;
  - `e2e/showcase.spec.ts`: `showcase tokens (ops)`, `showcase tokens (day)`,
    `showcase components (ops)`, `showcase components (day)`, full-page screenshots with
    `maxDiffPixelRatio: 0.001`;
  - `e2e/app.spec.ts` `starts in ops and switches to day without a reload` (a `window` marker survives,
    `navigation` entries unchanged);
  - `e2e/showcase.spec.ts` `showcase slider arrow keys`: focus the showcase Slider, press ArrowRight,
    assert its `.vt-slider-value` text and `aria-valuetext` change (native arrow keys are not
    reproducible in happy-dom, ASM-336 (3)).
  - screenshot item `showcase floating command bar (ops|day)`: a floating CommandBar with a glass Chip
    and a Panel inside, both themes, no second blur (guards the glass-on-glass CSS rule, ASM-337 (2)).
- **Baselines** are generated on Linux and committed under `gui/e2e/__screenshots__/`. The human looks
  at them once (H-10).
- **Risks:** screenshot flakiness from font loading. Wait for `document.fonts.ready`, and disable caret
  and animations.

### WI-9: `@umbra/data` and `gen:api` (both outputs), engine-stream skeleton
- **Files:**
  - `features/data/{package.json,tsconfig.json,README.md,orval.config.ts}`;
  - `src/{index.ts,fetcher.ts,problem.ts,DataProvider.tsx}`, `src/generated/` (generated);
  - `features/engine-stream/{package.json,...}` with `src/generated/stream.d.ts` and an index exporting
    the stream types only;
  - `gui/scripts/gen-api.mjs` (runs orval, then `json-schema-to-typescript` on
    `../api/stream.schema.json`, and prepends `// GENERATED, DO NOT EDIT`; `--check` generates into a
    temp dir and diffs);
  - `src/App.tsx` gains `DataProvider`.
- **orval config:** `client: 'react-query'`, `httpClient: 'fetch'`, mutator `src/fetcher.ts`, single
  output file, `clean: true`.
- **The mutator** prefixes the configured engine URL, sends JSON with `content-type`, returns parsed
  bodies (204 -> `undefined`), and turns `application/problem+json` into
  `ApiProblem { code, status, title, detail, diagnostics? }`. A non-problem error body becomes
  `ApiProblem { code: "internal" }` with the status.
- **First-use check (skill step 5):** orval must handle OpenAPI 3.1:
  - `["integer","null"]` as nullable;
  - `oneOf` with null;
  - the `ids` array query style;
  - `application/problem+json`.

  If it does not, stop and report. The contract is never changed to suit the generator.
- **Tests:**
  - `fetcher.test.ts` (fake fetch: base URL joining, problem decoding, 204, network error);
  - `DataProvider.test.tsx` (a hook resolves through a fake fetch);
  - `gen-api.test.ts` (`--check` fails after a one-byte change in a temp copy);
  - `verify` gains `gen:api --check` as its first step.
- **Risks:**
  - orval's 3.1 handling (above).
  - Large single-file output: lint-ignore `generated/`.

### WI-10: `@umbra/engine-stream`
- **Public API:** as in the package table. `connectEngineStream` per ADR 0045. The socket factory and
  frame scheduler are injectable, with defaults `new WebSocket(url)` and `requestAnimationFrame`.
- **Files:** `src/model/{decode.ts,store.ts,backoff.ts}`, `src/connection.ts`,
  `src/EngineStreamProvider.tsx`, `src/hooks.ts`. `src/App.tsx` creates the connection from the config.
- **Tests (fake socket, fake scheduler, fake timers):**
  - `sends subscribe after hello`;
  - `unknown message types and fields are ignored`;
  - `error messages are counted and the connection stays open`;
  - `latest frame wins within one animation frame`;
  - `a new session replaces all entities`;
  - `seq gaps accumulate dropped`;
  - `reconnects with backoff and resets on hello`;
  - `close stops reconnecting`;
  - `setSubscription sends a new subscribe`;
  - `encoding other than json is rejected at construction`;
  - `useRunStatus re-renders only on run changes`;
  - performance: `10,000-entity frames: one store notification per flush and constant hook renders`.
- **Risks:**
  - Whether the server requires the `umbra.stream.v1` subprotocol header: read `docs/api.md` and
    `crates/umbra-server` (read only) and follow it.
  - Integer `tick` beyond 2^53 is impossible in practice (documented).

### WI-11: `@umbra/map-view` model: interface, style, symbols, affiliation
- **Files:** `features/map-view/{package.json,...}`; `src/model/{affiliation.ts,style.ts,symbols.ts,
  features.ts,colors.ts}`; `src/MapView.ts` (the internal interface).
- **`colors.ts` `resolveMapColors(el)`** reads the token custom properties needed
  (`surface-0`, `surface-1`, `line`, `line-strong`, `ink-muted`, `symbol-frame`, `affil-*`,
  `opacity-track`).
- **`docs/design-system/tactical-display.md`** gains the remote-entity rule, if H-8 is accepted.
- **Tests:**
  - `affiliation.test.ts`: the mapping table of ASM-305, case-insensitive, null;
  - `symbols.test.ts`:
    - four frame geometries;
    - fill and stroke come from the input colours only (no literal: the SVG output contains only
      values passed in);
    - the remote ring is present only for `remote`;
    - stroke width 1.5;
  - `style.test.ts`:
    - layer order background, land, coast, graticule, entities, direction;
    - Day graticule uses `line-strong` at `opacity-track`;
    - no `glyphs` key;
    - `styleUrl` replaces the base;
  - `features.test.ts`: ids, coordinates `[lon, lat]`, properties `affiliation`, `ownership`,
    `heading_deg`, `moving`; 10,000 entities produce one collection.
- **Risks:** `check:tokens` flags a colour literal in an SVG template. Colours are only interpolated
  values, which the tests prove.

### WI-12: MapLibre implementation and `LiveMap`
- **Files:** `src/maplibre/createMapLibreView.ts`, `src/LiveMap.tsx`, `src/e2eHook.ts` (registered only
  when `import.meta.env.VITE_UMBRA_E2E`), `src/styles.css` (imports `maplibre-gl/dist/maplibre-gl.css`
  and container rules), `README.md`.
- **Behaviour:**
  - `setEntities` stores the frame and schedules one flush per animation frame;
  - the theme switch rebuilds the images and calls `setPaintProperty`, with no reload;
  - the first frame of a session triggers `fitTo`;
  - `inspect()` returns `sourceEntities()`, `renderedEntities()` (`queryRenderedFeatures`),
    `project(lon, lat)`, `isIdle()` and `tick()`.
- **Tests:**
  - happy-dom with `maplibre-gl` mocked at the module boundary (WebGL is not available): `LiveMap` creates
    one view, subscribes and unsubscribes, `setData` at most once per animation frame over 50 frames,
    theme change calls `setTheme` without recreating, a session change refits, `LiveMap render count is
    constant over 100 frames of 10,000 entities`;
  - a measurement script `gui/scripts/measure-map.ts` (Playwright, not a gate) logs `setData`-to-idle
    for 10,000 synthetic entities in the running log.
- **Risks:**
  - Headless WebGL (R-3).
  - `setData` cost at 10,000 (R-4).

### WI-13: Natural Earth offline basemap (only if H-2 accepts the asset)
- **Files:**
  - `features/map-view/assets/ne_50m_{land,coastline,admin0_lines}.geojson`, simplified, total under
    1.5 MB, with a provenance `README.md` (source URL, version, public domain, simplification command);
  - `style.ts` adds the sources and layers. Vite bundles the assets as URLs.
- **Tests:**
  - `style.test.ts` (land and coast layers present, colours from tokens);
  - `assets.test.ts` (valid GeoJSON, size budget).
- **If H-2 is declined:** drop this WI. The style stays background plus graticule (ADR 0041).

### WI-14: `@umbra/scenario-control` (read-only slice)
- **Public API:** `RunStatusBar()` and `ConnectionAlert()`.
- **Model:**
  - `formatSimTime(time_s)` -> `hh:mm:ss` elapsed, prefixed "T+" (ASM-308);
  - `runStateLabel(state)` -> Idle, Ready, Running, Paused, Stopped, with tone (ASM-319);
  - `connectionMessage(status)` -> "Engine not reachable. Reconnecting." as an `Alert`, tone warning.
- **Rendering:** Readouts for sim time (live while running), tick and entity count, plus a Badge for
  the run state, all as CommandBar children.
- **Tests:** the model tables; component tests with a fake connection (status and count changes); the
  live class only while running.
- **Risks:** scope creep into M8 controls. No buttons in M5b.

### WI-15: `@umbra/shell-dashboard` (map only) and the route
- **`DashboardPage`:**
  - full-viewport `LiveMap` (the map is the page);
  - a floating CommandBar (glass) with brand "Umbra", title "Live map", the `RunStatusBar` children
    and `ThemeToggle`;
  - `ConnectionAlert` anchored below the CommandBar;
  - `source` from `useEntitySource()`.
- **`src/router.tsx`** registers `/` (dashboard) and `/showcase`.
- **Tests:** `DashboardPage.test.tsx` with a fake stream connection and map-view rendered with
  maplibre mocked: the map region, CommandBar, status and Alert appear; a frame updates the count
  Readout; no fetch is called.
- **e2e:** `e2e/dashboard.spec.ts` `dashboard (ops|day)` screenshots with the map canvas masked, and
  the axe check if H-3 includes `@axe-core/playwright`.
- **Risks:** glass legibility over the map (a WARN in `check:contrast`, by design).

### WI-16: e2e live map against a real headless `umbra serve`
- **Files:** `gui/e2e/global-setup.ts` (the engine part), `gui/e2e/live-map.spec.ts`,
  `gui/e2e/support/{geo.ts,engine.ts}` (haversine; the generated client configured for the engine URL).
- **Tests (acceptance 1):** `all walking_skeleton entities appear on the map`,
  `after step(600) the entity layer equals the engine list for that tick and every marker moved`,
  `a reload of the scenario replaces the entities` (load again: a new session; three entities; tick 0).
  Tolerances as in the acceptance mapping.
- **Risks:**
  - Port discovery and process cleanup on failure: teardown always runs, and the processes are
    started with `detached: false`.
  - The Vite preview proxy and the WebSocket `Origin`/`Host` checks.
  - A slow software renderer: idle timeouts are 10 s.

### WI-17: Close-out
- **Docs:**
  - `docs/frontend-architecture.md`: inventory (ui-kit complete, scenario-control slice,
    shell-showcase, milestone column), data flow, the `./styles.css` entry rule;
  - `docs/test-matrix.md`: REQ-A-4 and the M5b rows;
  - `docs/milestones.md`: tick M5b with evidence;
  - `docs/assumptions/M5b.md`: final;
  - SPEC D-rows for the accepted ADRs and the open question 6 answer;
  - `gui/README.md` (run dev against `umbra serve`).
- **Verification:**
  - `scripts/gates.sh --gui` (full engine gate plus gate 15);
  - the engine-isolation diff (acceptance 4);
  - goldens unchanged.
- Review by code-reviewer. Tag `m5b-done` (human).

## Test plan summary
| Level | Where | What |
|---|---|---|
| Script | `gui/scripts/*.test.ts` | check-tokens (A-003 6d), check-licenses, gen-api drift |
| Architecture | `gui/test/architecture.test.ts` | each dependency-cruiser rule with a failing fixture; REQ-A-4; plain-request import scan |
| Model | `features/*/src/model/*.test.ts` | decode, store, backoff, affiliation, symbols, style, features, formatters |
| Component | `*.test.tsx` (happy-dom, Testing Library, role queries) | 14 components, theme, LiveMap (mocked maplibre), RunStatusBar, pages |
| Performance (counts) | engine-stream, map-view | 10,000 entities: notifications and `setData` per frame, render counts |
| e2e `ui` | `e2e/app.spec.ts`, `showcase.spec.ts`, `dashboard.spec.ts` | favicon, theme switch, screenshots Ops/Day, axe |
| e2e `engine` | `e2e/live-map.spec.ts` | acceptance 1 against real `umbra serve` |

## Risks (milestone level)
| ID | Risk | Mitigation |
|---|---|---|
| R-1 | orval mishandles OpenAPI 3.1 (type arrays, problem+json) | WI-9 first-use check; stop and report; never edit the contract |
| R-2 | Playwright needs Node and a downloaded Chromium; Bun-only machines fail | Prerequisites in `docs/build.md` (H-5, H-3); clear error messages in global setup |
| R-3 | Headless Chromium no longer falls back to SwiftShader for WebGL automatically | Launch flags `--use-angle=swiftshader --enable-unsafe-swiftshader`; WI-7 smoke test creates a WebGL context |
| R-4 | MapLibre `setData` with 10,000 points every frame exceeds the budget | Counts tested; timings measured (WI-12); escalation path in ADR 0040/0045 for M8 |
| R-5 | Screenshot flakiness | Self-hosted fonts, `document.fonts.ready`, reduced motion, masked canvas, Linux-only baselines |
| R-6 | dependency-cruiser does not resolve Bun workspace `exports` | Explicit resolver options; fixtures prove each rule fires |
| R-7 | Merge conflicts with M6 in shared docs and `gates.sh` | Local edits only; second merger rebases |
| R-8 | `check-tokens.mjs` passes vacuously when roots are missing | WI-1 makes zero scanned files a failure |
| R-9 | Scope creep into M8 (controls, inspector) | scenario-control is read-only; no selection; reviewer checks |
| R-10 | The Vite proxy changes `Host`/`Origin` and the server refuses | No `changeOrigin`; loopback preview; WI-16 proves it |

## Decisions for the human
Each item needs a yes/no or a choice. Each gives a recommendation (**Rec**). ADRs stay Proposed until
answered.

**H-1 Map library (ADR 0040, D-56).** Use MapLibre GL JS 5.x (BSD-3-Clause, no token) behind `MapView`.
The alternatives are CesiumJS (3D, ion token), OpenLayers, deck.gl and Leaflet.
- **Rec: MapLibre GL JS.**

**H-2 Basemap and network (ADR 0041, D-57).** Three choices:
- (a) offline style from the tokens (background, graticule), plus bundled Natural Earth 1:50m land,
  coastline and borders (public domain, under 1.5 MB committed);
- (b) the offline style without Natural Earth (grid only);
- (c) online tiles by default.

Separately: should an opt-in `basemap.styleUrl` runtime key ship now (off by default; when set, the
app fetches that style at runtime)?
- **Rec: (a), and ship the opt-in key.** No network at runtime by default and none in any test, so the
  app is fully offline-friendly.

**H-3 New npm dependencies (ADR 0043, D-59).** Each one is listed below with its expected version and
licence.
- Human decision 2026-10-20: where the registry major differed (typescript 7, vite 8, plugin-react 6, happy-dom 21, jest-dom 7), the plan's major is pinned; all others take the latest within the plan's major. WI-1 recorded the installed versions and licences in ADR 0043 ("Installed versions"); fonts are OFL-1.1, not MIT.
- These versions are what I know as of planning. I could not query the registry. WI-1 pins exact
  versions in `bun.lock` and records them in ADR 0043. If a resolved **major** differs from this table,
  work stops for you.
- Needs network: one online `bun install`, plus `playwright install chromium` (about 170 MB).

| Package | Expected version | Licence | Used by | Kind |
|---|---|---|---|---|
| react | 19.2.x | MIT | all | runtime |
| react-dom | 19.2.x | MIT | src, tests | runtime |
| react-router | 7.9.x | MIT | src (router) | runtime |
| @tanstack/react-query | 5.90.x | MIT | data | runtime |
| zustand | 5.0.x | MIT | engine-stream | runtime |
| maplibre-gl | 5.9.x | BSD-3-Clause (deps ISC, MIT, BSD-2/3-Clause) | map-view | runtime |
| @fontsource/archivo | 5.2.x | MIT (font files OFL-1.1) | ui-kit | runtime |
| @fontsource/jetbrains-mono | 5.2.x | MIT (font files OFL-1.1) | ui-kit | runtime |
| material-symbols | 0.36.x | Apache-2.0 | ui-kit | runtime |
| typescript | 5.9.x | Apache-2.0 | root | dev |
| vite | 7.1.x | MIT | root | dev |
| @vitejs/plugin-react | 5.0.x | MIT | root | dev |
| @types/react, @types/react-dom | 19.2.x | MIT | root | dev |
| @types/bun | 1.3.x | MIT | root | dev |
| orval | 7.13.x | MIT | data | dev |
| json-schema-to-typescript | 15.0.x | MIT | root (gen:api) | dev |
| @biomejs/biome | 2.2.x | MIT OR Apache-2.0 | root | dev |
| dependency-cruiser | 17.0.x | MIT | root | dev |
| @playwright/test (+ pinned Chromium) | 1.55.x | Apache-2.0 (Chromium BSD-3-Clause and others) | root | dev |
| @happy-dom/global-registrator (happy-dom) | 20.0.x | MIT | root | dev |
| @testing-library/react | 16.3.x | MIT | root | dev |
| @testing-library/dom | 10.4.x | MIT | root | dev |
| @testing-library/jest-dom | 6.8.x | MIT | root | dev |
| @testing-library/user-event | 14.6.x | MIT | root | dev |
| @axe-core/playwright (axe-core) | 4.10.x | MPL-2.0 | root (e2e axe checks) | dev, optional |

Notable dev-only transitive licences: `caniuse-lite` (CC-BY-4.0, via Babel/browserslist), `argparse`
(Python-2.0), and possibly BlueOak-1.0.0 and CC0-1.0.
- **Rec: accept all, including `@axe-core/playwright`.** It is dev-only, and MPL-2.0 is file-level
  copyleft, never shipped. Router choice: react-router 7, over TanStack Router or no router.

**H-4 npm licence policy (ADR 0043).** Proposed allow lists for `bun run check:licenses` (gate 15):
- Shipped bundle: MIT, ISC, BSD-2-Clause, BSD-3-Clause, Apache-2.0, 0BSD, and OFL-1.1 for fonts.
- Dev-only, in addition: MPL-2.0, CC-BY-4.0, Python-2.0, BlueOak-1.0.0, CC0-1.0.
- **Rec: accept.** It mirrors `deny.toml` for shipped code.

**H-5 Toolchain prerequisites.** Bun, pinned with `packageManager`, plus Node.js LTS (22 or 24) for
Playwright, orval and dependency-cruiser.
- **Rec: accept.** Playwright does not support the Bun runtime.

**H-6 Gate 15 (ADR 0044, D-60).** Change `docs/build.md` row 15 from "from M8" to "from M5b (WI-1)",
with the command `cd gui && bun install --frozen-lockfile && bun run verify`. The `verify` list adds
`gen:tokens --check`, `check:tokens`, `check:contrast`, `check:licenses` and `test:e2e` to today's row.
It runs via `scripts/gates.sh --gui` and is mandatory for changes under `gui/` or `api/`. Plain
`scripts/gates.sh` still skips it, so the M6 track needs no Bun or Chromium. The Tauri build stays M8.
- **Rec: accept.** This changes a canonical docs decision (the gate list), so it needs your yes.

**H-7 GUI rule amendments (ADR 0043, ADR 0045; `gui/CLAUDE.md`, `docs/frontend-architecture.md`).**
- (a) A package may declare one extra export `./styles.css`. This amends rule 5.
- (b) `@umbra/data` also re-exports orval's plain request functions, for `e2e/` and scripts only. An
  architecture test enforces this.
- (c) `@umbra/scenario-control` starts in M5b as a read-only slice.
- (d) A new shell `@umbra/shell-showcase` at `/showcase`, shipped (code-split).
- (e) `ThemeToggle` is a ui-kit composition, not a new visual component.
- **Rec: accept all five.**

**H-8 Symbols (ADR 0042, D-58).**
- Draw the APP-6D affiliation frames ourselves; no `milsymbol` until the contract carries entity types.
- The default force-to-affiliation mapping: blue -> friend, red -> hostile, green -> neutral, else
  unknown.
- An APP-6D direction-of-movement line.
- A new design-language rule: remote entities get a 1px `ink-muted` outer ring. It is added to
  `tactical-display.md`.
- **Rec: accept.** If you prefer a different remote marker, say which.

**H-9 A-003 apply steps 2-3.** These edit `gui/CLAUDE.md`, three `.claude/agents/frontend-*.md` files and
the `review-frontend-change` skill, with the exact text from A-003.
- **Rec: confirm.** A-003 already mandates them, but they change agent configuration, so you should say
  yes explicitly.

**H-10 Screenshot baselines.** Linux-only, rendered by the Playwright-pinned Chromium. You review the
showcase and dashboard baselines once, in WI-8 and WI-15.
- **Rec: accept.**

**Decision rows** (added to SPEC section 2 once accepted):
- D-56 map library (0040);
- D-57 basemap (0041);
- D-58 symbols (0042);
- D-59 GUI dependencies and licence policy (0043);
- D-60 gate 15 and e2e harness (0044);
- D-61 live data flow (0045);
- D-62 unused.

## Assumptions
ASM-300..ASM-326 are in `docs/assumptions/M5b.md` (proposed with this plan; final at WI-17).

## Running log
### Status
- 2026-10-20: plan drafted by frontend-architect (Proposed), along with ADRs 0040-0045 (Proposed) and
  `docs/assumptions/M5b.md`.
- 2026-10-20: the human accepted the plan and ADRs 0040-0045; every recommendation H-1..H-10 as written.
- 2026-10-20: human decisions during WI-1: pin the plan's npm majors (TypeScript 5.9, Vite 7,
  plugin-react 5, happy-dom 20, jest-dom 6) with the latest minor in each, although newer majors exist;
  the Bun project lives in this repository as the monorepo workspace under `gui/` (no separate repo).
- 2026-10-20: **paused by the human** after WI-3. M5b runs in parallel with M6 in the worktree
  `../umbra-m5b` (branch `m5b`, not yet merged into `develop`); M6 is in `../umbra-m6` (branch `m6`).

### In progress
- Nothing. The working tree is clean at the WI-7 commit.

### Next
- WI-8 (showcase shell and screenshot tests, closes A-003; baselines committed and marked "pending human review"
  per the human's 2026-10-21 away instruction), then WI-9..WI-17.
- Open nit from the WI-2 review: the `scripts/` entry of the second `no-deep-import` rule has no fixture.
- Merge `m5b` into `develop` when M5b closes (shared docs may conflict, see `docs/assumptions.md`).

### Done
- WI-7 (2026-10-21, commit `616cdaf`, reviewed GO-with-fixes, fixes applied incl. the new foundation feature `@umbra/runtime-config` (ASM-339), Playwright as a dev dependency, router created once in `main.tsx`, `--port 0` preview, `/api/` proxy key): app shell in `gui/src` (`main`, `App`, `router`, `messages`), `public/config.json`, `index.html` icon link, `umbra-icon` Vite plugin (dev, preview, build; byte-identical, no copy), `/api` proxy with `ws: true`, Playwright `ui` project with global setup/teardown (build `--mode e2e`, `vite preview`), `e2e/app.spec.ts` (5 tests; 202 bun tests), unit tests in `runtime-config` (model, provider), `index-html.test.ts`, `App.test.tsx`; `test:e2e` added to `verify`; verify-frontend skill updated. ASM-338..343.
- WI-6 (2026-10-21, reviewed GO-with-fixes, fixes applied (dead biome-ignores removed, U+00A0 echelon, paused needs a visible word, WI-8 glass screenshot item, roving tab stop deferred to M8)): ui-kit components C: `CommandBar`, `Readout`, `EntityRow`/`EntityList` (tsx, css, tests each), shared `VisuallyHidden.css`, `props.test-d.ts` cases, README and `index.ts` updated; `bun run verify` passes (179 tests); EntityList virtualization not built (M8). ASM-337 (incl. the glass-on-glass CSS guard for a floating CommandBar).
- WI-5 (2026-10-21, reviewed GO-with-fixes, fixes applied (Tabs label/id/aria-controls, count separator and tabular figures, TextField aria-invalid and empty id; Slider arrow-key e2e scheduled in WI-8)): ui-kit components B: `Panel`, `Card`, `TextField`, `Toggle`, `Slider`, `Tabs`, `ThemeToggle` (tsx, css, tests each), shared `Label.css`, new type-level cases in `props.test-d.ts`, README and `index.ts` updated. `bun run verify` passes (151 tests). ASM-336 (incl. the glass-on-glass note for the WI-8 review).
- WI-4 (2026-10-21, reviewed GO-with-fixes, fixes applied (aria-labelledby variant, NonNullable children, shared `gui/test/jest-dom.d.ts`, README exports)): ui-kit components A: `Button`, `Chip`/`ChipGroup`, `Badge`, `AffiliationTag`, `Alert` (tsx, css, tests each), `props.test-d.ts` (type-level icon-only `aria-label` rule), `gui/test/jest-dom.d.ts`, CSS in `styles.css`, exports in `index.ts`. `bun run verify` passes (107 tests). ASM-335.
- WI-1 (2026-10-20, commit `09cc8b6`, reviewed GO-with-fixes, fixes applied): `gui/` Bun workspace (root `gui/package.json`, `gui/bun.lock`, `features/ui-kit`), tsconfig/biome/vite/bunfig/test setup, `check-licenses.ts`, `check-tokens.mjs --root` with the vacuous-pass guard, the three design-system scripts wired, gate 15 in `scripts/gates.sh` and `docs/build.md`, A-003 apply steps 1-3 (H-9), `verify-frontend` skill. `bun run verify` (WI-1 steps 2-7, 9, 10) passes. ASM-322..ASM-326.
- WI-2 (2026-10-20, commit `445f46f`, reviewed GO-with-fixes, fixes applied incl. `no-undeclared-workspace-dep`): `gui/.dependency-cruiser.cjs` (all rules of the WI list, tiers read from `umbra.tier`), `gui/test/architecture.test.ts` with `gui/test/arch-fixtures/` (one failing fixture per rule, REQ-A-4 `../crates` and `../api/openapi.json` cases, meta test that every rule has a fixture), `gui/scripts/check-plain-requests.ts` (ADR 0045), `check:deps` in `verify` (after lint, ADR 0044), dependency-cruiser 17.4.3 recorded in ADR 0043. ASM-327..ASM-331.
- WI-3 (2026-10-20, commit `e287d89`, reviewed GO-with-fixes, fixes applied incl. the `check-tokens` `var()` false positive and the happy-dom preload): `@umbra/ui-kit` foundations: `styles.css` entry, `base.css`, `ThemeProvider`/`useTheme` (`src/theme/`), `defineMessages` (`src/i18n/`), `Icon`, token re-exports; tests `ThemeProvider.test.tsx`, `defineMessages.test.ts`, `Icon.test.tsx`; `bun run verify` passes. ASM-332..ASM-334.

### Deferred
- To M8:
  - selection ring and label pill, stale and destroyed styling, map text (glyphs);
  - force-to-affiliation overrides in settings;
  - viewport `bbox` subscription;
  - EntityList virtualization;
  - any EntityList shown with live entities first gets a single roving tab stop (APG listbox);
  - Tauri origins and bundle icons from `umbra_icon.svg`;
  - Windows screenshots.
- Later ADR: `milsymbol` once the contract has entity types.

### Assumed
See `docs/assumptions/M5b.md` (ASM-300..ASM-343 so far). Added during implementation: ASM-322..326
(exact pins, licence rules incl. OFL-1.1 only for `@fontsource/*`, tsconfig layout, `gui/**/node_modules`,
biome preset), ASM-327..331 (dependency-cruiser details, app root exempt from the workspace-dep rule,
plain-request allow-list until WI-9), ASM-332..334 (remaining `umbra-allow` lines, own-keys i18n lookup,
happy-dom preload order and the `^19.3.1` React peer range).
