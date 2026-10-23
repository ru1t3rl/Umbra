// WI-17 (docs/plans/M8.md, H-8): the settings page in a real browser. New baselines settings-{ops,day}.png (stored
// overrides, recent scenarios, rate and capacity) and settings-invalid-{ops,day}.png (a flagged override row and a
// flagged journal capacity), both 1280x1700 so that all seven Cards are in view whatever the layout (if the page needs
// more height, raise FULL.height here and say so). No engine runs in the `ui` project: the stream is a fake
// (`routeWebSocket`: a hello, then a frame in run state `ready`, and it records what the client sends), `GET /status`
// is fulfilled by `page.route`, and `config.json` gets a fixed engine URL so no random preview port is in a baseline.
// The settings live in localStorage `umbra.settings.v1` (the web backend); the theme in `umbra.theme` (ASM-301).
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
const NARROW = { width: 1280, height: 800 };
const WIDE = { width: 1600, height: 900 };
const FULL = { width: 1280, height: 1700 };
const KEY = "umbra.settings.v1";

const HELLO = {
  type: "hello",
  protocol: "umbra.stream.v1",
  api_version: "1.0.0",
  encoding: "json",
  settings: {
    default_rate_hz: 30,
    max_rate_hz: 60,
    max_entity_ids: 10000,
    max_events_per_frame: 4096,
    max_message_bytes: 16_000_000,
    queue_frames: 4,
    send_timeout_s: 10,
  },
};
const RUN = {
  state: "ready",
  session: 1,
  clock: { mode: "realtime" },
  step_pending: false,
  tick: 0,
  time_s: 0,
};
const FRAME = {
  type: "frame",
  kind: "snapshot",
  cause: "control",
  session: 1,
  seq: 0,
  tick: 0,
  time_s: 0,
  dropped: 0,
  entities: [],
  events: [],
  events_truncated: false,
  run: RUN,
};

/** What a run starts with: two overrides, two recent scenarios, a 24 Hz rate and a capacity of 8,000. */
const SEEDED = {
  version: 1,
  stream: { maxRateHz: 24 },
  journal: { capacity: 8000 },
  map: { affiliationOverrides: { blue: "hostile", purple: "friend" } },
  scenarios: { recent: ["examples/walking_skeleton.yaml", "examples/patrol.yaml"] },
};

interface Opened {
  /** The messages the page sent on the stream socket, parsed. */
  sent: Array<Record<string, unknown>>;
}

async function openSettings(
  page: Page,
  theme: Theme,
  viewport: { width: number; height: number },
  options: { seed?: boolean; state?: string } = {},
): Promise<Opened> {
  const sent: Array<Record<string, unknown>> = [];
  const state = options.state ?? "ready";
  await page.routeWebSocket(/\/api\/v1\/stream/, (ws) => {
    ws.onMessage((m) => {
      try {
        sent.push(JSON.parse(String(m)));
      } catch {
        // not JSON: ignore
      }
    });
    ws.send(JSON.stringify(HELLO));
    ws.send(JSON.stringify({ ...FRAME, run: { ...RUN, state } }));
  });
  // The preview's port is random; a fixed engine URL keeps it out of the baselines. Fulfilled cross-origin
  // replies need the CORS header.
  const cors = { "access-control-allow-origin": "*" };
  await page.route(/\/config\.json$/, (route) =>
    route.fulfill({
      contentType: "application/json",
      body: JSON.stringify({ engineUrl: "http://engine.test:8080" }),
    }),
  );
  await page.route(/\/api\/v1\/status$/, (route) =>
    route.fulfill({
      contentType: "application/json",
      headers: cors,
      body: JSON.stringify({ api_version: "1.0.0", run: { ...RUN, state } }),
    }),
  );
  if (options.seed !== false) {
    // Only when nothing is stored yet, so a reload keeps what the page wrote.
    await page.addInitScript(
      ([key, value]) => {
        if (window.localStorage.getItem(key) === null) window.localStorage.setItem(key, value);
      },
      [KEY, JSON.stringify(SEEDED)] as const,
    );
  }
  await page.setViewportSize(viewport);
  await page.goto("/settings");
  await expect(page.getByRole("heading", { level: 3, name: "About" })).toBeVisible();
  if (theme === "day") await page.getByRole("switch", { name: "Day theme", exact: true }).click();
  await expect(page.locator("html")).toHaveAttribute(
    "data-theme",
    theme === "day" ? "light" : "dark",
  );
  await page.mouse.move(0, 0);
  await page.evaluate(async () => {
    await document.fonts.ready;
  });
  await page.evaluate(() => new Promise<void>((done) => requestAnimationFrame(() => done())));
  return { sent };
}

