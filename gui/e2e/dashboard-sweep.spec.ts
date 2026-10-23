// WI-15 review round 2 (ASM-992): the bar's contents must not shrink or wrap where the fit ladder cannot see it.
// A sweep of the viewport width, 1600 down to 1280 px in steps of 20, with the idle and the running run state and
// with two links (as shipped) and four (two more injected through `__UMBRA_E2E_NAV__`, src/e2eNav.ts). At every
// width: no element of the bar is clipped (`scrollWidth` over `clientWidth`; only the title's own text box may be,
// and only when the title is dropped), the theme switch, the nav links, the buttons and every readout label are on
// one line, the bar does not overflow, and the level (0 full, 1 icon navigation, 2 no title, 3 icon lifecycle
// buttons) only grows as the window shrinks.
import { expect, type Page, test } from "@playwright/test";

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
const EXTRA = [
  { path: "/plugins", label: "Plugins", icon: "extension" },
  { path: "/settings", label: "Settings", icon: "settings" },
];
const WIDTHS = Array.from({ length: 17 }, (_, i) => 1600 - i * 20);

/** `worst`: the widest readouts the bar can show (T+99:59:59, tick 9,999,999, 10,000 entities). */
const frame = (state: string, worst: boolean) => {
  const tick = worst ? 9_999_999 : 42;
  const time = worst ? 359_999 : 1.4;
  return {
    type: "frame",
    kind: "snapshot",
    cause: "tick",
    session: 1,
    seq: 0,
    tick,
    time_s: time,
    dropped: 0,
    entities: worst
      ? Array.from({ length: 10_000 }, (_, i) => ({
          id: i + 1,
          name: `Unit ${i + 1}`,
          lat_deg: 52 + (i % 100) * 0.01,
          lon_deg: 5 + (i % 100) * 0.01,
          alt_m: 0,
          heading_deg: 0,
          speed_mps: 1,
          ownership: "owned",
          force: i % 2 === 0 ? "blue" : "red",
        }))
      : [],
    events: [],
    events_truncated: false,
    run: {
      state,
      session: 1,
      clock: { mode: "realtime" },
      step_pending: false,
      tick,
      time_s: time,
    },
  };
};

async function open(page: Page, state: string, links: 2 | 4, worst = false): Promise<void> {
  if (links === 4) {
    await page.addInitScript((entries) => {
      Reflect.set(window, "__UMBRA_E2E_NAV__", entries);
    }, EXTRA);
  }
  await page.routeWebSocket(/\/api\/v1\/stream/, (ws) => {
    ws.send(JSON.stringify(HELLO));
    ws.send(JSON.stringify(frame(state, worst)));
  });
  await page.setViewportSize({ width: WIDTHS[0] ?? 1600, height: 800 });
  await page.goto("/");
  await expect(page.getByText(/^(Idle|Ready|Running|Paused)$/).first()).toBeVisible();
}

const settle = (page: Page) =>
  page.evaluate(async () => {
    await document.fonts.ready;
    for (let i = 0; i < 5; i++) await new Promise((done) => requestAnimationFrame(done));
  });

/** What the bar looks like now: its level and every problem found. */
async function inspect(page: Page) {
  return page.evaluate(() => {
    const bar = document.querySelector<HTMLElement>(".vt-cmdbar");
    if (!bar) throw new Error("no bar");
    const title = bar.querySelector<HTMLElement>(".vt-cmdbar-title");
    const titleShown = Boolean(title?.querySelector("b")?.textContent?.trim());
    const links = Array.from(bar.querySelectorAll<HTMLElement>(".vt-nav-link"));
    const navIcons = links.every((l) => l.hasAttribute("title"));
    const lifecycleIcons = Boolean(bar.querySelector('[aria-label="Load"]'));
    const level = titleShown ? (navIcons ? 1 : 0) : lifecycleIcons ? 3 : 2;
    const name = (el: Element): string =>
      `${el.tagName.toLowerCase()}.${el.className}`.replace(/\s+/g, ".");
    /** The most lines any one text node of the element takes (an icon next to a label is not a second line). */
    const lines = (el: Element): number => {
      let most = 0;
      const walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
      for (let node = walker.nextNode(); node; node = walker.nextNode()) {
        if (!node.textContent?.trim()) continue;
        const range = document.createRange();
        range.selectNodeContents(node);
        const tops = new Set(Array.from(range.getClientRects()).map((r) => Math.round(r.top)));
        most = Math.max(most, tops.size);
      }
      return most;
    };
    const clipped = Array.from(bar.querySelectorAll("*"))
      .filter((el) => !el.closest(".vt-visually-hidden"))
      .filter((el) => el.scrollWidth > el.clientWidth)
      .filter((el) => !(el.closest(".vt-cmdbar-title") && !titleShown))
      .map(name);
    const multiline = Array.from(
      bar.querySelectorAll(
        "dt, .vt-theme-toggle, .vt-toggle, .vt-nav-link, button, [role=switch], dd",
      ),
    )
      .filter((el) => lines(el) > 1)
      .map(name);
    const switchBox = bar.querySelector<HTMLElement>("[role=switch]")?.getBoundingClientRect();
    return {
      level,
      clipped,
      multiline,
      barOverflow: bar.scrollWidth - bar.clientWidth,
      barHeight: bar.getBoundingClientRect().height,
      switchHeight: switchBox?.height ?? 0,
    };
  });
}

async function sweep(page: Page): Promise<string[]> {
  let previous = 0;
  const problems: string[] = [];
  for (const width of WIDTHS) {
    await page.setViewportSize({ width, height: 800 });
    await settle(page);
    const m = await inspect(page);
    if (m.clipped.length > 0) problems.push(`${width}: clipped ${m.clipped.join(", ")}`);
    if (m.multiline.length > 0) problems.push(`${width}: wrapped ${m.multiline.join(", ")}`);
    if (m.barOverflow > 0) problems.push(`${width}: bar overflows by ${m.barOverflow}`);
    if (m.barHeight > 64) problems.push(`${width}: bar is ${m.barHeight} px high`);
    if (m.switchHeight > 28) problems.push(`${width}: switch is ${m.switchHeight} px high`);
    if (m.level < previous) problems.push(`${width}: level ${m.level} after ${previous}`);
    previous = Math.max(previous, m.level);
  }
  return problems;
}

for (const links of [2, 4] as const) {
  for (const state of ["idle", "running"] as const) {
    test(`sweep 1600 to 1280 px, ${links} links, ${state}: nothing clipped or wrapped, the level only grows`, async ({
      page,
    }) => {
      await open(page, state, links);
      expect(await sweep(page)).toEqual([]);
    });
  }
}

// The widest readouts with four links push the bar to level 3 (icon-only lifecycle buttons) at the narrow end; it
// must still not overflow there.
test("sweep 1600 to 1280 px, 4 links, running with the widest readouts: nothing clipped or wrapped, no overflow at level 3", async ({
  page,
}) => {
  await open(page, "running", 4, true);
  expect(await sweep(page)).toEqual([]);
  const m = await inspect(page);
  expect(m.level).toBe(3);
  expect(m.barOverflow).toBeLessThanOrEqual(0);
});
