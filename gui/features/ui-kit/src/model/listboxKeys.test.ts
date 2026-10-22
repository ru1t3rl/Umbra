import { expect, test } from "bun:test";
import { nextActiveIndex, scrollToReveal } from "./listboxKeys.ts";

test("arrows clamp and start from the first row when nothing is active", () => {
  expect(nextActiveIndex("ArrowDown", null, 5, 3)).toBe(0);
  expect(nextActiveIndex("ArrowUp", null, 5, 3)).toBe(0);
  expect(nextActiveIndex("ArrowDown", 4, 5, 3)).toBe(4);
  expect(nextActiveIndex("ArrowUp", 0, 5, 3)).toBe(0);
});

test("paging moves by at least one row and clamps", () => {
  expect(nextActiveIndex("PageDown", 0, 100, 0)).toBe(1);
  expect(nextActiveIndex("PageDown", 98, 100, 10)).toBe(99);
  expect(nextActiveIndex("PageUp", 3, 100, 10)).toBe(0);
});

test("Home, End, other keys and an empty list", () => {
  expect(nextActiveIndex("Home", 7, 10, 3)).toBe(0);
  expect(nextActiveIndex("End", 0, 10, 3)).toBe(9);
  expect(nextActiveIndex("a", 0, 10, 3)).toBeNull();
  expect(nextActiveIndex("End", null, 0, 3)).toBeNull();
});

test("scrollToReveal only moves when the row is outside the viewport", () => {
  expect(scrollToReveal(5, 10, 0, 100)).toBe(0);
  expect(scrollToReveal(20, 10, 0, 100)).toBe(110);
  expect(scrollToReveal(2, 10, 50, 100)).toBe(20);
});
