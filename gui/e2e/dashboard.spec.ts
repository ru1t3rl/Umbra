import AxeBuilder from "@axe-core/playwright";
import { expect, type Page, test } from "@playwright/test";

type Theme = "ops" | "day";
const THEMES: Theme[] = ["ops", "day"];
const SHOT = {
  maxDiffPixelRatio: 0.001,
  threshold: 0.05,
  animations: "disabled",
  caret: "hide",
} as const;

/**
 * No engine runs in the `ui` project, so the stream fails and the page settles in `reconnecting`: the
 * "Engine not reachable" alert is the state under test (first `connecting`, then `reconnecting` for every
 * later retry, ASM-357). The map canvas is hidden for the screenshot (software WebGL is not pixel stable, ASM-313).
 */
async function openDashboard(page: Page, theme: Theme): Promise<void> {
  await page.goto("/");
  await expect(page.getByRole("alert", { name: "Engine not reachable" })).toBeVisible();
  await expect(page.getByRole("region", { name: "Live map" })).toBeVisible();
  if (theme === "day") await page.getByRole("switch", { name: "Day theme" }).click();
  await expect(page.locator("html")).toHaveAttribute(
    "data-theme",
    theme === "day" ? "light" : "dark",
  );
  await page.mouse.move(0, 0);
  await page.evaluate(async () => {
    await document.fonts.ready;
  });
  await page.evaluate(() => new Promise<void>((done) => requestAnimationFrame(() => done())));
}

for (const theme of THEMES) {
  test(`dashboard (${theme})`, async ({ page }) => {
    await openDashboard(page, theme);
    // A Playwright `mask` paints over the element's whole box, and the map is full-bleed, so it would hide
    // the bar and the alert too. The canvas is hidden instead: the map's own background shows through.
    await expect(page.locator(".maplibregl-canvas")).toHaveCount(1); // the hiding rule below must hit something
    await page.addStyleTag({ content: ".maplibregl-canvas { visibility: hidden !important; }" });
    await expect(page).toHaveScreenshot(`dashboard-${theme}.png`, SHOT);
  });

  test(`dashboard has no accessibility violations (${theme})`, async ({ page }) => {
    await openDashboard(page, theme);
    // Only the WebGL canvas is excluded (a bitmap axe cannot judge); the container, the attribution control
    // and every banner stay in scope.
    const results = await new AxeBuilder({ page }).exclude(".maplibregl-canvas").analyze();
    expect(
      results.violations.map((v) => `${v.id}: ${v.nodes.map((n) => n.target).join(", ")}`),
    ).toEqual([]);
  });
}

test("the dashboard has one banner, one main and no glass on glass", async ({ page }) => {
  await openDashboard(page, "ops");
  await expect(page.getByRole("banner")).toHaveCount(1);
  await expect(page.getByRole("main")).toHaveCount(1);
  await expect(page.locator(".vt-cmdbar-floating")).toHaveCSS("backdrop-filter", /blur/);
  const blurred = await page
    .locator(".vt-cmdbar-floating")
    .evaluate(
      (bar) =>
        [...bar.querySelectorAll("*")].filter(
          (el) => getComputedStyle(el).backdropFilter !== "none",
        ).length,
    );
  expect(blurred).toBe(0);
});
