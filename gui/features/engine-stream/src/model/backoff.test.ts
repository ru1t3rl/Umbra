import { expect, test } from "bun:test";
import { backoffDelay } from "./backoff.ts";

test("0.5, 1, 2, 4, 8 s, then capped at 8 s", () => {
  expect([0, 1, 2, 3, 4, 5, 50].map(backoffDelay)).toEqual([
    500, 1000, 2000, 4000, 8000, 8000, 8000,
  ]);
});
test("a negative attempt is the first delay", () => {
  expect(backoffDelay(-3)).toBe(500);
});