const stored = (page: Page) =>
  page.evaluate((key) => {
    const raw = window.localStorage.getItem(key);
    return raw === null ? null : JSON.parse(raw);
  }, KEY);

/** Flags a row in Map and the journal capacity in Log. */
async function flagSomething(page: Page): Promise<void> {
  await page.getByRole("button", { name: "Add override" }).click();
  const force = page.getByRole("textbox", { name: "Force" }).last();
  await force.fill("constructor");
  await force.blur();
  await expect(force).toHaveAttribute("aria-invalid", "true");
  const capacity = page.getByRole("spinbutton", { name: "Journal capacity" });
  await capacity.fill("999");
  await capacity.blur();
  await expect(capacity).toHaveAttribute("aria-invalid", "true");
  await page.mouse.move(0, 0);
}

const violations = async (page: Page) =>
  (await new AxeBuilder({ page }).analyze()).violations.map(
    (v) => `${v.id}: ${v.nodes.map((n) => n.target).join(", ")}`,
  );

for (const theme of THEMES) {
  test(`settings page (${theme})`, async ({ page }) => {
    await openSettings(page, theme, FULL);
    await expect(page).toHaveScreenshot(`settings-${theme}.png`, SHOT);
  });

  test(`settings page with a flagged override and a flagged capacity (${theme})`, async ({
    page,
  }) => {
    await openSettings(page, theme, FULL);
    await flagSomething(page);
    await expect(page).toHaveScreenshot(`settings-invalid-${theme}.png`, SHOT);
  });

  test(`settings page has no accessibility violations (${theme})`, async ({ page }) => {
    await openSettings(page, theme, NARROW);
    expect(await violations(page)).toEqual([]);
  });

  test(`settings page with flagged fields has no accessibility violations (${theme})`, async ({
    page,
  }) => {
    await openSettings(page, theme, NARROW);
    await flagSomething(page);
    expect(await violations(page)).toEqual([]);
  });

  test(`the idle settings page has no accessibility violations (${theme})`, async ({ page }) => {
    await openSettings(page, theme, WIDE, { state: "idle" });
    await expect(page.getByText(/load a scenario/i)).toHaveCount(0);
    expect(await violations(page)).toEqual([]);
  });
}

for (const viewport of [NARROW, WIDE]) {
  test(`nothing overflows sideways at ${viewport.width} px`, async ({ page }) => {
    await openSettings(page, "ops", viewport);
    const bar = page.getByRole("banner");
    const barBox = await bar.boundingBox();
    expect(barBox).not.toBeNull();
    if (!barBox) return;
    expect(barBox.height).toBeLessThanOrEqual(64); // one row
    expect(barBox.width).toBeLessThanOrEqual(viewport.width);
    const barOverflow = await bar.evaluate(
      (el) =>
        [el, ...el.querySelectorAll("*")].filter((c) => {
          const r = c.getBoundingClientRect();
          return r.right > el.getBoundingClientRect().right + 0.5;
        }).length,
    );
    expect(barOverflow).toBe(0);
    const doc = await page.evaluate(() => ({
      scrollWidth: document.documentElement.scrollWidth,
      clientWidth: document.documentElement.clientWidth,
    }));
    expect(doc.scrollWidth).toBeLessThanOrEqual(doc.clientWidth);
  });
}

test("the navigation marks Settings as the current page; one banner, one main, seven sections", async ({
  page,
}) => {
  await openSettings(page, "ops", WIDE);
  await expect(page.getByRole("banner")).toHaveCount(1);
  await expect(page.getByRole("main")).toHaveCount(1);
  await expect(
    page.getByRole("navigation", { name: "Pages" }).getByRole("link", { name: /Settings/ }),
  ).toHaveAttribute("aria-current", "page");
  await expect(page.getByRole("heading", { level: 3 })).toHaveText([
    "Engine connection",
    "Appearance",
    "Live data",
    "Map",
    "Log",
    "Scenarios",
    "About",
  ]);
});

test("the engine URL of the runtime config is shown read-only", async ({ page }) => {
  await openSettings(page, "ops", WIDE);
  const field = page.getByRole("textbox", { name: "Engine URL" });
  await expect(field).toHaveValue("http://engine.test:8080");
  await expect(field).toHaveAttribute("readonly", "");
});

