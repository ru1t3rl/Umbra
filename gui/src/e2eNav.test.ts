import { expect, test } from "bun:test";
import { e2eNavItems } from "./e2eNav.ts";

test("valid entries pass through, anything else is dropped", () => {
  expect(
    e2eNavItems([
      { path: "/plugins", label: "Plugins", icon: "extension" },
      { path: "/settings", label: "Settings", icon: "settings" },
      { path: 1, label: "bad" },
      { path: "plugins", label: "relative" },
      { path: "//evil.example/x", label: "protocol-relative" },
      { label: "no path" },
      "x",
    ]),
  ).toEqual([
    { path: "/plugins", label: "Plugins", icon: "extension" },
    { path: "/settings", label: "Settings", icon: "settings" },
  ]);
});

test("a missing or non-array source gives no entries", () => {
  expect(e2eNavItems(undefined)).toEqual([]);
  expect(e2eNavItems({ path: "/x", label: "x" })).toEqual([]);
});

test("without the e2e flag the router adds nothing (production and unit tests)", async () => {
  // An entry is on the global, as in an e2e run; without the flag the router must not read it.
  Reflect.set(window, "__UMBRA_E2E_NAV__", [{ path: "/injected", label: "Injected" }]);
  try {
    const { navItems } = await import("./router.tsx");
    expect(navItems.map((i) => i.path)).toEqual(["/", "/templates", "/plugins", "/settings"]);
  } finally {
    Reflect.deleteProperty(window, "__UMBRA_E2E_NAV__");
  }
});
