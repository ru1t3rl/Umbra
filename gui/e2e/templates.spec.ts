// WI-15 (docs/plans/M8.md, H-8): the templates page in a real browser. New baselines templates-{ops,day}.png
// (1280x800, the "tank" template selected). No engine runs in the `ui` project, so the stream is a fake
// (`routeWebSocket`: a hello, then a frame in run state `ready`) and the REST reads are fulfilled by `page.route`.
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
  run: {
    state: "ready",
    session: 1,
    clock: { mode: "realtime" },
    step_pending: false,
    tick: 0,
    time_s: 0,
  },
};
const TEMPLATES = [
  {
    id: "tank",
    description: "Main battle tank",
    motion: "builtin.waypoint",
    behavior: "builtin.patrol",
    source: "templates/tank.yaml",
  },
  {
    id: "scout",
    description: "Fast recon vehicle",
    motion: "builtin.waypoint",
    behavior: null,
    source: "templates/scout.yaml",
  },
  {
    id: "drone",
    description: "Quadcopter",
    motion: "builtin.hover",
    behavior: "builtin.loiter",
    source: "templates/drone.yaml",
  },
];
const schema = {
  type: "object",
  properties: { speed_mps: { type: "number" }, loop: { type: "boolean" } },
};
const COMPONENTS = [
  ["builtin.waypoint", "motion_model"],
  ["builtin.patrol", "behavior"],
].map(([id, kind]) => ({
  id,
  kind,
  params_schema: schema,
  description: `The ${id} component`,
  provenance: "builtin",
  version: "1.0.0",
}));
const TANK = {
  id: "tank",
  chain: ["tank", "base"],
  source: "templates/tank.yaml",
  description: "Main battle tank",
  motion: { id: "builtin.waypoint", params: { speed_mps: 8, loop: false } },
  sensors: [],
  behavior: { id: "builtin.patrol", params: { speed_mps: 3 } },
};

async function openTemplates(
  page: Page,
  theme: Theme,
  viewport: { width: number; height: number },
  select = true,
): Promise<void> {
  await page.routeWebSocket(/\/api\/v1\/stream/, (ws) => {
    ws.send(JSON.stringify(HELLO));
    ws.send(JSON.stringify(FRAME));
  });
  const json = (body: unknown) => ({ contentType: "application/json", body: JSON.stringify(body) });
  await page.route(/\/api\/v1\/templates$/, (route) => route.fulfill(json(TEMPLATES)));
  await page.route(/\/api\/v1\/templates\/tank$/, (route) => route.fulfill(json(TANK)));
  await page.route(/\/api\/v1\/components/, (route) => route.fulfill(json(COMPONENTS)));
  await page.setViewportSize(viewport);
  await page.goto("/templates");
  await expect(page.getByRole("option", { name: /tank/ })).toBeVisible();
  if (select) {
    await page.getByRole("option", { name: /tank/ }).click();
    await expect(page.getByRole("heading", { name: "tank" })).toBeVisible();
    await expect(page.getByRole("form", { name: "Spawn entity" })).toBeVisible();
  }
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

const NARROW = { width: 1280, height: 800 };
const WIDE = { width: 1600, height: 900 };

for (const theme of THEMES) {
  test(`templates page (${theme})`, async ({ page }) => {
    await openTemplates(page, theme, NARROW);
    await expect(page).toHaveScreenshot(`templates-${theme}.png`, SHOT);
  });

  test(`templates page has no accessibility violations (${theme})`, async ({ page }) => {
    await openTemplates(page, theme, WIDE);
    const results = await new AxeBuilder({ page }).analyze();
    expect(
      results.violations.map((v) => `${v.id}: ${v.nodes.map((n) => n.target).join(", ")}`),
    ).toEqual([]);
  });

  test(`templates page without a selection has no accessibility violations (${theme})`, async ({
    page,
  }) => {
    await openTemplates(page, theme, WIDE, false);
    const results = await new AxeBuilder({ page }).analyze();
    expect(
      results.violations.map((v) => `${v.id}: ${v.nodes.map((n) => n.target).join(", ")}`),
    ).toEqual([]);
  });
}

for (const viewport of [NARROW, WIDE]) {
  test(`nothing overflows at ${viewport.width} px`, async ({ page }) => {
    await openTemplates(page, "ops", viewport);
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
    const page_ = await page.evaluate(() => ({
      scrollWidth: document.documentElement.scrollWidth,
      clientWidth: document.documentElement.clientWidth,
      scrollHeight: document.documentElement.scrollHeight,
      clientHeight: document.documentElement.clientHeight,
    }));
    expect(page_.scrollWidth).toBeLessThanOrEqual(page_.clientWidth);
    expect(page_.scrollHeight).toBeLessThanOrEqual(page_.clientHeight);
    const list = await page.getByRole("region", { name: "Template list" }).boundingBox();
    expect(list?.width).toBe(320);
  });
}

test("the navigation marks Templates as the current page and the page has one banner and one main", async ({
  page,
}) => {
  await openTemplates(page, "ops", WIDE, false);
  await expect(page.getByRole("banner")).toHaveCount(1);
  await expect(page.getByRole("main")).toHaveCount(1);
  await expect(
    page.getByRole("navigation", { name: "Pages" }).getByRole("link", { name: /Templates/ }),
  ).toHaveAttribute("aria-current", "page");
});
