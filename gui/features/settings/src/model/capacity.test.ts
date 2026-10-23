import { expect, test } from "bun:test";
import { parseCapacity } from "./capacity.ts";

test("a whole number from 1,000 to 50,000 is a capacity", () => {
  expect(parseCapacity("1000")).toBe(1000);
  expect(parseCapacity("50000")).toBe(50_000);
  expect(parseCapacity(" 7000 ")).toBe(7000);
});

test("anything else is not", () => {
  for (const bad of ["", "  ", "999", "50001", "1500.5", "abc", "-5000", "1e9"])
    expect(parseCapacity(bad), bad).toBeUndefined();
});
