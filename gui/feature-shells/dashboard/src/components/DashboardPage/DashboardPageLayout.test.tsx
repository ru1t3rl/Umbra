// WI-14 failing tests, layout half (skill write-frontend-tests; ADR 0078, docs/plans/M8.md "Dashboard layout").
// Written before the implementation. The pinned DashboardPage.test.tsx (M5b) is not edited here; see ASM-969.
// Assumed markup, from the plan:
//   - the CommandBar (banner) holds, in this order: AppNav ("Pages"), the run status, RunControls (group
//     "Run controls"), ClockControl (compact), the theme switch;
//   - the left floating panel is an `<aside aria-label="Entities">` with Tabs "Entities" and "Log"; it is
//     collapsible through one button that carries `aria-expanded` (ASM-973);
//   - the right panel is an `<aside aria-label="Inspector">`, mounted only while an entity is selected;
//   - the centre: while the run is `idle`, a Card holds ScenarioLoader; otherwise a "Scenario" button in the
//     bar opens it in a Panel named "Scenario" (ASM-970, ASM-971).
import { afterEach, beforeEach, describe, expect, mock, test } from "bun:test";
import { act, cleanup, fireEvent, screen, within } from "@testing-library/react";
import {
  type Dashboard,
  fakeFetch,
  installResizeObserver,
  LiveMapStub,
  lastMapProps,
  mountDashboard,
  type RunState,
  resetMapStub,
  wireEntity,
} from "../../testing.tsx";

mock.module("@umbra/map-view", () => ({ LiveMap: LiveMapStub }));
const { default: DashboardPage } = await import("./DashboardPage.tsx");

let restoreObserver: () => void;
let restoreFetch: () => void;
let page: Dashboard | undefined;
beforeEach(() => {
  resetMapStub();
  restoreObserver = installResizeObserver(400);
  restoreFetch = fakeFetch().restore;
});
afterEach(() => {
  cleanup();
  page?.close();
  page = undefined;
  restoreFetch();
  restoreObserver();
});

const mount = (state?: RunState, entities: unknown[] = [wireEntity(1, { name: "Alpha" })]) => {
  page = mountDashboard(DashboardPage);
  if (state) page.start(entities, state);
  return page;
};
const select = (id: number | null) => act(() => lastMapProps().onSelect?.(id));
/**
 * The page banner. Testing Library maps every `header` to the banner role, including the Panel headers inside
 * the asides (a browser does not, per HTML-AAM), so the banner is the CommandBar's header and "exactly one" is
 * asserted on those: one floating CommandBar, not nested in a sectioning element. axe checks the real
 * landmarks in e2e (dashboard-layout.spec.ts).
 */
const banner = (): HTMLElement => {
  const bars = screen.getAllByRole("banner").filter((el) => el.classList.contains("vt-cmdbar"));
  expect(bars).toHaveLength(1);
  return bars[0] as HTMLElement;
};
const filledButtons = () => [...document.querySelectorAll("button.vt-btn-filled")];

describe("landmarks", () => {
  // Catches: the bar rendered inside main (it would stop being the banner) or a second banner.
  test("one banner (the CommandBar, outside every sectioning element), one main, one h1", () => {
    mount("running");
    expect(banner()).toHaveClass("vt-cmdbar-floating");
    expect(banner().closest("main, aside, section, article, nav")).toBeNull();
    expect(screen.getAllByRole("main")).toHaveLength(1);
    expect(screen.getAllByRole("heading", { level: 1 })).toHaveLength(1);
  });

  // Catches: AppNav missing from the bar (WI-3 made it a navigation landmark labelled "Pages").
  test("the banner holds the Pages navigation with the current page marked", () => {
    mount("running");
    const nav = within(banner()).getByRole("navigation", { name: "Pages" });
    expect(within(nav).getByRole("link", { name: /Live map/ })).toHaveAttribute(
      "aria-current",
      "page",
    );
  });

  // Catches: the list panel being a plain div (no landmark) or named differently.
  test("the left panel is a complementary landmark named Entities", () => {
    mount("running");
    expect(screen.getByRole("complementary", { name: "Entities" })).toBeInTheDocument();
    expect(screen.queryByRole("complementary", { name: "Inspector" })).toBeNull();
  });

  // Catches: the inspector mounted without a selection, or not a landmark.
  test("with an entity selected there are exactly two complementary panels, Entities and Inspector", () => {
    mount("running");
    select(1);
    const names = screen.getAllByRole("complementary").map((el) => el.getAttribute("aria-label"));
    expect(names).toEqual(["Entities", "Inspector"]);
    expect(
      within(screen.getByRole("complementary", { name: "Inspector" })).getByText("Alpha"),
    ).toBeInTheDocument();
  });

  // Catches: the panels placed inside main (the map is the page; the panels float beside it).
  test("the panels are not inside main and the map is", () => {
    mount("running");
    select(1);
    const main = screen.getByRole("main");
    expect(main).toContainElement(screen.getByTestId("map"));
    for (const panel of screen.getAllByRole("complementary")) {
      expect(main).not.toContainElement(panel);
    }
  });

  // Catches: the inspector staying open after Close (the shell decides what closing means, ADR 0078).
  test("closing the inspector removes the Inspector panel and clears the map selection", () => {
    mount("running");
    select(1);
    const inspector = screen.getByRole("complementary", { name: "Inspector" });
    fireEvent.click(within(inspector).getByRole("button", { name: "Close" }));
    expect(screen.queryByRole("complementary", { name: "Inspector" })).toBeNull();
    expect(screen.getByTestId("map")).toHaveAttribute("data-selected", "");
  });
});

