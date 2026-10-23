// WI-13 `kindLabel(kind)`: the display label of a component kind (ASM-822, ASM-955).
import { describe, expect, test } from "bun:test";
import { ComponentKind } from "@umbra/data";
import { kindLabel } from "./kindLabel.ts";

describe("kindLabel", () => {
  const expected: Record<string, string> = {
    motion_model: "Motion model",
    terrain: "Terrain",
    behavior: "Behavior",
    sensor: "Sensor",
    condition: "Condition",
    action: "Action",
    network_adapter: "Network adapter",
  };

  test("labels every kind of the contract", () => {
    for (const kind of Object.values(ComponentKind))
      expect(kindLabel(kind)).toBe(expected[kind] as string);
  });

  test("the table covers exactly the contract kinds (a new kind needs a label)", () => {
    expect(Object.keys(expected).sort()).toEqual(Object.values(ComponentKind).sort());
  });

  test("an unknown kind comes back verbatim", () => {
    expect(kindLabel("quantum_sensor")).toBe("quantum_sensor");
  });

  test("the empty string comes back verbatim", () => {
    expect(kindLabel("")).toBe("");
  });
});
