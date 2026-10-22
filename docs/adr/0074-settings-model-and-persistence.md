# ADR 0074: Settings: a versioned model in a foundation package, persisted per platform

- **Status**: Proposed (frontend-architect, M8 plan `docs/plans/M8.md`, 2026-10-22)
- **Requirement IDs**: spec section 13 ("connect to engine"), A-002 M8 ("settings"), ASM-301 (theme), ASM-304 (runtime config), ASM-305 (affiliation), ASM-315 (stream rate), ADR 0075, ADR 0076
- **Affected**: new foundation package `@umbra/settings`, `@umbra/ui-kit` (`ThemeProvider` storage), `@umbra/desktop`, `@umbra/shell-settings`, `gui/src`, decision row D-84

## Context
M8 adds user settings: theme (exists, `umbra.theme` in `localStorage`), stream rate, force-to-affiliation
overrides (deferred from M5b), event-journal size, recent scenario paths and, in the desktop app, how to
reach the engine (managed sidecar or remote URL). They are distinct from the runtime config
`/config.json`, which is deployment-owned and read once at startup.

Two platform facts shape persistence:
1. In the web build `localStorage` is per origin and works.
2. In the desktop app the webview's origin is the gateway's loopback port (ADR 0075). The port changes
   per launch, so `localStorage` would be lost. Also, the engine-connection settings must be readable by
   the Rust side before the webview starts.

## Options
1. **One versioned `Settings` model in `@umbra/settings` behind a `SettingsBackend`; web backend
   `localStorage`, desktop backend a JSON file in the app config directory owned by the Rust side and
   reached over IPC (chosen).** The desktop backend is loaded once before the first render, then kept in
   memory and written through.
2. `localStorage` everywhere, with a fixed gateway port. Cons: port collisions; the Rust side still
   cannot read the engine settings before startup.
3. Settings on the engine. Cons: the engine has no such endpoint; it would be a GUI concern inside the
   engine (D-1).
4. Tauri `plugin-store` from the frontend. Cons: a second persistence path for the web build anyway;
   the engine settings still need Rust-side reading.

## Decision
Option 1.
- `Settings` v1 (ASM-829): `{ version: 1, stream: { maxRateHz }, map: { affiliationOverrides:
  Record<string, Affiliation> }, journal: { capacity }, scenarios: { recent: string[] } }`, plus
  `engine: { mode: "managed" | "remote", remoteUrl, scenarioRoot, pluginPath }`, which only the desktop
  app uses. `parseSettings` validates, drops unknown keys, clamps ranges and fills defaults, so bad
  stored data never breaks startup. A future v2 migrates from v1 in the same function.
- The theme stays in `ThemeProvider` under `umbra.theme` (ASM-301). In the desktop app,
  `ThemeProvider.storage` is a `Storage`-like adapter over the settings backend, so the theme persists
  there too.
- Public API: `SettingsProvider`, `useSettings`, `useUpdateSettings`, `useAffiliationOf`,
  `defaultAffiliationOf`, `parseSettings`, `DEFAULT_SETTINGS`, `createLocalStorageBackend`, types
  `Settings`, `SettingsBackend`. The ASM-305 affiliation table moves here from `@umbra/map-view`, so
  map-view and entity-inspector share one implementation (DRY).
- The web build never lets the user change the engine URL: it comes from `/config.json` and is shown
  read-only, because any other origin would bypass the same-origin proxy and the server has no CORS
  (ASM-830).
- Providers: `RuntimeConfigProvider > SettingsProvider > ThemeProvider > DataProvider >
  EngineStreamProvider > NavigationProvider > RouterProvider`.

## Consequences
- Easier: one model, tested once; platform differences live in two small backends.
- Harder: startup in the desktop app waits for one IPC read (bounded, with defaults on failure).
- Revisit when settings need to sync between machines or per engine profile.
