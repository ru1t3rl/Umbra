# ADR 0041: Offline basemap and a map style derived from the tokens

- **Status**: Accepted by the human 2026-10-20 (proposed by frontend-architect, M5b plan `docs/plans/M5b.md`, 2026-10-20)
- **Requirement IDs**: A-002 (M5b deliverable 5: tests use no remote tiles, online basemap a runtime option), A-003 (deviation 1, offline app; `tactical-display.md`), REQ-G-1, F-8
- **Affected**: `@umbra/map-view`, `gui/public/config.json`, `src/config.ts`, decision row D-57

## Context
The map is `surface-0`; base-map content is darkened and desaturated in Ops so symbols carry the colour
(`tactical-display.md`). The desktop app must work offline, tests must not touch the network, and A-002
wants an online basemap as a runtime option. No colour literal may appear outside the tokens
(`check:tokens`). MapLibre styles take resolved colour strings, not `var(--x)`. MapLibre text needs
glyph PBFs from a `glyphs` URL, which is network unless bundled.

## Options
1. **Blank style: background + graticule only.** Zero assets, zero network. Cons: no geography at all.
2. **Offline style + bundled Natural Earth vectors (chosen, if the human accepts the asset).** Background
   (`surface-0`), land (`surface-1`), coastline and borders (`line`), lat/long graticule (`line` in Ops,
   `line-strong` at `opacity-track` in Day). Natural Earth 1:50m land, coastline and admin-0 boundary
   lines (public domain), simplified and committed as GeoJSON under `features/map-view/assets/` with a
   provenance README. Size target under 1.5 MB. Cons: coarse at zoom 12+ (a coastline is off by
   hundreds of metres); an asset to keep.
3. **Online tiles by default** (OpenFreeMap, MapTiler, OSM raster). Cons: network at runtime and in tests,
   usage policies or keys, cannot be recoloured from tokens reliably.

## Decision
Option 2 as the default style, with option 1 as the automatic fallback if the human declines the asset
(WI-13 is then dropped; nothing else changes).
- The style is built at runtime by a pure function `buildMapStyle(colors)` from colours resolved with
  `getComputedStyle(document.documentElement)` on the token custom properties, so no literal enters the
  code and a theme switch rebuilds the paint properties (`setPaintProperty`, no map reload).
- No `glyphs` and no symbol text layers in M5b; entity names are not drawn on the canvas (M8 adds the
  label pill as an HTML overlay or bundled Archivo glyph PBFs by a new ADR).
- **Online basemap (runtime option, off by default).** `/config.json` may carry
  `"basemap": { "styleUrl": "<MapLibre style URL>" }`. When set, that style replaces the offline one and
  the entity layers are added on top; Ops darkening of a third-party style is the operator's
  responsibility (documented). No test loads it; a unit test checks the option is passed through.
- No attribution control for the offline style (public domain); a configured online style shows the
  attribution its style declares.

## Consequences
- Easier: works offline, deterministic tests, theme switch without reload, no new token.
- Harder: coarse coastlines at tactical zooms; real tactical basemaps (MBTiles/PMTiles, terrain) are a
  later item through the same `styleUrl` option or a bundled-tiles ADR.
- Revisit when an operator needs street-level context offline (bundle PMTiles) or labels on the map.
