// WI-12 `componentIndex(components)`: the join table from component id to its `ComponentEntry` (ASM-816, ASM-947).
import { describe, expect, test } from "bun:test";
import type { ComponentEntry } from "@umbra/data";
import { COMPONENTS } from "../browserTesting.tsx";
import { componentIndex } from "./componentIndex.ts";

describe("componentIndex", () => {
  test("maps every component id to its entry", () => {
    const index = componentIndex(COMPONENTS);
    expect(index.size).toBe(COMPONENTS.length);
    for (const c of COMPONENTS) expect(index.get(c.id)).toBe(c);
  });

  test("an unknown id has no entry", () => {
    expect(componentIndex(COMPONENTS).get("acme.lidar")).toBeUndefined();
  });

  test("an empty list gives an empty index", () => {
    expect(componentIndex([]).size).toBe(0);
  });

  test("for a duplicate id the first entry wins (ASM-947)", () => {
    const first: ComponentEntry = { ...(COMPONENTS[0] as ComponentEntry), description: "first" };
    const second: ComponentEntry = { ...first, description: "second" };
    expect(componentIndex([first, second]).get(first.id)?.description).toBe("first");
  });

  test("does not modify its input", () => {
    const input = [...COMPONENTS];
    componentIndex(input);
    expect(input).toEqual([...COMPONENTS]);
  });
});
