import { describe, expect, test } from "bun:test";
import { affiliationOf } from "./affiliation.ts";
import { frameBounds, toFeatureCollection } from "./features.ts";
import { entity, frameOf } from "./testing.ts";

describe("toFeatureCollection", () => {
  test("ids, [lon, lat] coordinates and the four properties", () => {
    const fc = toFeatureCollection(
      frameOf([
        entity(7, {
          lon_deg: 4.5,
          lat_deg: 52.1,
          heading_deg: 270,
          speed_mps: 3,
          force: "red",
          ownership: "remote",
        }),
        entity(9, { speed_mps: 0, force: null, ownership: "unknown" }),
      ]),
      affiliationOf,
    );
    expect(fc.type).toBe("FeatureCollection");
    expect(fc.features).toHaveLength(2);
    expect(fc.features[0]).toEqual({
      type: "Feature",
      id: 7,
      geometry: { type: "Point", coordinates: [4.5, 52.1] },
      properties: { affiliation: "hostile", ownership: "remote", heading_deg: 270, moving: true },
    });
    expect(fc.features[1]?.id).toBe(9);
    expect(fc.features[1]?.properties).toEqual({
      affiliation: "unknown",
      ownership: "unknown",
      heading_deg: 90,
      moving: false,
    });
  });

  test("moving is speed above 0.1 m/s, exclusive", () => {
    const moving = (s: number) =>
      toFeatureCollection(frameOf([entity(1, { speed_mps: s })]), affiliationOf).features[0]
        ?.properties.moving;
    expect(moving(0.1)).toBe(false);
    expect(moving(0.11)).toBe(true);
  });

  test("10,000 entities produce one collection of 10,000 features", () => {
    const entities = Array.from({ length: 10_000 }, (_, i) => entity(i));
    const fc = toFeatureCollection(frameOf(entities), affiliationOf);
    expect(fc.features).toHaveLength(10_000);
    expect(new Set(fc.features.map((f) => f.id)).size).toBe(10_000);
  });

  test("a reused buffer is rewritten in place, grows and shrinks, and matches a fresh build", () => {
    const first = toFeatureCollection(
      frameOf([
        entity(1, { heading_deg: 10, speed_mps: 0, ownership: "owned", force: "blue" }),
        entity(2),
      ]),
      affiliationOf,
    );
    const f0 = first.features[0];
    const next = frameOf([
      entity(5, {
        lon_deg: 1,
        lat_deg: 2,
        force: "green",
        heading_deg: 270,
        speed_mps: 0.5,
        ownership: "remote",
      }),
      entity(6),
      entity(7),
    ]);
    const again = toFeatureCollection(next, affiliationOf, first);
    expect(again).toBe(first);
    expect(again.features[0]).toBe(f0);
    expect(f0?.properties).toEqual({
      affiliation: "neutral",
      ownership: "remote",
      heading_deg: 270,
      moving: true,
    });
    expect(again).toEqual(toFeatureCollection(next, affiliationOf));
    // moving flips back across the threshold, and the heading and ownership follow
    const calm = frameOf([
      entity(5, { heading_deg: 45, speed_mps: 0.05, ownership: "unknown", force: null }),
    ]);
    const shrunk = toFeatureCollection(calm, affiliationOf, first);
    expect(shrunk.features).toHaveLength(1);
    expect(shrunk).toEqual(toFeatureCollection(calm, affiliationOf));
    expect(shrunk.features[0]?.properties).toEqual({
      affiliation: "unknown",
      ownership: "unknown",
      heading_deg: 45,
      moving: false,
    });
  });
});

describe("frameBounds", () => {
  test("covers all entities; an empty frame has none", () => {
    expect(
      frameBounds(
        frameOf([entity(1, { lon_deg: 5, lat_deg: 50 }), entity(2, { lon_deg: 7, lat_deg: 48 })]),
      ),
    ).toEqual({ west: 5, south: 48, east: 7, north: 50 });
    expect(frameBounds(frameOf([]))).toBeNull();
  });
});
