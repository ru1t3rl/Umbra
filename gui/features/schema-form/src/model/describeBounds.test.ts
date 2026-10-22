import { describe, expect, test } from "bun:test";
import { describeBounds } from "./describeBounds.ts";

// Assumed API: describeBounds(schema: Record<string, unknown>): string | undefined.
// Parts in the order min, greater than, max, less than, joined by ", "; numbers via String(n).
describe("describeBounds (ADR 0070: bounds shown as help)", () => {
  test("minimum and maximum", () => {
    // Catches: swapped words or order (move: chance schema 0..1).
    expect(describeBounds({ type: "number", minimum: 0, maximum: 1 })).toBe("min 0, max 1");
  });
  test("exclusiveMinimum reads 'greater than' (engine speed fields)", () => {
    // Catches: treating exclusiveMinimum like minimum (it is a number in draft 2020-12, not a boolean).
    expect(describeBounds({ type: "number", exclusiveMinimum: 0, maximum: 1000 })).toBe(
      "greater than 0, max 1000",
    );
  });
  test("exclusiveMaximum reads 'less than'", () => {
    expect(describeBounds({ type: "number", exclusiveMaximum: 5 })).toBe("less than 5");
  });
  test("negative bounds keep their sign (flat terrain height)", () => {
    expect(describeBounds({ minimum: -1000, maximum: 10000 })).toBe("min -1000, max 10000");
  });
  test("no bounds, or non-numeric bounds, give undefined", () => {
    // Catches: printing 'min undefined' or a string bound.
    expect(describeBounds({ type: "number" })).toBeUndefined();
    expect(describeBounds({ minimum: "0", maximum: null })).toBeUndefined();
  });
});
