import { expect, test } from "bun:test";
import { route } from "./index.ts";

test("the settings route is a navigation entry with an icon (WI-17)", () => {
  expect(route.path).toBe("/settings");
  expect(route.label).toBe("Settings");
  expect(route.icon).toBe("settings");
  expect(route.nav).toBe(true);
});
