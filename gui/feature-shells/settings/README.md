# @umbra/shell-settings

Shell (page composition, no logic): the settings page at `/settings` (M8 WI-17).

- `route` = `{ path: "/settings", label: "Settings", icon: "settings", nav: true, lazy }`; the app registers it in `src/router.tsx` (ASM-312), between Plugins and the showcase. It appears in the page navigation (`AppNav`).
- Layout: a docked `CommandBar` (`AppNav`, `ThemeToggle`; no visible title, the current navigation link names the page; the `h1` and an `h2` "App settings" are visually hidden), then seven Cards in one column: Engine connection, Appearance, Live data, Map, Log, Scenarios, About. Each Card lays out one editor and holds no logic:
  - `EngineConnectionSettings` of `@umbra/desktop` (a read-only engine URL stub until WI-22);
  - `ThemePreferenceSetting` and a second `ThemeToggle` named "Use the Day theme" (the bar's stays "Day theme");
  - `StreamRateSetting`; the page calls `useApplySubscription` with the stored rate, so a change sends a subscribe at once;
  - `AffiliationOverridesEditor`, `JournalCapacitySetting`, `RecentScenariosSetting` of `@umbra/settings`;
  - About (`components/AboutSection`): `api_version` of `GET /status` ("Unavailable" when it fails), the app version and a link to `/showcase`.
- The page is not gated on the run state: settings are app-level, so the idle state shows every section.
- App version: `version` of `gui/package.json`, injected by `vite.config.ts` as `import.meta.env.VITE_UMBRA_APP_VERSION` (ASM-996); `src/appVersion.ts` falls back to `0.0.0` outside a Vite build (unit tests).
- Depends on `@umbra/ui-kit`, `@umbra/settings`, `@umbra/desktop`, `@umbra/data` (status) and `@umbra/engine-stream` (subscription); `@umbra/runtime-config` is a devDependency of the tests (`src/testing.tsx`).
- Screenshot and axe target of `e2e/settings.spec.ts` (Ops and Day baselines `settings-*` under `e2e/__screenshots__/linux/`). The lazy chunk has a size budget in `src/e2eHook.build.test.ts` (ASM-996).
