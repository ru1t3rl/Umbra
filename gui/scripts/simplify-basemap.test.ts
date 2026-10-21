import { describe, expect, test } from "bun:test";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  douglasPeucker,
  PRECISION,
  serialise,
  simplifyCollection,
  simplifyRing,
  TOLERANCE_DEG,
} from "./simplify-basemap.ts";

const fixture = {
  type: "FeatureCollection",
  crs: { type: "name" },
  features: [
    {
      type: "Feature",
      properties: { name: "b", scalerank: 1 },
      geometry: {
        type: "LineString",
        coordinates: [
          [10.123456, 20.987654],
          [11, 20.9876],
          [12.0001, 20.98],
          [13, 25],
          [14.5, 25.00001],
        ],
      },
    },
    {
      type: "Feature",
      properties: { name: "a" },
      geometry: {
        type: "Polygon",
        coordinates: [
          [
            [0, 0],
            [1, 0.001],
            [2, 0],
            [2, 2],
            [0, 2],
            [0, 0],
          ],
          [
            [0.5, 0.5],
            [0.5000001, 0.5],
            [0.5, 0.5000001],
            [0.5, 0.5],
          ],
        ],
      },
    },
    {
      type: "Feature",
      properties: {},
      geometry: {
        type: "Polygon",
        coordinates: [
          [
            [5, 5],
            [5.001, 5],
            [5, 5],
          ],
        ],
      },
    },
    { type: "Feature", properties: {}, geometry: null },
  ],
};

describe("simplify-basemap", () => {
  test("deterministic: the same input gives byte-identical output, whatever the feature order", () => {
    const a = serialise(simplifyCollection(fixture));
    const b = serialise(simplifyCollection(structuredClone(fixture)));
    const reversed = serialise(
      simplifyCollection({ ...fixture, features: [...fixture.features].reverse() }),
    );
    expect(a).toBe(b);
    expect(a).toBe(reversed);
  });

  test("drops properties and degenerate geometry, rounds, removes collapsed holes", () => {
    const out = simplifyCollection(fixture);
    expect(out.features).toHaveLength(2);
    for (const f of out.features) expect(f.properties).toEqual({});
    const poly = out.features.find((f) => f.geometry.type === "Polygon");
    expect(poly?.geometry.coordinates as unknown[]).toHaveLength(1);
    const line = out.features.find((f) => f.geometry.type === "LineString");
    const coords = line?.geometry.coordinates as number[][] | undefined;
    expect(coords?.[0]).toEqual([10.123, 20.988]);
  });

  test("Douglas-Peucker keeps the ends and drops points within tolerance", () => {
    const line: Array<[number, number]> = [
      [0, 0],
      [1, 0.01],
      [2, 0],
      [3, 3],
    ];
    expect(douglasPeucker(line, 0.05)).toEqual([
      [0, 0],
      [2, 0],
      [3, 3],
    ]);
    expect(douglasPeucker(line, 0)).toEqual(line);
  });

  test("vertices on the antimeridian or at the Antarctic edge are never dropped", () => {
    // Each middle vertex is within the tolerance of the chord, so only the must-keep rule retains it.
    const east: Array<[number, number]> = [
      [179.5, 0],
      [180, 1],
      [179.5, 2],
    ];
    expect(douglasPeucker(east, 1)).toEqual(east);
    const west: Array<[number, number]> = [
      [-179.5, 0],
      [-180, 1],
      [-179.5, 2],
    ];
    expect(douglasPeucker(west, 1)).toEqual(west);
    const south: Array<[number, number]> = [
      [0, -80],
      [1, -89.95],
      [2, -80],
    ];
    expect(douglasPeucker(south, 20)).toEqual(south);
    const inner: Array<[number, number]> = [
      [0, 0],
      [0.5, 0.2],
      [1, 0],
    ];
    expect(douglasPeucker(inner, 1)).toEqual([
      [0, 0],
      [1, 0],
    ]);
    const ring = simplifyRing(
      [
        [170, 60],
        [180, 65],
        [170, 70],
        [170, 60],
      ],
      5,
      PRECISION,
    );
    expect(ring).toContainEqual([180, 65]);
  });

  test("a ring stays closed", () => {
    const ring = simplifyRing(
      [
        [0, 0],
        [4, 0],
        [4, 4],
        [0, 4],
        [0, 0],
      ],
      TOLERANCE_DEG,
      PRECISION,
    );
    expect(ring?.[0]).toEqual(ring?.[ring.length - 1]);
    expect(ring).toHaveLength(5);
  });

  test("the CLI writes the same bytes twice", () => {
    const dir = mkdtempSync(join(tmpdir(), "simplify-"));
    const src = join(dir, "ne_50m_land.geojson");
    writeFileSync(src, JSON.stringify(fixture));
    const run = (out: string) => {
      const r = Bun.spawnSync(["bun", join(import.meta.dirname, "simplify-basemap.ts"), out, src]);
      expect(r.exitCode).toBe(0);
      return readFileSync(join(out, "ne_50m_land.geojson"), "utf8");
    };
    expect(run(join(dir, "o1"))).toBe(run(join(dir, "o2")));
  });

  test("rejects non-FeatureCollection input", () => {
    expect(() => simplifyCollection({ type: "Feature" })).toThrow();
  });
});
