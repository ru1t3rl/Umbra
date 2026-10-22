import { expect, type Page, test } from "@playwright/test";

type Theme = "ops" | "day";
const THEMES: Theme[] = ["ops", "day"];
const SHOT = {
  maxDiffPixelRatio: 0.001,
  threshold: 0.05,
  animations: "disabled",
  caret: "hide",
} as const;

async function openShowcase(page: Page, theme: Theme): Promise<void> {
  await page.goto("/showcase");
  await expect(page.getByRole("heading", { name: "Design system", level: 1 })).toBeVisible();
  if (theme === "day") {
    await page.getByRole("switch", { name: "Day theme" }).click();
  }
  await expect(page.locator("html")).toHaveAttribute(
    "data-theme",
    theme === "day" ? "light" : "dark",
  );
  // keep the pointer off any control so no hover state is captured
  await page.mouse.move(0, 0);
  // fonts (text and Material Symbols) must be loaded before pixels are compared
  await page.evaluate(async () => {
    await document.fonts.ready;
  });
  await page.evaluate(() => new Promise<void>((done) => requestAnimationFrame(() => done())));
}

for (const theme of THEMES) {
  test(`showcase tokens (${theme})`, async ({ page }) => {
    await openShowcase(page, theme);
    await expect(page.locator("#showcase-tokens")).toHaveScreenshot(
      `showcase-tokens-${theme}.png`,
      SHOT,
    );
  });

  test(`showcase components (${theme})`, async ({ page }) => {
    await openShowcase(page, theme);
    await expect(page.locator("#showcase-components")).toHaveScreenshot(
      `showcase-components-${theme}.png`,
      SHOT,
    );
  });

  test(`showcase floating bar has no glass on glass (${theme})`, async ({ page }) => {
    await openShowcase(page, theme);
    const bar = page.locator("#showcase-floating .vt-cmdbar");
    // the bar itself is the one glass layer
    await expect(bar).toHaveCSS("backdrop-filter", /blur/);
    const inner = [
      bar.locator(".vt-chip-glass").first(),
      bar.locator(".vt-chip-glass:not([aria-pressed='true'])"),
      bar.locator(".vt-panel"),
    ];
    for (const el of inner) await expect(el).toHaveCSS("backdrop-filter", "none");
    // no blur on any descendant of the bar
    const blurred = await bar.evaluate(
      (node) =>
        [...node.querySelectorAll("*")].filter((e) => getComputedStyle(e).backdropFilter !== "none")
          .length,
    );
    expect(blurred).toBe(0);
    // unselected glass chip and Panel sit on surface-2, not on the glass fill
    const surface2 = await page.evaluate(() => {
      const probe = document.createElement("div");
      probe.style.backgroundColor = "var(--surface-2)";
      document.body.append(probe);
      const colour = getComputedStyle(probe).backgroundColor;
      probe.remove();
      return colour;
    });
    await expect(bar.locator(".vt-chip-glass:not([aria-pressed='true'])")).toHaveCSS(
      "background-color",
      surface2,
    );
    await expect(bar.locator(".vt-panel")).toHaveCSS("background-color", surface2);
  });

  test(`showcase floating command bar (${theme})`, async ({ page }) => {
    await openShowcase(page, theme);
    await expect(page.locator("#showcase-floating")).toHaveScreenshot(
      `showcase-floating-${theme}.png`,
      SHOT,
    );
  });
}

test("showcase slider arrow keys", async ({ page }) => {
  await openShowcase(page, "ops");
  const slider = page.getByRole("slider", { name: "Update rate" });
  const value = slider.locator(
    "xpath=ancestor::div[contains(@class,'vt-slider-wrap')]//*[contains(@class,'vt-slider-value')]",
  );
  await expect(value).toHaveText("60 Hz");
  await slider.focus();
  await page.keyboard.press("ArrowRight");
  await expect(value).toHaveText("61 Hz");
  await expect(slider).toHaveAttribute("aria-valuetext", "61 Hz");
});
