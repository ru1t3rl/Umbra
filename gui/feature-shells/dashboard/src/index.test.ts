import { expect, test } from "bun:test";
import { route } from "./index.ts";

test("the dashboard route is a navigation entry with an icon (WI-3)", () => {
  expect(route.path).toBe("/");
  expect(route.nav).toBe(true);
  expect(route.icon.length).toBeGreaterThan(0);
});
