import { expect, test } from "bun:test";
import { route } from "./index.ts";

test("the showcase route is not a navigation entry but keeps an icon (WI-3)", () => {
  expect(route.path).toBe("/showcase");
  expect(route.nav).toBe(false);
  expect(route.icon.length).toBeGreaterThan(0);
});
