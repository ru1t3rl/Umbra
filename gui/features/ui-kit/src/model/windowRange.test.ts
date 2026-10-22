import { expect, test } from "bun:test";
import { windowRange } from "./windowRange.ts";

// windowRange(scrollTop, viewport, rowHeight, count, overscan) -> { start, end } with end exclusive (ASM-871).

test("at the top the window starts at 0 and extends by the viewport plus overscan", () => {
  expect(windowRange(0, 100, 20, 1000, 3)).toEqual({ start: 0, end: 8 });
});

test("scrolled into the middle the window is padded by overscan on both sides", () => {
  // rows 10..14 visible (200..300), overscan 3 -> 7..18
  expect(windowRange(200, 100, 20, 1000, 3)).toEqual({ start: 7, end: 18 });
});

test("a partly visible first row is included", () => {
  // scrollTop 210 shows the bottom half of row 10 through the top half of row 15
  expect(windowRange(210, 100, 20, 1000, 0)).toEqual({ start: 10, end: 16 });
});

test("the window is clamped to the end of the list", () => {
  expect(windowRange(19_900, 100, 20, 1000, 8)).toEqual({ start: 987, end: 1000 });
});

test("an empty list gives an empty window", () => {
  expect(windowRange(0, 100, 20, 0, 8)).toEqual({ start: 0, end: 0 });
});

test("a list shorter than the viewport renders every row", () => {
  expect(windowRange(0, 400, 20, 5, 8)).toEqual({ start: 0, end: 5 });
});

test("a zero viewport (not yet measured) renders only the overscan rows", () => {
  expect(windowRange(0, 0, 20, 1000, 8)).toEqual({ start: 0, end: 8 });
});

test("a scrollTop beyond the content is clamped to the last rows", () => {
  const r = windowRange(1_000_000, 100, 20, 50, 2);
  expect(r.end).toBe(50);
  expect(r.start).toBeLessThan(50);
  expect(r.start).toBeGreaterThanOrEqual(0);
});

test("the window never exceeds ceil(viewport / rowHeight) + 1 + 2 * overscan rows", () => {
  for (let top = 0; top < 200_000; top += 137) {
    const { start, end } = windowRange(top, 333, 24, 10_000, 8);
    expect(end - start).toBeLessThanOrEqual(Math.ceil(333 / 24) + 1 + 16);
  }
});
