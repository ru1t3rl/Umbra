import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";

const html = readFileSync(join(import.meta.dir, "../index.html"), "utf8");

test("index.html links umbra_icon.svg as an svg icon", () => {
  const doc = new DOMParser().parseFromString(html, "text/html");
  const link = doc.querySelector('link[rel="icon"]');
  expect(link?.getAttribute("type")).toBe("image/svg+xml");
  expect(link?.getAttribute("href")).toBe("/umbra_icon.svg");
});

test("index.html is titled Umbra", () => {
  const doc = new DOMParser().parseFromString(html, "text/html");
  expect(doc.title).toBe("Umbra");
});
