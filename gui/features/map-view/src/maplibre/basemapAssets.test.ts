import { describe, expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { BASEMAP_ASSETS } from "./basemapAssets.ts";

const ASSETS = join(import.meta.dirname, "../../assets");
const BUDGET_BYTES = 1_500_000;

interface Fc {
  type: string;
  features: Array<{
    type: string;
    properties: unknown;
    geometry: { type: string; coordinates: unknown };
  }>;
}
const load = (name: string): Fc => JSON.parse(readFileSync(join(ASSETS, name), "utf8")) as Fc;

const walk = (c: unknown, visit: (lon: number, lat: number) => void): void => {
  if (!Array.isArray(c)) throw new Error("coordinates must be arrays");
  if (typeof c[0] === "number") visit(c[0], c[1] as number);
  else for (const x of c) walk(x, visit);
};

const EXPECTED: Record<string, string[]> = {
  "ne_50m_land.geojson": ["Polygon", "MultiPolygon"],
  "ne_50m_admin0_lines.geojson": ["LineString", "MultiLineString"],
};

describe("basemap assets", () => {
  test("exactly the two GeoJSON files plus the provenance README", () => {
    expect(readdirSync(ASSETS).sort()).toEqual([...Object.keys(EXPECTED), "README.md"].sort());
  });

  for (const [name, types] of Object.entries(EXPECTED)) {
    test(`${name}: FeatureCollection of ${types.join("/")} within the lon/lat range`, () => {
      const fc = load(name);
      expect(fc.type).toBe("FeatureCollection");
      expect(fc.features.length).toBeGreaterThan(50);
      let count = 0;
      for (const f of fc.features) {
        expect(f.type).toBe("Feature");
        expect(f.properties).toEqual({});
        expect(types).toContain(f.geometry.type);
        walk(f.geometry.coordinates, (lon, lat) => {
          count++;
          if (lon < -180 || lon > 180 || lat < -90 || lat > 90 || !Number.isFinite(lon + lat)) {
            throw new Error(`${name}: out of range ${lon},${lat}`);
          }
        });
      }
      expect(count).toBeGreaterThan(1000);
    });
  }

  test("polygon rings are closed with at least four positions", () => {
    for (const f of load("ne_50m_land.geojson").features) {
      const polys = (
        f.geometry.type === "Polygon" ? [f.geometry.coordinates] : f.geometry.coordinates
      ) as number[][][][];
      for (const poly of polys) {
        for (const ring of poly) {
          expect(ring.length).toBeGreaterThanOrEqual(4);
          expect(ring[0]).toEqual(ring[ring.length - 1]);
        }
      }
    }
  });

  test("total size is under 1.5 MB", () => {
    const total = Object.keys(EXPECTED).reduce((n, f) => n + statSync(join(ASSETS, f)).size, 0);
    expect(total).toBeLessThan(BUDGET_BYTES);
  });

  test("the provenance README names the source, version, licence, hashes and the command", () => {
    const readme = readFileSync(join(ASSETS, "README.md"), "utf8");
    for (const needle of [
      "naturalearthdata.com",
      "natural-earth-vector",
      "v5.1.2",
      "public domain",
      "SHA-256",
      "bun scripts/simplify-basemap.ts",
    ]) {
      expect(readme.toLowerCase()).toContain(needle.toLowerCase());
    }
  });

  test("the README 'Files here' table matches each file's byte count and SHA-256", () => {
    const readme = readFileSync(join(ASSETS, "README.md"), "utf8");
    const section = readme.slice(
      readme.indexOf("## Files here"),
      readme.indexOf("## Known limits"),
    );
    const rows = [
      ...section.matchAll(/^\| `([^`]+\.geojson)`[^|]*\| ([\d,]+) \| `([0-9a-f]{64})` \|$/gm),
    ];
    expect(rows.map((r) => r[1]).sort()).toEqual(Object.keys(EXPECTED).sort());
    for (const [, name, bytes, sha] of rows) {
      const data = readFileSync(join(ASSETS, name as string));
      expect(data.length).toBe(Number((bytes as string).replace(/,/g, "")));
      expect(createHash("sha256").update(data).digest("hex")).toBe(sha as string);
    }
  });

  test("BASEMAP_ASSETS points at the two files (the coast reuses the land source)", () => {
    for (const [key, name] of [
      ["land", "ne_50m_land.geojson"],
      ["borders", "ne_50m_admin0_lines.geojson"],
    ] as const) {
      const url = BASEMAP_ASSETS[key];
      expect(typeof url).toBe("string");
      const path =
        typeof url === "string" && url.startsWith("file:") ? fileURLToPath(url) : String(url);
      expect(dirname(path)).toBe(ASSETS);
      expect(path.endsWith(name)).toBe(true);
      expect(existsSync(path)).toBe(true);
    }
  });
});
