import type { BasemapData } from "../model/style.ts";

/**
 * The bundled Natural Earth basemap (WI-13, ADR 0041, H-2). Each `new URL(literal, import.meta.url)` is
 * recognised by Vite, which emits the file as a hashed asset and rewrites the URL; the source URL is the
 * file under `assets/`. Provenance: `assets/README.md`. MapLibre fetches them from the app's own origin.
 */
export const BASEMAP_ASSETS: BasemapData = {
  land: new URL("../../assets/ne_50m_land.geojson", import.meta.url).href,
  borders: new URL("../../assets/ne_50m_admin0_lines.geojson", import.meta.url).href,
};
