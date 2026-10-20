# ADR 0040: Map library: MapLibre GL JS behind `MapView`

- **Status**: Accepted by the human 2026-10-20 (proposed by frontend-architect, M5b plan `docs/plans/M5b.md`, 2026-10-20)
- **Requirement IDs**: D-13, D-36, F-4, F-8, REQ-G-6, REQ-A-4, REQ-K-5, NFR-1 (10,000 entities), spec section 19 question 6, A-002 (M5b deliverable 5), A-003
- **Affected**: `gui/features/map-view` (`@umbra/map-view`), `gui/package.json`, `bun.lock`, decision row D-56

## Context
Spec open question 6 (moved to M5b by A-002): MapLibre (2D) or Cesium (3D). F-8 hides the library behind
a `MapView` interface in `@umbra/map-view` so the choice stays cheap to change. Forces:
- 10,000 live entities at up to 30 frames per second without a React render per entity (`gui/CLAUDE.md`).
- The Windows app must work offline (A-003 deviation 1); tests use no remote tiles (A-002).
- The style must derive from the Vantage tokens, darkened in Ops (`tactical-display.md`).
- Headless Playwright must render it (software WebGL).
- Licence must be permissive (NFR-4; parity with `deny.toml`).

## Options
1. **MapLibre GL JS 5.x (chosen).** BSD-3-Clause, no access token, WebGL vector renderer, GeoJSON source
   tiled in a web worker, data-driven paint, `addImage` for runtime-drawn symbols, globe projection
   available for later. Cons: text labels need glyph PBFs (offline: bundle them later, see ADR 0041);
   `setData` of 10,000 points re-tiles in the worker each frame (cost measured in WI-12).
2. **CesiumJS (3D).** Apache-2.0, 3D globe and terrain. Cons: several MB of assets, the default imagery
   and terrain need a Cesium ion token (network), heavier headless rendering, the GUI does not need 3D yet.
3. **OpenLayers.** BSD-2-Clause, canvas text with CSS fonts (no glyph server), WebGL point layer. Cons:
   not the A-002 proposal; weaker GPU styling and no globe; its strengths are raster/GIS editing.
4. **deck.gl on MapLibre.** MIT, fastest for very large point sets. Cons: a second large library for a
   problem MapLibre handles at 10,000 points; add only if WI-12's measurement says so.
5. **Leaflet.** BSD-2-Clause, DOM or canvas markers. Cons: DOM markers do not scale to 10,000 live updates.

## Decision
Option 1, `maplibre-gl` 5.x (exact version pinned in `bun.lock`, recorded in ADR 0043). Only
`@umbra/map-view` imports it (dependency-cruiser rule `maplibre-only-in-map-view`). The internal
`MapView` interface (`setEntities`, `setTheme`, `fitTo`, `inspect`, `destroy`) is the seam; the public
API of the package is the `LiveMap` component, so shells never see MapLibre types. The entity layer is
imperative: one GeoJSON source updated at most once per animation frame (ADR 0045). If WI-12's
10,000-entity measurement misses the frame budget, the fallback is a custom layer or deck.gl behind the
same interface, by a new ADR.

## Consequences
- Easier: offline by default, no token, permissive licence, swappable implementation.
- Harder: map text needs bundled glyphs (deferred, ADR 0041); headless WebGL needs SwiftShader flags in
  the Playwright config (risk R-3 in the plan).
- Revisit when 3D terrain or a globe view becomes a requirement (Cesium becomes a second `MapView`
  implementation, not a replacement), or when the 10,000-entity measurement fails.
