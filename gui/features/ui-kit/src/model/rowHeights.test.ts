import { expect, test } from "bun:test";
import { rowHeightTokens } from "../tokens/tokens.ts";
import { ROW_HEIGHT_PX } from "./rowHeights.ts";

// Row heights are fixed per token (ADR 0072); the CSS tokens `--row-*` must match (ASM-872).
test("every row-height token resolves to a positive whole number of pixels", () => {
  for (const t of rowHeightTokens) {
    expect(Number.isInteger(ROW_HEIGHT_PX[t])).toBe(true);
    expect(ROW_HEIGHT_PX[t]).toBeGreaterThan(0);
  }
});

test("the dense log row is lower than the entity row", () => {
  expect(ROW_HEIGHT_PX["row-sm"]).toBeLessThan(ROW_HEIGHT_PX["row-md"]);
});

test("the two-line row is taller than the entity row", () => {
  expect(ROW_HEIGHT_PX["row-lg"]).toBeGreaterThan(ROW_HEIGHT_PX["row-md"]);
});
