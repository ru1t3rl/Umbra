// WI-10 (plan "Model: findById (binary search)"): written before model/findById.ts exists.
// Catches: a linear scan of a 10,000-entity frame, a wrong result for absent ids, an unsorted-input crash.
import { describe, expect, test } from "bun:test";
import { summary } from "../inspectorTesting.tsx";
import { findById } from "./findById.ts";

describe("findById", () => {
  const entities = [1, 3, 5, 8, 13].map((id) => summary(id));
  test("finds an entity by id in an id-ascending list", () => {
    for (const id of [1, 3, 5, 8, 13]) expect(findById(entities, id)?.id).toBe(id);
  });
  test("is undefined for ids that are absent, below or above the range, and for an empty list", () => {
    for (const id of [0, 2, 9, 14]) expect(findById(entities, id)).toBeUndefined();
    expect(findById([], 1)).toBeUndefined();
  });
  test("is a binary search: a handful of reads in a 10,000-entity list", () => {
    const big = Array.from({ length: 10_000 }, (_, i) => summary(i + 1));
    let reads = 0;
    const counted = new Proxy(big, {
      get(target, key, receiver) {
        if (typeof key === "string" && /^\d+$/.test(key)) reads += 1;
        return Reflect.get(target, key, receiver);
      },
    });
    expect(findById(counted, 7_321)?.id).toBe(7_321);
    expect(reads).toBeLessThanOrEqual(20);
  });
});
