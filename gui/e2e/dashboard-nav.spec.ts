// WI-15 (ASM-992): the page navigation is part of the command bar's fit ladder (icon-only before the title goes), so
// the bar keeps its title and one row as shells add navigation entries. The e2e build reads two extra entries from
// `window.__UMBRA_E2E_NAV__` (src/e2eNav.ts; the production build has neither, e2eHook.build.test.ts), standing in
// for the Settings shell (WI-17): four links, as the finished app will have (Plugins is a real link since WI-16).
import { expect, type Page, test } from "@playwright/test";

const EXTRA_ENTRIES = [{ path: "/settings", label: "Settings", icon: "settings" }];

async function open(page: Page, width: number, extra = true): Promise<void> {
  if (extra) {
    await page.addInitScript((entries) => {
      Reflect.set(window, "__UMBRA_E2E_NAV__", entries);
    }, EXTRA_ENTRIES);
  }
  await page.setViewportSize({ width, height: 800 });
  await page.goto("/");
  await expect(page.getByRole("alert", { name: "Engine not reachable" })).toBeVisible();
  await page.evaluate(async () => {
    await document.fonts.ready;
  });
  await page.evaluate(
    () =>
      new Promise<void>((done) => requestAnimationFrame(() => requestAnimationFrame(() => done()))),
  );
}

async function measure(page: Page) {
  return page.evaluate(() => {
    const bar = document.querySelector<HTMLElement>(".vt-cmdbar");
    const title = bar?.querySelector<HTMLElement>(".vt-cmdbar-title");
    const b = title?.querySelector<HTMLElement>("b");
    if (!bar || !title || !b) throw new Error("no command bar");
    const links = Array.from(bar.querySelectorAll<HTMLElement>(".vt-nav-link"));
    return {
      client: bar.clientWidth,
      scroll: bar.scrollWidth,
      height: bar.getBoundingClientRect().height,
      titleText: b.textContent?.trim() ?? "",
      /** Room left in the title's own box (it is a flex child that takes the spare width). */
      titleSlack: title.offsetWidth - b.scrollWidth,
      links: links.map((l) => ({
        name:
          l.getAttribute("aria-label") ??
          (l.textContent ?? "").replace(l.querySelector(".vt-icon")?.textContent ?? "", "").trim(),
        title: l.getAttribute("title"),
        text: l.textContent?.trim() ?? "",
      })),
    };
  });
}

test("at 1600 px with four links the idle bar keeps its title and one row (level 1 or less)", async ({
  page,
}) => {
  await open(page, 1600);
  const m = await measure(page);
  expect(m.links.map((l) => l.name)).toEqual(["Live map", "Templates", "Plugins", "Settings"]);
  expect(m.height).toBeLessThanOrEqual(64);
  expect(m.scroll).toBeLessThanOrEqual(m.client);
  expect(m.titleText).toBe("Live map");
  expect(m.titleSlack).toBeGreaterThanOrEqual(0);
  // Level 1 or less: the lifecycle buttons and the Clock keep their labels.
  await expect(page.getByRole("button", { name: "Start", exact: true })).toHaveText("Start");
  await expect(page.getByRole("button", { name: /^Clock/ })).toContainText("Clock");
});

// Measured with the bar's contents unable to shrink (ASM-992): four links need more than 1280 px with the title, so
// the ladder takes level 2 (the title is dropped, the lifecycle buttons keep their labels). The page navigation
// stays icon-only and the bar does not overflow.
test("at 1280 px with four links the title is dropped, the links are icon-only and the lifecycle buttons labelled (level 2)", async ({
  page,
}) => {
  await open(page, 1280);
  const m = await measure(page);
  expect(m.height).toBeLessThanOrEqual(64);
  expect(m.scroll).toBeLessThanOrEqual(m.client);
  expect(m.titleText).toBe("");
  await expect(page.getByRole("button", { name: "Start", exact: true })).toHaveText("Start");
  for (const link of m.links) {
    expect(link.title).toBe(link.name);
    expect(link.text).toMatch(/^[a-z_]+$/); // only the icon ligature
  }
});

test("the icon-only links keep a name and a tooltip", async ({ page }) => {
  await open(page, 1280, false);
  const link = page
    .getByRole("navigation", { name: "Pages" })
    .getByRole("link", { name: "Templates" });
  await expect(link).toHaveAttribute("title", "Templates");
  await expect(link).toHaveAttribute("aria-label", "Templates");
});
