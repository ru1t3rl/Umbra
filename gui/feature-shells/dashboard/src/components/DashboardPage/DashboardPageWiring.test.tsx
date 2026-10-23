// WI-14 failing tests, wiring half (skill write-frontend-tests; ADR 0078). Written before the implementation.
// Everything except the map is real (see ../../testing.tsx): the shell is tested through the features it
// composes, with a fake socket, a fake `fetch` and an in-memory settings backend.
// Assumed wiring, from docs/plans/M8.md WI-14:
//   - one `useState<number | null>` selection goes to LiveMap (`selectedId`, `onSelect`), EntityListPanel and
//     EntityInspector; the inspector is mounted only while it is not null;
//   - EntityInspector `onCenter` calls `mapRef.current?.centerOn(id)`; `onClose` clears the selection;
//   - `useAffiliationOf()` (settings) feeds `affiliationOf` of the map, the list and the inspector;
//   - (the stored stream rate is applied by the app, `StreamRateApplier`, not by this page: ASM-997, was ASM-974);
//   - ScenarioLoader gets `recentPaths` from settings; its `onLoaded(path)` calls `useUpdateSettings` with
//     the path first, no duplicate (the settings parser caps the list at 10) (ASM-975).
import { afterEach, beforeEach, describe, expect, mock, test } from "bun:test";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, resolve } from "node:path";
import { act, cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";
import {
  centerOnCalls,
  type Dashboard,
  type FetchCall,
  fakeFetch,
  installResizeObserver,
  json,
  LiveMapStub,
  lastMapProps,
  mountDashboard,
  problem,
  type RunState,
  resetMapStub,
  setCenterOnResult,
  wireEntity,
  wireFrame,
} from "../../testing.tsx";

mock.module("@umbra/map-view", () => ({ LiveMap: LiveMapStub }));
const { default: DashboardPage } = await import("./DashboardPage.tsx");

let restoreObserver: () => void;
let engine: ReturnType<typeof fakeFetch>;
let page: Dashboard | undefined;
beforeEach(() => {
  resetMapStub();
  restoreObserver = installResizeObserver(400);
  engine = fakeFetch();
});
afterEach(() => {
  cleanup();
  page?.close();
  page = undefined;
  engine.restore();
  restoreObserver();
});

const ENTITIES = [
  wireEntity(1, { name: "Alpha", force: "blue" }),
  wireEntity(2, { name: "Bravo", force: "red" }),
  wireEntity(3, { name: "Charlie", force: "blue" }),
];

/** Mount with the stream open and a first frame already delivered (no fake timers, see `preload`). */
function mount(
  stored?: unknown,
  state: RunState = "running",
  entities: unknown[] = ENTITIES,
): Dashboard {
  page = mountDashboard(DashboardPage, { storedSettings: stored, preload: { entities, state } });
  return page;
}
const mapSelect = (id: number | null) => act(() => lastMapProps().onSelect?.(id));
const listbox = () => screen.getByRole("listbox", { name: "Entities" });
const options = () => within(listbox()).getAllByRole("option");
const inspector = () => screen.queryByRole("complementary", { name: "Inspector" });

describe("selection (ADR 0078)", () => {
  // Catches: the list not wired to the shell's selection state.
  test("selecting an entity in the list sets the map's selectedId", () => {
    mount();
    expect(lastMapProps().selectedId ?? null).toBeNull();
    fireEvent.click(options()[1] as HTMLElement);
    expect(lastMapProps().selectedId).toBe(2);
  });

  // Catches: the inspector not following the list.
  test("selecting an entity in the list opens the inspector on it", () => {
    mount();
    expect(inspector()).toBeNull();
    fireEvent.click(options()[0] as HTMLElement);
    expect(within(inspector() as HTMLElement).getByText("Alpha")).toBeInTheDocument();
  });

  // Catches: the map's onSelect not reaching the inspector (the M8 acceptance sentence).
  test("a map onSelect opens the inspector", () => {
    mount();
    mapSelect(3);
    expect(within(inspector() as HTMLElement).getByText("Charlie")).toBeInTheDocument();
  });

  // Catches: the map selection not reaching the list (aria-selected).
  test("a map onSelect marks the row in the list", () => {
    mount();
    mapSelect(2);
    const selected = options().filter((o) => o.getAttribute("aria-selected") === "true");
    expect(selected).toHaveLength(1);
    expect(selected[0]).toHaveTextContent("Bravo");
  });

  // Catches: a click on empty map (null) leaving the inspector open.
  test("a map onSelect(null) closes the inspector", () => {
    mount();
    mapSelect(1);
    mapSelect(null);
    expect(inspector()).toBeNull();
  });

  // Catches: the inspector's Close and onSelect(null) not reaching the map.
  test("the inspector Close clears the map's selectedId", () => {
    mount();
    mapSelect(1);
    fireEvent.click(within(inspector() as HTMLElement).getByRole("button", { name: "Close" }));
    expect(lastMapProps().selectedId ?? null).toBeNull();
  });

  // Catches: the session guard's onSelect(null) not wired to the shell state (a stale selection after a reload).
  test("a new session clears the selection", () => {
    const d = mount();
    mapSelect(1);
    const next = wireFrame(5, ENTITIES);
    d.send({ ...next, session: 2, run: { ...next.run, session: 2 } });
    expect(lastMapProps().selectedId ?? null).toBeNull();
    expect(inspector()).toBeNull();
  });
});

describe("centre on map", () => {
  // Catches: onCenter not wired to the map handle (`mapRef.current?.centerOn`).
  test("Center on map calls centerOn on the map handle with the selected id", () => {
    mount();
    mapSelect(2);
    fireEvent.click(
      within(inspector() as HTMLElement).getByRole("button", { name: "Center on map" }),
    );
    expect(centerOnCalls).toEqual([2]);
  });

  // Catches: a throw when the map says the entity is not there (false is an answer, not an error).
  test("a false answer from the map changes nothing", () => {
    mount();
    setCenterOnResult(false);
    mapSelect(2);
    fireEvent.click(screen.getByRole("button", { name: "Center on map" }));
    expect(centerOnCalls).toEqual([2]);
    expect(lastMapProps().selectedId).toBe(2);
  });
});

describe("affiliation (settings)", () => {
  const stored = { version: 1, map: { affiliationOverrides: { red: "hostile", blue: "friend" } } };

  // Catches: the map keeping its own default instead of the settings function.
  test("the map gets useAffiliationOf(): an override applies, the default table is the fallback", () => {
    mount({ version: 1, map: { affiliationOverrides: { zebra: "neutral" } } });
    const affiliationOf = lastMapProps().affiliationOf;
    expect(affiliationOf?.("zebra")).toBe("neutral");
    expect(affiliationOf?.("ZEBRA ")).toBe("neutral");
    expect(affiliationOf?.(null)).toBe("unknown");
  });

  // Catches: the list not receiving it (rows carry the affiliation word for screen readers).
  test("the list rows use the same affiliation function", () => {
    mount({ version: 1, map: { affiliationOverrides: { blue: "hostile", red: "friend" } } });
    expect(options()[0]).toHaveTextContent(/hostile/i); // Alpha is blue, overridden to hostile
    expect(options()[1]).toHaveTextContent(/friend/i); // Bravo is red, overridden to friend
  });

  // Catches: the inspector not receiving it.
  test("the inspector uses the same affiliation function", () => {
    mount(stored);
    mapSelect(2);
    expect(inspector()).toHaveTextContent(/hostile/i);
  });

  // Catches: a new function per render (the map applies it on the next flush, so it must stay stable).
  test("the function given to the map is stable across frames", () => {
    const d = mount(stored);
    const first = lastMapProps().affiliationOf;
    d.send(wireFrame(9, ENTITIES));
    expect(lastMapProps().affiliationOf).toBe(first);
  });
});

describe("recent scenarios", () => {
  const info = (path: string) =>
    json({
      session: 2,
      name: "scn",
      path,
      seed: "1",
      tick_rate_hz: 30,
      tick_rate_source: "scenario",
      duration_ticks: 10,
      entity_count: 0,
    });
  const loadPath = (path: string) => {
    fireEvent.change(screen.getByRole("textbox", { name: "Scenario path" }), {
      target: { value: path },
    });
    fireEvent.click(screen.getByRole("button", { name: "Load scenario" }));
  };
  const lastRecent = (d: Dashboard) =>
    (d.writes.at(-1) as { scenarios: { recent: string[] } } | undefined)?.scenarios.recent;

  // Catches: settings' recent paths not passed as `recentPaths`.
  test("the loader offers the stored recent paths", () => {
    mount({ version: 1, scenarios: { recent: ["a/one.toml", "b/two.toml"] } }, "idle", []);
    const select = screen.getByRole("combobox", { name: "Recent scenarios" });
    const labels = within(select)
      .getAllByRole("option")
      .map((o) => o.textContent);
    expect(labels).toEqual(expect.arrayContaining(["a/one.toml", "b/two.toml"]));
  });

  // Catches: onLoaded not persisting the path.
  test("a loaded scenario is stored as the newest recent path", async () => {
    engine.restore();
    engine = fakeFetch(() => info("scenarios/new.toml"));
    const d = mount({ version: 1, scenarios: { recent: ["old.toml"] } }, "idle", []);
    loadPath("scenarios/new.toml");
    await waitFor(() => expect(lastRecent(d)).toEqual(["scenarios/new.toml", "old.toml"]));
  });

  // Catches: duplicates in the list (a reloaded path moves to the front, it is not added twice).
  test("loading a path that is already recent moves it to the front without a duplicate", async () => {
    engine.restore();
    engine = fakeFetch(() => info("b.toml"));
    const d = mount({ version: 1, scenarios: { recent: ["a.toml", "b.toml"] } }, "idle", []);
    loadPath("b.toml");
    await waitFor(() => expect(lastRecent(d)).toEqual(["b.toml", "a.toml"]));
  });

  // Catches: a failed load being remembered.
  test("a rejected load stores nothing", async () => {
    engine.restore();
    engine = fakeFetch(() => problem(422, "scenario_invalid"));
    const d = mount(undefined, "idle", []);
    loadPath("bad.toml");
    await waitFor(() => expect(engine.calls.some((c) => c.method === "POST")).toBe(true));
    await Promise.resolve();
    expect(d.writes).toEqual([]);
  });

  // Catches: the Panel opened from the bar staying open over the map after a successful load (ASM-975).
  test("a successful load closes the Scenario panel opened from the bar", async () => {
    engine.restore();
    engine = fakeFetch(() => info("x.toml"));
    mount(undefined, "running");
    fireEvent.click(screen.getByRole("button", { name: "Scenario" }));
    loadPath("x.toml");
    await waitFor(() => expect(screen.queryByRole("region", { name: "Scenario" })).toBeNull());
  });
});

describe("no fetch outside the @umbra/data hooks", () => {
  const SRC = resolve(import.meta.dir, "../..");
  const sources = (dir: string): string[] =>
    readdirSync(dir).flatMap((name) => {
      const path = join(dir, name);
      if (statSync(path).isDirectory()) return sources(path);
      return /\.(ts|tsx)$/.test(name) && !/\.test\.|^testing\./.test(name) ? [path] : [];
    });

  // Catches: a direct request in the shell (gui/CLAUDE.md rule 2: shells hold no fetch calls).
  test("no shell source calls fetch, XMLHttpRequest or WebSocket, or imports a generated client", () => {
    const offenders = sources(SRC).filter((file) =>
      /\bfetch\s*\(|XMLHttpRequest|new\s+WebSocket|generated\//.test(readFileSync(file, "utf8")),
    );
    expect(offenders).toEqual([]);
  });

  const urls = (calls: FetchCall[]) => calls.map((c) => `${c.method} ${c.url}`);

  // Catches: the shell requesting anything of its own while idle.
  test("an idle dashboard makes no request", () => {
    mount(undefined, "idle", []);
    expect(urls(engine.calls)).toEqual([]);
  });

  // Catches: requests other than the inspector's detail read, which goes through useGetEntity.
  test("with every panel mounted and an entity selected, the only request is that entity's detail", () => {
    mount();
    mapSelect(1);
    expect(urls(engine.calls)).toEqual(["GET /api/v1/entities/1"]);
  });
});
