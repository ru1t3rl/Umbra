import { expect, type Page, test } from "@playwright/test";

// The ui-kit Table `render` column cell (M8 WI-13, ASM-961): normal and dense. Its own spec and section, so the
// baselines of showcase.spec.ts are untouched.
type Theme = "ops" | "day";
const SHOT = {
  maxDiffPixelRatio: 0.001,
  threshold: 0.05,
  animations: "disabled",
  caret: "hide",
} as const;

async function openShowcase(page: Page, theme: Theme): Promise<void> {
  await page.goto("/showcase");
  await expect(page.getByRole("heading", { name: "Design system", level: 1 })).toBeVisible();
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

for (const theme of ["ops", "day"] as const) {
  test(`showcase table cells (${theme})`, async ({ page }) => {
    await openShowcase(page, theme);
    const demo = page.locator("#showcase-table-cells");
    await expect(demo.getByText("Faulted")).toHaveCount(2);
    await expect(demo).toHaveScreenshot(`showcase-table-cells-${theme}.png`, SHOT);
  });
}
