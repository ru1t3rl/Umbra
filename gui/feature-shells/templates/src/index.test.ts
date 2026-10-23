import { expect, test } from "bun:test";
import { route } from "./index.ts";

test("the templates route is a navigation entry with an icon (WI-15)", () => {
  expect(route.path).toBe("/templates");
  expect(route.label).toBe("Templates");
  expect(route.icon).toBe("category");
  expect(route.nav).toBe(true);
});
