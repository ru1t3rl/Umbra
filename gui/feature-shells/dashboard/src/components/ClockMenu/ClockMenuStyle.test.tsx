// WI-14 review round 2 minor 3 (ASM-988): the Clock panel hangs from the bar's right edge, so it cannot cover the
// alert column in the middle of the page at any width; the e2e matrix checks the geometry in a browser.

import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";

const css = readFileSync(join(import.meta.dir, "ClockMenu.css"), "utf8");

test("the button wrapper is not a positioning context (the panel anchors to the bar)", () => {
  expect(css).not.toMatch(/\.dashboard-clock\s*\{[^}]*position:\s*relative/);
});

test("the panel is absolutely positioned against the bar's right edge", () => {
  expect(css).toMatch(/\.dashboard-clock-panel\s*\{[^}]*position:\s*absolute/);
  expect(css).toMatch(/\.dashboard-clock-panel\s*\{[^}]*right:\s*var\(--space-4\)/);
});
