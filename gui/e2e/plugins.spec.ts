// WI-16 (docs/plans/M8.md, H-8): the plugins page in a real browser. New baselines plugins-{ops,day}.png (Plugins
// tab), plugins-adapters-{ops,day}.png and plugins-components-{ops,day}.png (1280x800). No engine runs in the `ui`
// project, so the stream is a fake (`routeWebSocket`: a hello, then a frame in run state `ready`) and the REST reads
// are fulfilled by `page.route`: a plugin with a faulted capability, a faulted adapter.
import AxeBuilder from "@axe-core/playwright";
import { expect, type Page, test } from "@playwright/test";

type Theme = "ops" | "day";
type Tab = "plugins" | "adapters" | "components";
const THEMES: Theme[] = ["ops", "day"];
const TABS: { id: Tab; name: string }[] = [
  { id: "plugins", name: "Plugins" },
  { id: "adapters", name: "Network adapters" },
  { id: "components", name: "Components" },
];
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
const frame = (state: string) => ({
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
    state,
    session: 1,
    clock: { mode: "realtime" },
    step_pending: false,
    tick: 0,
    time_s: 0,
  },
});

const PLUGINS = {
  plugins: [
    {
      id: "acme.radar",
      name: "Acme Radar Pack",
      version: "1.2.0",
      abi_major: 1,
      abi_minor: 3,
      root: "scenario",
      origin: "plugins/radar.so",
      capabilities: [
        { id: "acme.radar.scan", kind: "sensor", version: "1.0.0", faulted: false },
        { id: "acme.radar.track", kind: "behavior", version: "0.4.1", faulted: true },
      ],
    },
    {
      id: "acme.dis-plugin",
      name: "Acme DIS",
      version: "2.0.1",
      abi_major: 1,
      abi_minor: 0,
      root: "plugins[0]",
      origin: "dis.so",
      capabilities: [{ id: "acme.dis", kind: "network_adapter", version: "2.0.0", faulted: false }],
    },
  ],
};
const counters = (over: Record<string, number> = {}) => ({
  control_ignored: 0,
  decode_errors: 0,
  echo_dropped: 0,
  ghosts_capped: 0,
  invalid_state: 0,
  io_errors: 0,
  keys_exhausted: 0,
  published: 0,
  received: 0,
  rx_dropped: 0,
  tx_dropped: 0,
  unmapped: 0,
  unsupported: 0,
  world_errors: 0,
  ...over,
});
const ADAPTERS = [
  {
    name: "dis-main",
    adapter: "builtin.dis",
    enabled: true,
    faulted: false,
    ghosts: 3,
    key_space: { base: 1000, count: 500 },
    counters: counters({ published: 120, received: 87, rx_dropped: 4 }),
  },
  {
    name: "hla-lab",
    adapter: "acme.hla",
    plugin: "acme.hla-plugin",
    enabled: false,
    faulted: true,
    fault_reason: "socket closed",
    ghosts: 0,
    key_space: { base: 5000, count: 100 },
    counters: counters({ io_errors: 9 }),
  },
];
const COMPONENTS = [
  ["builtin.waypoint", "motion_model", "builtin"],
  ["builtin.patrol", "behavior", "builtin"],
  ["acme.radar.scan", "sensor", "plugin"],
  ["file.loiter", "behavior", "file"],
].map(([id, kind, provenance]) => ({
  id,
  kind,
  provenance,
  version: "1.0.0",
  description: `The ${id} component`,
  params_schema: { type: "object" },
}));

