# @umbra/shell-dashboard

Shell (page composition, no logic): the live map page at `/` (M5b, map only; M8 adds controls and the inspector).

- `route` = `{ path: "/", label: "Live map", icon: "map", nav: true, lazy }`; the app registers it in `src/router.tsx`. `DashboardPage` is the default export of `src/components/DashboardPage/DashboardPage.tsx` and is code-split (it carries MapLibre).
- Composes `LiveMap` (`@umbra/map-view`) with `useEntitySource()` and, when set, `basemap.styleUrl` from `useRuntimeConfig()`; the floating `CommandBar` with `RunStatusBar` and `ThemeToggle` (`@umbra/scenario-control`, `@umbra/ui-kit`); `ConnectionAlert` under the bar.
- Landmarks: the bar sits directly in the body so it stays the page `banner`; `<main>` holds a visually hidden `h1`, the map region and the connection alert.
- Insets: `useBarInset` measures the rendered bar (margins plus the ui-kit `--vt-cmdbar-height`) and passes `insets={{ top }}` to `LiveMap` and `--dashboard-inset-top` to the alert, so the fitted entities and the banners stay clear of the bar (ASM-508).
