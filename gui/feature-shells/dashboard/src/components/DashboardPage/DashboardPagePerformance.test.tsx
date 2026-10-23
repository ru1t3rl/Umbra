// WI-14 failing test, performance half (plan "Performance budgets" item 6, NFR-1). Written before the
// implementation. 10,000 entities, every panel mounted (list, inspector on one entity, the compact log in its
// tab, the bar), frames arriving under fake timers. Counts, not milliseconds: render counts are deterministic;
// the flush time at 10,000 entities is measured by the implementer and logged in docs/plans/M8.md (not gated).
import { afterEach, beforeEach, describe, expect, jest, mock, test } from "bun:test";
import { act, cleanup, screen, within } from "@testing-library/react";
import {
  type Dashboard,
  fakeFetch,
  installResizeObserver,
  LiveMapStub,
  lastMapProps,
  mapRenders,
  mountDashboard,
  resetMapStub,
  wireEntity,
  wireFrame,
} from "../../testing.tsx";

mock.module("@umbra/map-view", () => ({ LiveMap: LiveMapStub }));
const { default: DashboardPage } = await import("./DashboardPage.tsx");

const COUNT = 10_000;
const FRAMES = 30;
const VIEWPORT = 400;
let restoreObserver: () => void;
let restoreFetch: () => void;
let page: Dashboard | undefined;
beforeEach(() => {
  resetMapStub();
  restoreObserver = installResizeObserver(VIEWPORT);
  restoreFetch = fakeFetch().restore;
});
afterEach(() => {
  cleanup();
  // Fake timers must not leak: after `useFakeTimers` without draining, bun 1.4.4 leaves `waitFor` of later files
  // (same process, readdir order) timing out (ASM-985).
  act(() => void jest.runOnlyPendingTimers());
  jest.useRealTimers();
  page?.close();
  page = undefined;
  restoreFetch();
  restoreObserver();
});

const big = () =>
  Array.from({ length: COUNT }, (_, i) =>
    wireEntity(i + 1, { name: `Unit ${i + 1}`, force: i % 2 === 0 ? "blue" : "red" }),
  );

function mountBig(onCommit: () => void): { entities: unknown[]; d: Dashboard } {
  jest.useFakeTimers();
  const entities = big();
  const d = mountDashboard(DashboardPage, { onCommit });
  page = d;
  d.start(entities);
  act(() => void jest.advanceTimersByTime(300));
  act(() => lastMapProps().onSelect?.(5_000));
  return { entities, d };
}

describe("DashboardPage at 10,000 entities with every panel mounted", () => {
  // Catches: the shell holding frame state (a re-render of the page per frame re-renders the map element).
  test("the map element is not re-rendered by 30 frames", () => {
    const { entities, d } = mountBig(() => {});
    const before = mapRenders.length;
    for (let i = 0; i < FRAMES; i++) {
      d.send(wireFrame(100 + i, entities));
      act(() => void jest.advanceTimersByTime(10));
    }
    expect(mapRenders.length).toBe(before);
  }, 60_000);

  // Catches: cascading commits (a frame causing more than one commit anywhere in the page, for example a
  // list rebuild on every frame instead of at most every 250 ms, ASM-937). The readouts and the inspector
  // update once per frame by design, so the budget is one commit per frame plus the throttled list.
  test("30 frames in 300 ms cause at most one commit per frame plus the throttled list rebuilds", () => {
    let commits = 0;
    const { entities, d } = mountBig(() => {
      commits += 1;
    });
    const mounted = commits;
    for (let i = 0; i < FRAMES; i++) {
      d.send(wireFrame(100 + i, entities));
      act(() => void jest.advanceTimersByTime(10));
    }
    expect(commits - mounted).toBeLessThanOrEqual(FRAMES + 5);
    expect(commits - mounted).toBeGreaterThanOrEqual(1); // the page does follow the stream
  }, 60_000);

  // Catches: an unvirtualized list (10,000 rows in the DOM).
  test("only the visible rows exist in the DOM", () => {
    mountBig(() => {});
    const listbox = screen.getByRole("listbox", { name: "Entities" });
    const rows = within(listbox).getAllByRole("option");
    expect(rows.length).toBeLessThanOrEqual(Math.ceil(VIEWPORT / 56) + 1 + 2 * 8);
    expect(rows[0]).toHaveAttribute("aria-setsize", String(COUNT));
    expect(document.querySelectorAll("*").length).toBeLessThan(1500);
  }, 60_000);

  // Catches: the selection of one entity making the inspector and the list render per entity.
  test("selecting another entity keeps the DOM size bounded", () => {
    mountBig(() => {});
    const size = () => document.querySelectorAll("*").length;
    const before = size();
    act(() => lastMapProps().onSelect?.(9_999));
    expect(size()).toBeLessThanOrEqual(before + 50);
    expect(screen.getByRole("complementary", { name: "Inspector" })).toBeInTheDocument();
  }, 60_000);
});
