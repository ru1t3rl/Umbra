import { describe, expect, test } from "bun:test";
import type { LayerSpecification } from "maplibre-gl";
import { buildMapStyle, type GeoJsonData, graticule, paintUpdates } from "./style.ts";
import { COLORS } from "./testing.ts";

const inline = (theme: "dark" | "light", basemap = {}) => {
  const built = buildMapStyle(COLORS, theme, basemap);
  if (built.kind !== "inline") throw new Error("expected an inline style");
  return built.style;
};
const layer = (layers: LayerSpecification[], id: string): LayerSpecification => {
  const l = layers.find((x) => x.id === id);
  if (!l) throw new Error(`no layer ${id}`);
  return l;
};

describe("buildMapStyle", () => {
  test("layer order: background, land, coast, borders, graticule, direction, entities (direction below the symbols)", () => {
    expect(inline("dark").layers.map((l) => l.id)).toEqual([
      "background",
      "land",
      "coast",
      "borders",
      "graticule",
      "direction",
      "entities",
    ]);
  });

  test("colours come from the input: surface-0 background, surface-1 land, line coast", () => {
    const { layers } = inline("dark");
    expect(layer(layers, "background")).toMatchObject({ paint: { "background-color": "S0" } });
    expect(layer(layers, "land")).toMatchObject({ paint: { "fill-color": "S1" } });
    expect(layer(layers, "coast")).toMatchObject({ paint: { "line-color": "LINE" } });
  });

  test("Ops graticule is line at full opacity; Day is line-strong at opacity-track", () => {
    expect(layer(inline("dark").layers, "graticule")).toMatchObject({
      paint: { "line-color": "LINE", "line-opacity": 1 },
    });
    expect(layer(inline("light").layers, "graticule")).toMatchObject({
      paint: { "line-color": "LINE-STRONG", "line-opacity": 0.55 },
    });
  });

  test("no glyphs key (ASM-314) and no text layout", () => {
    for (const theme of ["dark", "light"] as const) {
      const style = inline(theme);
      expect("glyphs" in style).toBe(false);
      expect(JSON.stringify(style)).not.toContain("text-");
    }
    const url = buildMapStyle(COLORS, "dark", { styleUrl: "https://tiles.test/style.json" });
    expect(JSON.stringify(url)).not.toContain("glyphs");
  });

  test("a styleUrl replaces the base and keeps only the entity layers as the overlay", () => {
    const built = buildMapStyle(COLORS, "dark", { styleUrl: "https://tiles.test/style.json" });
    if (built.kind !== "url") throw new Error("expected a url style");
    expect(built.styleUrl).toBe("https://tiles.test/style.json");
    expect(built.overlay.layers.map((l) => l.id)).toEqual(["direction", "entities"]);
    expect(Object.keys(built.overlay.sources)).toEqual(["entities"]);
  });

  test("an empty styleUrl is the offline base", () => {
    expect(buildMapStyle(COLORS, "dark", { styleUrl: "" }).kind).toBe("inline");
  });

  test("coast and borders are line layers in the line colour; the Day theme keeps them", () => {
    for (const theme of ["dark", "light"] as const) {
      const { layers } = inline(theme);
      for (const id of ["coast", "borders"]) {
        expect(layer(layers, id)).toMatchObject({
          type: "line",
          source: id,
          paint: { "line-color": "LINE" },
        });
      }
    }
  });

  test("the entities layer picks the plain image only for owned (exact expression)", () => {
    const entities = layer(inline("dark").layers, "entities");
    if (entities.type !== "symbol") throw new Error("symbol layer");
    expect(entities.layout?.["icon-image"]).toEqual([
      "case",
      ["==", ["get", "ownership"], "owned"],
      ["concat", "umbra-symbol-", ["get", "affiliation"], "-plain"],
      ["concat", "umbra-symbol-", ["get", "affiliation"], "-ring"],
    ]);
  });

  test("direction: only when moving, rotated by heading about the map; sizes 20/24/40px by zoom", () => {
    const { layers } = inline("dark");
    const direction = layer(layers, "direction");
    const entities = layer(layers, "entities");
    if (entities.type !== "symbol" || direction.type !== "symbol") throw new Error("symbol layers");
    expect(direction.filter).toEqual(["get", "moving"]);
    expect(direction.layout?.["icon-rotate"]).toEqual(["get", "heading_deg"]);
    expect(direction.layout?.["icon-rotation-alignment"]).toBe("map");
    const size: unknown = ["interpolate", ["linear"], ["zoom"], 4, 20 / 24, 12, 1, 18, 40 / 24];
    expect(JSON.stringify(entities.layout?.["icon-size"])).toBe(JSON.stringify(size));
    expect(JSON.stringify(direction.layout?.["icon-size"])).toBe(JSON.stringify(size));
  });

  test("basemap geometry is passed through to land, coast and borders", () => {
    const mk = (lon: number): GeoJsonData => ({
      type: "FeatureCollection",
      features: [
        { type: "Feature", properties: null, geometry: { type: "Point", coordinates: [lon, 0] } },
      ],
    });
    const [land, coast, borders] = [mk(1), mk(2), mk(3)];
    const { sources } = inline("dark", { data: { land, coast, borders } });
    expect(sources.land).toEqual({ type: "geojson", data: land });
    expect(sources.coast).toEqual({ type: "geojson", data: coast });
    expect(sources.borders).toEqual({ type: "geojson", data: borders });
    expect(sources.land).not.toBe(sources.coast);
  });

  test("the graticule does not repeat the antimeridian", () => {
    const meridians = graticule().features.flatMap((f) =>
      f.geometry.type === "LineString" &&
      f.geometry.coordinates[0]?.[0] === f.geometry.coordinates[1]?.[0]
        ? [f.geometry.coordinates[0]?.[0]]
        : [],
    );
    expect(new Set(meridians).size).toBe(meridians.length);
    expect(meridians).not.toContain(180);
    expect(meridians).toContain(-180);
  });

  test("paintUpdates matches the paint buildMapStyle emits, for both themes", () => {
    for (const theme of ["dark", "light"] as const) {
      const { layers } = inline(theme);
      for (const { layer, property, value } of paintUpdates(COLORS, theme)) {
        expect(layer_paint(layers, layer)[property]).toBe(value);
      }
    }
  });
});

const layer_paint = (layers: LayerSpecification[], id: string): Record<string, unknown> => {
  const l = layers.find((x) => x.id === id);
  return l && "paint" in l && l.paint ? { ...l.paint } : {};
};
