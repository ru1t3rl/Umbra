# @umbra/shell-plugins

Shell (page composition, no logic): the plugin status page at `/plugins` (M8 WI-16).

- `route` = `{ path: "/plugins", label: "Plugins", icon: "extension", nav: true, lazy }`; the app registers it in `src/router.tsx` (ASM-312). It appears in the page navigation (`AppNav`).
- Layout: a docked `CommandBar` (`AppNav`, `ThemeToggle`), then `FaultSummary` above ui-kit `Tabs` ("Plugins", "Network adapters", "Components"). `useState` holds the open tab (layout state: it lives in the page, so it is kept across idle and the next scenario); only the open tab is mounted, so components are requested when their tab opens. The bar has no visible title (the current navigation link names the page; the `h1` is visually hidden).
- While no scenario is loaded, `PluginStatusGate` of `@umbra/plugin-status` renders the idle state once for the whole body (ASM-994); the shell reads no run state itself.
- Depends on `@umbra/ui-kit` and `@umbra/plugin-status` only; `@umbra/data` and `@umbra/engine-stream` are devDependencies of the tests (`src/testing.tsx`).
- Screenshot and axe target of `e2e/plugins.spec.ts` (Ops and Day baselines under `e2e/__screenshots__/linux/`). The lazy chunk has a size budget in `src/e2eHook.build.test.ts` (ASM-994).
