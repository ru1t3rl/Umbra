import type { ResolvedTheme } from "@umbra/ui-kit";
import type {
  GeoJSONSourceSpecification,
  LayerSpecification,
  StyleSpecification,
} from "maplibre-gl";
import type { MapColors } from "./colors.ts";
import { emptyFeatureCollection } from "./features.ts";
import { DIRECTION_IMAGE_ID, SYMBOL_FRAME_PX } from "./symbols.ts";

/**
 * The map style as plain data (ADR 0041): built from resolved token colours, no `glyphs` key (ASM-314).
 * Types come from `maplibre-gl` as type-only imports, so the model stays free of WebGL.
 */

type SymbolLayer = Extract<LayerSpecification, { type: "symbol" }>;
export type GeoJsonData = Exclude<GeoJSONSourceSpecification["data"], string>;
type GeoJsonCollection = Extract<GeoJsonData, { type: "FeatureCollection" }>;
type IconSize = NonNullable<NonNullable<SymbolLayer["layout"]>["icon-size"]>;
type IconImage = NonNullable<NonNullable<SymbolLayer["layout"]>["icon-image"]>;

export interface StyleOverlay {
  sources: Record<string, GeoJSONSourceSpecification>;
  layers: LayerSpecification[];
}

/** The offline basemap geometry (Natural Earth, WI-13). Omitted pieces are empty collections. */
export interface BasemapData {
  readonly land?: GeoJsonData;
  readonly coast?: GeoJsonData;
  readonly borders?: GeoJsonData;
}

export interface BasemapOptions {
  /** Runtime option (ADR 0041): when set, this style replaces the offline base. */
  readonly styleUrl?: string | undefined;
  readonly data?: BasemapData | undefined;
}

export type BuiltMapStyle =
  /** Offline base plus the entity layers, one style object. */
  | { kind: "inline"; style: StyleSpecification }
  /** A configured style replaces the base; the caller loads `styleUrl`, then adds `overlay` on top. */
  | { kind: "url"; styleUrl: string; overlay: StyleOverlay };

export const ENTITY_SOURCE = "entities";
export const ENTITY_LAYER = "entities";
export const DIRECTION_LAYER = "direction";

/** The icon-image expression: the plain image only for `owned`; `remote` and `unknown` get the ring (ASM-362). */
export function symbolImageExpression(): IconImage {
  return [
    "case",
    ["==", ["get", "ownership"], "owned"],
    ["concat", "symbol-", ["get", "affiliation"], "-plain"],
    ["concat", "symbol-", ["get", "affiliation"], "-ring"],
  ];
}

// Symbol size: 24px at zoom 12, 20px at zoom 4, 40px at zoom 18 (ADR 0042); the image frame is 24px wide.
const iconSize = (): IconSize => [
  "interpolate",
  ["linear"],
  ["zoom"],
  4,
  20 / SYMBOL_FRAME_PX,
  12,
  24 / SYMBOL_FRAME_PX,
  18,
  40 / SYMBOL_FRAME_PX,
];

/** The two entity layers: the direction line, then the symbols above it so the frame covers the line's inner part. */
export function entityOverlay(): StyleOverlay {
  const direction: SymbolLayer = {
    id: DIRECTION_LAYER,
    type: "symbol",
    source: ENTITY_SOURCE,
    filter: ["get", "moving"],
    layout: {
      "icon-image": DIRECTION_IMAGE_ID,
      "icon-size": iconSize(),
      "icon-rotate": ["get", "heading_deg"],
      "icon-rotation-alignment": "map",
      "icon-allow-overlap": true,
      "icon-ignore-placement": true,
    },
  };
  const entities: SymbolLayer = {
    id: ENTITY_LAYER,
    type: "symbol",
    source: ENTITY_SOURCE,
    layout: {
      "icon-image": symbolImageExpression(),
      "icon-size": iconSize(),
      "icon-allow-overlap": true,
      "icon-ignore-placement": true,
    },
  };
  return {
    sources: { [ENTITY_SOURCE]: { type: "geojson", data: emptyFeatureCollection() } },
    layers: [direction, entities],
  };
}

const emptyCollection = (): GeoJsonCollection => ({ type: "FeatureCollection", features: [] });

/** Lat/long lines every `stepDeg` degrees (lon -180 inclusive, 180 not repeated). Pure, so no asset is needed. */
export function graticule(stepDeg = 10): GeoJsonCollection {
  const out = emptyCollection();
  for (let lon = -180; lon < 180; lon += stepDeg) {
    out.features.push({
      type: "Feature",
      properties: null,
      geometry: {
        type: "LineString",
        coordinates: [
          [lon, -90],
          [lon, 90],
        ],
      },
    });
  }
  for (let lat = -80; lat <= 80; lat += stepDeg) {
    out.features.push({
      type: "Feature",
      properties: null,
      geometry: {
        type: "LineString",
        coordinates: [
          [-180, lat],
          [180, lat],
        ],
      },
    });
  }
  return out;
}

/**
 * Bottom to top: background, land, coast, borders, graticule, direction, entities. `light` is the Day theme
 * (graticule `line-strong` at `opacity-track`), `dark` is Ops (graticule `line`). With `basemap.styleUrl`
 * the configured style replaces the offline base and only the entity layers are returned as an overlay.
 */
export function buildMapStyle(
  colors: MapColors,
  theme: ResolvedTheme,
  basemap: BasemapOptions = {},
): BuiltMapStyle {
  const overlay = entityOverlay();
  if (basemap.styleUrl !== undefined && basemap.styleUrl !== "") {
    return { kind: "url", styleUrl: basemap.styleUrl, overlay };
  }
  const data = basemap.data ?? {};
  const day = theme === "light";
  const thin = (id: string, color: string, opacity: number): LayerSpecification => ({
    id,
    type: "line",
    source: id,
    paint: { "line-color": color, "line-width": 1, "line-opacity": opacity },
  });
  return {
    kind: "inline",
    style: {
      version: 8,
      sources: {
        land: { type: "geojson", data: data.land ?? emptyCollection() },
        coast: { type: "geojson", data: data.coast ?? emptyCollection() },
        borders: { type: "geojson", data: data.borders ?? emptyCollection() },
        graticule: { type: "geojson", data: graticule() },
        ...overlay.sources,
      },
      layers: [
        { id: "background", type: "background", paint: { "background-color": colors.surface0 } },
        { id: "land", type: "fill", source: "land", paint: { "fill-color": colors.surface1 } },
        thin("coast", colors.line, 1),
        thin("borders", colors.line, 1),
        day
          ? thin("graticule", colors.lineStrong, colors.opacityTrack)
          : thin("graticule", colors.line, 1),
        ...overlay.layers,
      ],
    },
  };
}