describe("the bar", () => {
  const before = (a: Element, b: Element) =>
    Boolean(a.compareDocumentPosition(b) & Node.DOCUMENT_POSITION_FOLLOWING);

  // Catches: a different order (the plan fixes: nav, status, run controls, clock, theme).
  test("AppNav, run status, RunControls, ClockControl and the theme switch, in that order", () => {
    mount("running");
    const bar = banner();
    const nav = within(bar).getByRole("navigation", { name: "Pages" });
    const status = within(bar).getByText("Entities").closest("dl") as Element;
    const controls = within(bar).getByRole("group", { name: "Run controls" });
    const clock = within(bar).getByText(/^Clock: /);
    const theme = within(bar).getByRole("switch", { name: "Day theme" });
    expect(before(nav, status)).toBe(true);
    expect(before(status, controls)).toBe(true);
    expect(before(controls, clock)).toBe(true);
    expect(before(clock, theme)).toBe(true);
  });

  // Catches: Stop and Unload losing the danger variant (plan: they use `danger`).
  test("Stop and Unload are danger buttons", () => {
    mount("running");
    const bar = banner();
    expect(within(bar).getByRole("button", { name: "Stop" })).toHaveClass("vt-btn-danger");
    expect(within(bar).getByRole("button", { name: "Unload" })).toHaveClass("vt-btn-danger");
  });
});

