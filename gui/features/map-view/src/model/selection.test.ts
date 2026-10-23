// WI-9 (ADR 0078, ASM-803, ASM-933): the pure selection model, written before model/selection.ts exists.
// Catches: a wrong hit box, a first-hit-wins pick, a selection source holding more than one feature,
// a linear scan of a 10,000-entity frame, a label that is not "name or #id".
import { describe, expect, test } from "bun:test";
import { defaultAffiliationOf } from "@umbra/settings";
import {
  findEntity,
  hitBox,
  pickNearest,
  SELECT_BOX_PX,
  selectionLabel,
  toSelectionCollection,
} from "./selection.ts";
import { entity, frameOf } from "./testing.ts";

describe("hitBox", () => {
  test("the click box is 12 px: 6 px each side of the point", () => {
    expect(SELECT_BOX_PX).toBe(12);
    expect(hitBox({ x: 100, y: 50 })).toEqual([
      [94, 44],
      [106, 56],
    ]);
  });
});

describe("pickNearest", () => {
  const point = { x: 100, y: 100 };
  test("returns the id of the candidate closest to the point, not the first one", () => {
    const hits = [
      { id: 1, x: 105, y: 100 },
      { id: 2, x: 101, y: 101 },
      { id: 3, x: 100, y: 96 },
    ];
    expect(pickNearest(point, hits)).toBe(2);
  });
  test("is independent of the order of the candidates", () => {
    const hits = [
      { id: 2, x: 101, y: 101 },
      { id: 1, x: 105, y: 100 },
    ];
    expect(pickNearest(point, hits)).toBe(2);
    expect(pickNearest(point, [...hits].reverse())).toBe(2);
  });
  test("returns null when nothing was hit", () => {
    expect(pickNearest(point, [])).toBeNull();
  });
  test("a feature returned twice (tile edge) is the same pick", () => {
    expect(
      pickNearest(point, [
        { id: 7, x: 100, y: 100 },
        { id: 7, x: 100, y: 100 },
      ]),
    ).toBe(7);
  });
  test("equal distances pick the lowest id, so the result is deterministic", () => {
    const hits = [
      { id: 9, x: 104, y: 100 },
      { id: 4, x: 96, y: 100 },
    ];
    expect(pickNearest(point, hits)).toBe(4);
    expect(pickNearest(point, [...hits].reverse())).toBe(4);
  });
});

describe("selectionLabel", () => {
  test("is the name when the entity has one", () => {
    expect(selectionLabel(entity(12, { name: "Alpha" }))).toBe("Alpha");
  });
  test("is #id without a name, with a null name or with an empty name", () => {
    expect(selectionLabel(entity(12))).toBe("#12");
    expect(selectionLabel(entity(12, { name: null }))).toBe("#12");
    expect(selectionLabel(entity(12, { name: "" }))).toBe("#12");
  });
});

describe("findEntity", () => {
  const frame = frameOf([1, 3, 5, 8, 13].map((id) => entity(id)));
  test("finds an entity by id in an id-ascending frame", () => {
    for (const id of [1, 3, 5, 8, 13]) expect(findEntity(frame, id)?.id).toBe(id);
  });
  test("is undefined for an id that is absent, below or above the range", () => {
    for (const id of [0, 2, 9, 14]) expect(findEntity(frame, id)).toBeUndefined();
    expect(findEntity(frameOf([]), 1)).toBeUndefined();
  });
  test("is a binary search: a handful of reads in a 10,000-entity frame", () => {
    const entities = Array.from({ length: 10_000 }, (_, i) => entity(i + 1));
    let reads = 0;
    const counted = new Proxy(entities, {
      get(target, key, receiver) {
        if (typeof key === "string" && /^\d+$/.test(key)) reads += 1;
        return Reflect.get(target, key, receiver);
      },
    });
    expect(findEntity(frameOf(counted), 7_321)?.id).toBe(7_321);
    expect(reads).toBeLessThanOrEqual(20);
  });
});

describe("toSelectionCollection", () => {
  const frame = frameOf([entity(1), entity(2, { force: "red" }), entity(3)]);

  test("holds exactly the selected entity: id, position and affiliation", () => {
    const fc = toSelectionCollection(frame, 2, defaultAffiliationOf);
    expect(fc.type).toBe("FeatureCollection");
    expect(fc.features).toHaveLength(1);
    const [f] = fc.features;
    expect(f?.id).toBe(2);
    expect(f?.geometry.coordinates).toEqual([20.002, 10.002]);
    expect(f?.properties.affiliation).toBe(defaultAffiliationOf("red"));
  });
  test("uses the affiliation function it is given", () => {
    const fc = toSelectionCollection(frame, 1, () => "hostile");
    expect(fc.features[0]?.properties.affiliation).toBe("hostile");
  });
  test("is empty for no selection and for an id that is not in the frame", () => {
    expect(toSelectionCollection(frame, null, defaultAffiliationOf).features).toHaveLength(0);
    expect(toSelectionCollection(frame, 99, defaultAffiliationOf).features).toHaveLength(0);
  });
  test("rewrites the collection passed as reuse in place, and never holds more than one feature", () => {
    const first = toSelectionCollection(frame, 1, defaultAffiliationOf);
    const again = toSelectionCollection(frame, 3, defaultAffiliationOf, first);
    expect(again).toBe(first);
    expect(first.features).toHaveLength(1);
    expect(first.features[0]?.id).toBe(3);
    const cleared = toSelectionCollection(frame, null, defaultAffiliationOf, first);
    expect(cleared).toBe(first);
    expect(first.features).toHaveLength(0);
  });
});
