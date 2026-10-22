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

  for (const part of ["table", "select", "appnav", "vlist-listbox", "vlist-log"] as const) {
    test(`showcase ${part} (${theme})`, async ({ page }) => {
      await openShowcase(page, theme);
      await expect(page.locator(`#showcase-${part}`)).toHaveScreenshot(
        `showcase-${part}-${theme}.png`,
        SHOT,
      );
    });
  }

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

test("showcase select changes by keyboard", async ({ page }) => {
  await openShowcase(page, "ops");
  const select = page.getByRole("combobox", { name: "Coordinate format" });
  await select.focus();
  // The e2e project runs Chromium on Linux/Windows, where ArrowDown changes a closed native select (macOS opens the popup).
  await page.keyboard.press("ArrowDown");
  await expect(select).not.toHaveValue("wgs84");
});

test("showcase navigation exposes the current page", async ({ page }) => {
  await openShowcase(page, "ops");
  const nav = page.locator("#showcase-appnav").getByRole("navigation", { name: "Pages" });
  await expect(nav.locator("[aria-current='page']")).toHaveCount(1);
});

test("showcase virtual listbox scrolls and moves the active row by keyboard", async ({ page }) => {
  await openShowcase(page, "ops");
  const list = page
    .locator("#showcase-vlist-listbox")
    .getByRole("listbox", { name: "Sample entities" });
  await list.focus();
  await page.keyboard.press("ArrowDown");
  await expect(list).toHaveAttribute("aria-activedescendant", /.+/);
  await page.keyboard.press("End");
  await expect(list.getByRole("option").last()).toHaveAttribute("aria-posinset", "10000");
  // far fewer DOM rows than items (real layout, NFR-1)
  expect(await list.getByRole("option").count()).toBeLessThan(100);
});

test("showcase virtual log follows the end until scrolled up", async ({ page }) => {
  await openShowcase(page, "ops");
  const log = page.locator("#showcase-vlist-log").getByRole("log", { name: "Sample event log" });
  const atEnd = () =>
    log.evaluate((el) => Math.abs(el.scrollHeight - el.clientHeight - el.scrollTop) <= 1);
  await expect.poll(atEnd).toBe(true);
  await log.evaluate((el) => {
    el.scrollTop = 0;
  });
  await expect.poll(atEnd).toBe(false);
});

test("showcase virtual log: add event stays at the end, stops after scrolling up, resumes at the end", async ({
  page,
}) => {
  await openShowcase(page, "ops");
  const host = page.locator("#showcase-vlist-log");
  const log = host.getByRole("log", { name: "Sample event log" });
  const add = page.locator("#showcase-m8").getByRole("button", { name: "Add event" });
  const atEnd = () =>
    log.evaluate((el) => Math.abs(el.scrollHeight - el.clientHeight - el.scrollTop) <= 1);
  await expect.poll(atEnd).toBe(true);
  await add.click();
  await expect.poll(atEnd).toBe(true);
  await log.evaluate((el) => {
    el.scrollTop = 0;
  });
  await add.click();
  await expect.poll(atEnd).toBe(false);
  await log.evaluate((el) => {
    el.scrollTop = el.scrollHeight;
  });
  await add.click();
  await expect.poll(atEnd).toBe(true);
});
