import { expect, test } from "bun:test";
import { route } from "./index.ts";

test("the plugins route is a navigation entry with an icon (WI-16)", () => {
  expect(route.path).toBe("/plugins");
  expect(route.label).toBe("Plugins");
  expect(route.icon).toBe("extension");
  expect(route.nav).toBe(true);
});
