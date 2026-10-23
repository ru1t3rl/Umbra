# @umbra/settings

Foundation feature (tier `foundation`): versioned app settings (ADR 0074). Depends on `@umbra/ui-kit` for types, `THEME_STORAGE_KEY` and `parsePreference`.

## Public API
- `Settings` v1: `{ version: 1, appearance: { theme }, stream: { maxRateHz 1..60 }, map: { affiliationOverrides }, journal: { capacity 1,000..50,000, default 5,000 (= engine-stream)}, scenarios: { recent (max 10, unique) }, engine: { mode, remoteUrl, scenarioRoot, pluginPath } }`; `DEFAULT_SETTINGS`; `SettingsPatch` (per-section partial).
- `parseSettings(raw, warn = console.warn)`: never throws, idempotent, drops unknown keys, clamps, falls back per key; a `version` above 1 is read as v1 with one warning.
- `SettingsBackend { read(): unknown; write(settings): void }` (synchronous; `read` never throws). `createLocalStorageBackend(storage?)` uses the key `umbra.settings.v1`. The desktop backend (WI-22) satisfies the interface structurally.
- `useThemeStorage()`: a stable `Storage`-like view of `appearance.theme` (only `umbra.theme` maps) for `ThemeProvider.storage` on desktop; writes go through the provider, so there is one writer. The web app does not use it (the theme stays in localStorage `umbra.theme`, ASM-301).
- `SettingsProvider({ backend?, children })` reads once (a changed `backend` prop after mount is ignored; cross-tab is last writer wins); a file from a newer version is used but never written back; `useSettings()` (defaults outside a provider); `useUpdateSettings()` (stable; merges one level per section (an `affiliationOverrides` patch replaces the whole map), re-parses, skips unchanged results, one write per effective update; no-op outside a provider).
- `useRecordRecentScenario()`: a stable `(path) => void` that records a loaded scenario path as the newest recent one (once, capped at 10) and persists it; `withRecentPath(recent, path)` is the pure rule (ASM-975, WI-14 review).
- `useAffiliationOf()`: override (trimmed, case-insensitive) first, then `defaultAffiliationOf` (the ASM-305 table); stable while overrides are unchanged.
- Setting editors (M8 WI-17, ASM-995), all without props and laid out by `@umbra/shell-settings`; each writes through `useUpdateSettings`:
  - `StreamRateSetting`: Slider "Stream rate" (1..60 Hz). It only edits; the page that mounts the stream consumer sends the subscribe (`useApplySubscription`).
  - `JournalCapacitySetting`: number field "Journal capacity" (1,000..50,000), committed on blur or Enter; an invalid value is flagged and not saved. The change applies after a reload (the connection reads the capacity once).
  - `AffiliationOverridesEditor`: Table of force/affiliation rows with Add and Remove. The stored map is exactly the valid rows; empty, duplicate and reserved names (`__proto__`, `constructor`, `prototype`) are flagged and add nothing.
  - `ThemePreferenceSetting`: Select Dark/Light/System through `useTheme`; it writes nothing to the settings backend (the ThemeProvider is the one writer).
  - `RecentScenariosSetting`: the count of recent scenarios and a button that clears them.

```tsx
<SettingsProvider><ThemeProvider>...</ThemeProvider></SettingsProvider>
```
