# @umbra/map-view

Domain feature (tier `domain`): the live map. MapLibre GL JS 5.9.0 behind an internal `MapView` seam (ADR 0040, F-8); only this package imports `maplibre-gl`.

## Public API
- `LiveMap({ source, label?, basemapStyleUrl? })` and `LiveMapProps`.
  - `source` is an `EntitySource` (`useEntitySource()` of `@umbra/engine-stream`, passed by the shell). Frames go from the source straight to the map; React re-renders only when the entity count or the error state changes.
  - The region is named `label` (default "Live map"); a polite live region announces the entity count when it changes.
  - The camera fits the first non-empty frame of each session (padding `space-16`, max zoom 15) and never moves by itself afterwards (ASM-307).
  - The theme follows `useTheme().resolved`: paint properties and symbol images are updated in place, no reload.
  - Three error states in an `Alert` (details are logged): unreadable token colours (retried on the next theme change), no WebGL on the device, and a lost WebGL context (cleared when it is restored).
  - The entity count is announced at most every 2 s.
  - `basemapStyleUrl` replaces the offline basemap (`/config.json` `basemap.styleUrl`, ADR 0041).
  - `insets` (`MapInsets`: `{ top?, right?, bottom?, left? }`, px) is the screen space a shell covers with floating UI. The camera fit padding is `space-16` plus the inset on each side, so fitted entities never sit under the overlay, and the error banner is placed below the top inset and centred in the free width. Missing, negative or non-finite sides count as 0. The latest value is read when the camera fits; changing it never remounts the map (ASM-508).
- `@umbra/map-view/styles.css`: MapLibre CSS and the container rules; imported once by the app (ADR 0043).

```tsx
// src/main.tsx
import "@umbra/map-view/styles.css";
// a shell
const source = useEntitySource();
<LiveMap source={source} />
// with a floating bar of 80px (margins plus height): <LiveMap source={source} insets={{ top: 80 }} />
```

## Basemap
- Offline by default: background, Natural Earth 1:50m land (its outline is the coast) and land boundary lines, and a graticule, all coloured from tokens. The two GeoJSON files (about 1.16 MB in total) are in `assets/`, with their provenance in `assets/README.md`; `src/maplibre/basemapAssets.ts` references them with `new URL(..., import.meta.url)` so Vite bundles them as hashed assets that MapLibre fetches from the app's own origin.
- Regenerate with `bun scripts/simplify-basemap.ts` (in `gui/`, deterministic; command in `assets/README.md`).

## Internals
- `src/model/` is pure and tested without WebGL: `affiliationOf`, `resolveMapColors`, `symbolSvg` (APP-6D frames, remote ring), `buildMapStyle`, `toFeatureCollection`, `fitOptionsFrom`.
- `src/maplibre/createMapLibreView.ts` implements `MapView`: one `setData` per animation frame from a reused buffer. Never call `getData()` or `updateData` on the entity source (ASM-366).
- `src/e2eHook.ts` registers `window.__UMBRA_E2E__.map` only when `import.meta.env.VITE_UMBRA_E2E` is set (the e2e build); `gui/src/e2eHook.build.test.ts` builds the real app and proves the production bundle has no trace of it.
- `bun scripts/measure-map.ts [frames]` (in `gui/`, not a gate) times `setData`-to-idle for 10,000 entities in headless Chromium.