test("About shows the API version of the status and the link opens the showcase in the app", async ({
  page,
}) => {
  await openSettings(page, "ops", WIDE);
  await expect(page.getByText("API version")).toBeVisible();
  await expect(page.getByText("API version").locator("xpath=following-sibling::dd[1]")).toHaveText(
    "1.0.0",
  );
  await page.getByRole("link", { name: "Design showcase" }).click();
  await expect(page).toHaveURL(/\/showcase$/);
});

// Each setting persists through the backend (localStorage) and takes effect.
test("the stream rate persists, sends a subscribe and survives a reload", async ({ page }) => {
  const opened = await openSettings(page, "ops", WIDE);
  const slider = page.getByRole("slider", { name: "Stream rate" });
  await expect(slider).toHaveValue("24");
  await slider.focus();
  await page.keyboard.press("End");
  await expect(slider).toHaveValue("60");
  expect((await stored(page)).stream.maxRateHz).toBe(60);
  await expect
    .poll(() => opened.sent.filter((m) => m.type === "subscribe").at(-1))
    .toEqual({ type: "subscribe", max_rate_hz: 60 });
  await page.reload();
  await expect(page.getByRole("slider", { name: "Stream rate" })).toHaveValue("60");
});

test("an override persists, is listed after a reload and removing it forgets it", async ({
  page,
}) => {
  await openSettings(page, "ops", WIDE);
  await page.getByRole("button", { name: "Add override" }).click();
  const force = page.getByRole("textbox", { name: "Force" }).last();
  await expect(force).toBeFocused();
  await page.getByRole("combobox", { name: "Affiliation" }).last().selectOption("neutral");
  await force.fill("  Teal ");
  await force.press("Enter");
  await expect(force).toHaveValue("teal");
  expect((await stored(page)).map.affiliationOverrides).toEqual({
    blue: "hostile",
    purple: "friend",
    teal: "neutral",
  });
  await page.reload();
  await expect(page.getByRole("textbox", { name: "Force" })).toHaveCount(3);
  await page.getByRole("button", { name: "Remove override teal" }).click();
  await expect(page.getByRole("button", { name: "Add override" })).toBeFocused();
  expect((await stored(page)).map.affiliationOverrides).toEqual({
    blue: "hostile",
    purple: "friend",
  });
});

test("invalid overrides and an invalid capacity are flagged and not saved", async ({ page }) => {
  await openSettings(page, "ops", WIDE);
  await flagSomething(page);
  const dup = page.getByRole("button", { name: "Add override" });
  await dup.click();
  const force = page.getByRole("textbox", { name: "Force" }).last();
  await force.fill("BLUE");
  await force.blur();
  await expect(force).toHaveAttribute("aria-invalid", "true");
  await expect(force).toHaveAccessibleDescription(/already listed/i);
  const after = await stored(page);
  expect(after.map.affiliationOverrides).toEqual({ blue: "hostile", purple: "friend" });
  expect(after.journal.capacity).toBe(8000);
});

test("the journal capacity persists on blur", async ({ page }) => {
  await openSettings(page, "ops", WIDE);
  const capacity = page.getByRole("spinbutton", { name: "Journal capacity" });
  await capacity.fill("20000");
  await capacity.blur();
  expect((await stored(page)).journal.capacity).toBe(20_000);
  await page.reload();
  await expect(page.getByRole("spinbutton", { name: "Journal capacity" })).toHaveValue("20000");
});

test("Clear recent scenarios empties the stored list", async ({ page }) => {
  await openSettings(page, "ops", WIDE);
  await expect(page.getByText("2 recent scenarios")).toBeVisible();
  await page.getByRole("button", { name: "Clear recent scenarios" }).click();
  await expect(page.getByText("No recent scenarios")).toBeVisible();
  await expect(page.getByRole("button", { name: "Clear recent scenarios" })).toBeDisabled();
  expect((await stored(page)).scenarios.recent).toEqual([]);
});

test("the theme preference switches the theme, is kept in umbra.theme and follows the bar's switch", async ({
  page,
}) => {
  await openSettings(page, "ops", WIDE);
  await page.getByRole("combobox", { name: "Theme preference" }).selectOption("light");
  await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
  await expect(page.getByRole("switch", { name: "Day theme", exact: true })).toBeChecked();
  expect(await page.evaluate(() => window.localStorage.getItem("umbra.theme"))).toBe("light");
  await page.reload();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
  await page.getByRole("switch", { name: "Use the Day theme" }).click();
  await expect(page.getByRole("combobox", { name: "Theme preference" })).toHaveValue("dark");
});