describe("the left panel", () => {
  const entities = () => screen.getByRole("complementary", { name: "Entities" });

  // Catches: the Log tab missing, or the tabs unnamed.
  test("has the Tabs Entities and Log, Entities first and selected", () => {
    mount("running");
    const tabs = within(entities()).getAllByRole("tab");
    expect(tabs.map((tab) => tab.textContent?.trim())).toEqual(["Entities", "Log"]);
    expect(tabs[0]).toHaveAttribute("aria-selected", "true");
    expect(within(entities()).getByRole("listbox", { name: "Entities" })).toBeInTheDocument();
  });

  // Catches: both views mounted at once (the Log tab would still render a list under the Entities tab).
  test("the Log tab shows the log in place of the list", () => {
    mount("running");
    fireEvent.click(within(entities()).getByRole("tab", { name: "Log" }));
    expect(within(entities()).getByRole("tab", { name: "Log" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    expect(within(entities()).queryByRole("listbox", { name: "Entities" })).toBeNull();
    // the compact log: a Filters button instead of the search field, and the JSON Lines download
    expect(within(entities()).getByRole("button", { name: "Download" })).toBeInTheDocument();
    expect(within(entities()).queryByRole("searchbox")).toBeNull();
  });

  // Catches: no way to collapse (the 1280x800 crowding mitigation), or a toggle without aria-expanded.
  test("one button toggles it, and aria-expanded tells the state (ASM-973)", () => {
    mount("running");
    const toggle = within(entities()).getByRole("button", { name: /^(Collapse|Expand) panel$/ });
    const startedOpen = toggle.getAttribute("aria-expanded") === "true";
    expect(toggle).toHaveAttribute("aria-expanded");
    const tabsShown = () => within(entities()).queryAllByRole("tab").length > 0;
    expect(tabsShown()).toBe(startedOpen);
    fireEvent.click(toggle);
    const flipped = within(entities()).getByRole("button", { name: /^(Collapse|Expand) panel$/ });
    expect(flipped.getAttribute("aria-expanded") === "true").toBe(!startedOpen);
    expect(tabsShown()).toBe(!startedOpen);
    fireEvent.click(flipped);
    expect(tabsShown()).toBe(startedOpen);
  });

  // Catches: a collapsed panel that is gone (the landmark and the way back must stay).
  test("collapsed, it stays a landmark with its toggle", () => {
    mount("running");
    const toggle = within(entities()).getByRole("button", { name: /^(Collapse|Expand) panel$/ });
    if (toggle.getAttribute("aria-expanded") === "true") fireEvent.click(toggle);
    expect(screen.getByRole("complementary", { name: "Entities" })).toBeInTheDocument();
    expect(within(entities()).getByRole("button", { name: "Expand panel" })).toHaveAttribute(
      "aria-expanded",
      "false",
    );
  });
});

describe("the scenario loader", () => {
  const loaderField = () => screen.queryByRole("textbox", { name: "Scenario path" });

  // Catches: the loader missing when the engine is idle (nothing else tells the user what to do).
  test("an idle run shows the loader in a Card in the centre of the page", () => {
    mount("idle", []);
    const field = loaderField();
    expect(field).toBeInTheDocument();
    expect(screen.getByRole("main")).toContainElement(field);
    const card = field?.closest(".vt-card");
    expect(card).not.toBeNull();
    // never glass on glass: the Card is tonal and sits in no glass panel
    for (const panel of screen.getAllByRole("complementary")) {
      expect(panel).not.toContainElement(field);
    }
    expect(card?.closest(".vt-cmdbar-floating")).toBeNull();
  });

  // Catches: the loader staying up over a running simulation.
  test.each(["ready", "running", "paused", "stopped"] as const)(
    "a %s run shows no loader in the centre",
    (state) => {
      mount(state);
      expect(loaderField()).toBeNull();
    },
  );

  // Catches: showing a loader that would post to an engine whose state is unknown (ASM-970).
  test("before the first frame there is no loader (the run state is unknown)", () => {
    page = mountDashboard(DashboardPage, { unopened: true });
    expect(loaderField()).toBeNull();
  });

  // Catches: no way back to the loader once a scenario runs.
  test("outside idle, a Scenario button in the bar opens the loader in a Panel named Scenario", () => {
    mount("running");
    const bar = banner();
    fireEvent.click(within(bar).getByRole("button", { name: "Scenario" }));
    const panel = screen.getByRole("region", { name: "Scenario" });
    expect(within(panel).getByRole("textbox", { name: "Scenario path" })).toBeInTheDocument();
  });

  // Catches: a redundant Scenario button next to the loader Card.
  test("an idle run has no Scenario button", () => {
    mount("idle", []);
    expect(within(banner()).queryByRole("button", { name: "Scenario" })).toBeNull();
  });
});

describe("alerts", () => {
  // Catches: the connection alert moved out of main or above the bar (pinned by the M5b test too).
  test("the connection alert sits in main, below the bar", () => {
    page = mountDashboard(DashboardPage, { unopened: true });
    const status = screen.getByRole("status", { name: "Connecting to the engine" });
    expect(screen.getByRole("main")).toContainElement(status);
  });
});

describe("one filled azure button per view (Vantage rule)", () => {
  // Catches: two filled Buttons in a view. In `idle` RunControls fills "Load" and the loader would fill
  // "Load scenario" (ASM-972, decided: the dashboard passes `submitVariant="tonal"`).
  test.each(["idle", "ready", "running", "paused", "stopped"] as const)(
    "a %s run has exactly one filled Button",
    (state) => {
      mount(state, state === "idle" ? [] : undefined);
      expect(filledButtons().map((b) => b.textContent)).toHaveLength(1);
    },
  );

  // Catches: the filled one not being the next lifecycle command (ASM-809).
  test("it is the next lifecycle command", () => {
    mount("ready");
    expect(filledButtons().map((b) => b.textContent?.trim())).toEqual(["Start"]);
  });

  // Catches: the inspector or the panels adding a filled Button.
  test("an open inspector adds no filled Button", () => {
    mount("running");
    select(1);
    expect(filledButtons()).toHaveLength(1);
  });
});
