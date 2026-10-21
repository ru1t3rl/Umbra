import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { expect, test } from "@playwright/test";

const ROOT_ICON = resolve(import.meta.dirname, "../../umbra_icon.svg");

test("the page links umbra_icon.svg as an svg icon", async ({ page }) => {
  await page.goto("/");
  const link = page.locator('link[rel="icon"]');
  await expect(link).toHaveAttribute("type", "image/svg+xml");
  await expect(link).toHaveAttribute("href", "/umbra_icon.svg");
});

test("the served icon is byte-identical to the repository root file", async ({ request }) => {
  const response = await request.get("/umbra_icon.svg");
  expect(response.ok()).toBe(true);
  expect(response.headers()["content-type"]).toContain("image/svg+xml");
  const served = await response.body();
  expect(Buffer.compare(served, await readFile(ROOT_ICON))).toBe(0);
});

test("unknown route shows page not found", async ({ page }) => {
  await page.goto("/no-such-route");
  await expect(page.getByRole("heading", { name: "Page not found." })).toBeVisible();
});

test("the placeholder route renders under the dark theme", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByRole("heading", { name: "Umbra" })).toBeVisible();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
});

test("headless Chromium can create a WebGL context", async ({ page }) => {
  await page.goto("/");
  const ok = await page.evaluate(
    () => document.createElement("canvas").getContext("webgl2") !== null,
  );
  expect(ok).toBe(true);
});

test("starts in ops and switches to day without a reload", async ({ page }) => {
  await page.goto("/showcase");
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  await page.evaluate(() => {
    (window as unknown as { __marker: string }).__marker = "alive";
  });
  const navigations = () => page.evaluate(() => performance.getEntriesByType("navigation").length);
  const before = await navigations();
  await page.getByRole("switch", { name: "Day theme" }).click();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
  expect(await page.evaluate(() => (window as unknown as { __marker?: string }).__marker)).toBe(
    "alive",
  );
  expect(await navigations()).toBe(before);
});
