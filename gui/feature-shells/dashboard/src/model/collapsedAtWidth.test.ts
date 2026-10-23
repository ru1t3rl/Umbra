import { expect, test } from "bun:test";
import { collapsedAtWidth } from "./collapsedAtWidth.ts";

test("an unmeasured width (0, no layout) starts open", () => {
  expect(collapsedAtWidth(0)).toBe(false);
});

test("1280 px and below start collapsed, wider starts open", () => {
  expect(collapsedAtWidth(1024)).toBe(true);
  expect(collapsedAtWidth(1280)).toBe(true);
  expect(collapsedAtWidth(1281)).toBe(false);
  expect(collapsedAtWidth(1600)).toBe(false);
});