async function openPlugins(
  page: Page,
  theme: Theme,
  viewport: { width: number; height: number },
  tab: Tab = "plugins",
  state = "ready",
  plugins: unknown = PLUGINS,
): Promise<void> {
  await page.routeWebSocket(/\/api\/v1\/stream/, (ws) => {
    ws.send(JSON.stringify(HELLO));
    ws.send(JSON.stringify(frame(state)));
  });
  const json = (body: unknown) => ({ contentType: "application/json", body: JSON.stringify(body) });
  await page.route(/\/api\/v1\/plugins$/, (route) => route.fulfill(json(plugins)));
  await page.route(/\/api\/v1\/network\/adapters$/, (route) => route.fulfill(json(ADAPTERS)));
  await page.route(/\/api\/v1\/components/, (route) => route.fulfill(json(COMPONENTS)));
  await page.setViewportSize(viewport);
  await page.goto("/plugins");
  if (state === "idle") {
    // The idle state shows once and the summary and tabs are not mounted (ASM-994 d).
    await expect(page.getByText("Load a scenario to see plugin status")).toHaveCount(1);
    await expect(page.getByRole("tablist")).toHaveCount(0);
  } else {
    await expect(page.getByRole("list", { name: "Faults" })).toBeVisible();
    const name = TABS.find((t) => t.id === tab)?.name ?? "Plugins";
    await page.getByRole("tab", { name }).click();
    await expect(page.getByRole("tab", { name })).toHaveAttribute("aria-selected", "true");
    await expect(page.getByRole("tabpanel", { name })).toBeVisible();
    if (tab === "plugins")
      await expect(page.getByText("Acme Radar Pack", { exact: true })).toBeVisible();
    if (tab === "adapters") await expect(page.getByText("hla-lab").first()).toBeVisible();
    if (tab === "components")
      await expect(page.getByText("acme.radar.scan", { exact: true })).toBeVisible();
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
const shotName = (tab: Tab, theme: Theme) =>
  tab === "plugins" ? `plugins-${theme}.png` : `plugins-${tab}-${theme}.png`;

for (const theme of THEMES) {
  for (const { id, name } of TABS) {
    test(`plugins page, ${name} tab (${theme})`, async ({ page }) => {
      await openPlugins(page, theme, NARROW, id);
      await expect(page).toHaveScreenshot(shotName(id, theme), SHOT);
    });

    test(`plugins page, ${name} tab has no accessibility violations (${theme})`, async ({
      page,
    }) => {
      await openPlugins(page, theme, WIDE, id);
      const results = await new AxeBuilder({ page }).analyze();
      expect(
        results.violations.map((v) => `${v.id}: ${v.nodes.map((n) => n.target).join(", ")}`),
      ).toEqual([]);
    });
  }

  test(`plugins page in the idle state has no accessibility violations (${theme})`, async ({
    page,
  }) => {
    await openPlugins(page, theme, WIDE, "plugins", "idle");
    await expect(page.getByRole("link", { name: "Open the dashboard" })).toHaveCount(1);
    const results = await new AxeBuilder({ page }).analyze();
    expect(
      results.violations.map((v) => `${v.id}: ${v.nodes.map((n) => n.target).join(", ")}`),
    ).toEqual([]);
  });
}

for (const viewport of [NARROW, WIDE]) {
  test(`nothing overflows at ${viewport.width} px`, async ({ page }) => {
    await openPlugins(page, "ops", viewport);
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

test("the navigation marks Plugins as the current page; one banner, one main", async ({ page }) => {
  await openPlugins(page, "ops", WIDE);
  await expect(page.getByRole("banner")).toHaveCount(1);
  await expect(page.getByRole("main")).toHaveCount(1);
  await expect(
    page.getByRole("navigation", { name: "Pages" }).getByRole("link", { name: /Plugins/ }),
  ).toHaveAttribute("aria-current", "page");
});

test("the fault summary shows the faulted capability and adapter above the tabs on every tab", async ({
  page,
}) => {
  await openPlugins(page, "ops", WIDE);
  for (const { name } of TABS) {
    await page.getByRole("tab", { name }).click();
    const faults = page.getByRole("list", { name: "Faults" });
    await expect(faults).toContainText("acme.radar / acme.radar.track");
    await expect(faults).toContainText("socket closed");
    const above = await faults.evaluate((el) =>
      Boolean(
        el.compareDocumentPosition(document.querySelector('[role="tablist"]') ?? el) &
          Node.DOCUMENT_POSITION_FOLLOWING,
      ),
    );
    expect(above).toBe(true);
  }
});

test("the tabs work from the keyboard", async ({ page }) => {
  await openPlugins(page, "ops", WIDE);
  await page.getByRole("tab", { name: "Plugins" }).focus();
  await page.keyboard.press("ArrowRight");
  await expect(page.getByRole("tab", { name: "Network adapters" })).toHaveAttribute(
    "aria-selected",
    "true",
  );
  await expect(page.getByRole("tab", { name: "Network adapters" })).toBeFocused();
  await expect(page.getByRole("tabpanel", { name: "Network adapters" })).toContainText("dis-main");
});

// WI-16 review M1: the panel scrolls, so it must take focus (WCAG 2.1.1) and pass axe (scrollable-region-focusable).
const MANY_PLUGINS = {
  plugins: Array.from({ length: 4 }, (_, i) => ({
    ...PLUGINS.plugins[0],
    id: `acme.radar${i}`,
    name: i === 0 ? "Acme Radar Pack" : `Acme Radar Pack ${i}`,
  })),
};

for (const theme of THEMES) {
  test(`an overflowing Plugins panel has no accessibility violations (${theme})`, async ({
    page,
  }) => {
    await openPlugins(page, theme, NARROW, "plugins", "ready", MANY_PLUGINS);
    const overflows = await page
      .getByRole("tabpanel")
      .evaluate((el) => el.scrollHeight > el.clientHeight);
    expect(overflows).toBe(true);
    const results = await new AxeBuilder({ page }).analyze();
    expect(
      results.violations.map((v) => `${v.id}: ${v.nodes.map((n) => n.target).join(", ")}`),
    ).toEqual([]);
  });
}

test("Tab from the tablist reaches the scrolling panel and the keys scroll it", async ({
  page,
}) => {
  await openPlugins(page, "ops", NARROW, "plugins", "ready", MANY_PLUGINS);
  const panel = page.getByRole("tabpanel");
  expect(await panel.evaluate((el) => el.scrollHeight > el.clientHeight)).toBe(true);
  await page.getByRole("tab", { name: "Plugins" }).focus();
  await page.keyboard.press("Tab");
  await expect(panel).toBeFocused();
  expect(await panel.evaluate((el) => el.scrollTop)).toBe(0);
  await page.keyboard.press("PageDown");
  await expect.poll(() => panel.evaluate((el) => el.scrollTop)).toBeGreaterThan(0);
  const after = await panel.evaluate((el) => el.scrollTop);
  await page.keyboard.press("ArrowDown");
  await expect.poll(() => panel.evaluate((el) => el.scrollTop)).toBeGreaterThan(after);
});
