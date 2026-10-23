// WI-14 review minor 2b: a VirtualList inside a glass Panel must not paint an opaque surface on the glass
// (a white box in Day). The precedent is the CommandBar rule for glass children (no glass on glass).

import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";

test("the stylesheet makes a VirtualList inside a Panel transparent", () => {
  const css = readFileSync(join(import.meta.dir, "Panel.css"), "utf8");
  expect(css).toMatch(/\.vt-panel\s+\.vt-vlist\s*\{[^}]*background:\s*transparent/);
});
