// WI-14 review round: the bar's two popovers (Clock, Scenario), their exclusivity and keyboard behaviour, the
// compact bar, the step group in the clock panel and the command problem in the alert column. New file; the
// WI-14 and M5b tests are untouched. Written before the implementation (ASM-985..).
import { afterEach, beforeEach, describe, expect, mock, test } from "bun:test";
import { act, cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";
import {
  type Dashboard,
  fakeFetch,
  installResizeObserver,
  json,
  LiveMapStub,
  lastMapProps,
  mountDashboard,
  problem,
  type RunState,
  resetMapStub,
  wireEntity,
  wireFrame,
} from "../../testing.tsx";

mock.module("@umbra/map-view", () => ({ LiveMap: LiveMapStub }));
const { default: DashboardPage } = await import("./DashboardPage.tsx");

let restoreObserver: () => void;
let engine: ReturnType<typeof fakeFetch>;
let page: Dashboard | undefined;
/** The faked bar (no layout in happy-dom): its width and the width its content needs at each level (ASM-988). */
let barClient = 0;
let barWanted: [number, number, number] = [1400, 1250, 1000];
const levelOf = (bar: HTMLElement): 0 | 1 | 2 =>
  bar.querySelector("b")?.textContent?.trim()
    ? 0
    : bar.querySelector('[aria-label="Load"]')
      ? 2
      : 1;
const proto = HTMLElement.prototype;
const realScroll = Object.getOwnPropertyDescriptor(proto, "scrollWidth");
const realClient = Object.getOwnPropertyDescriptor(proto, "clientWidth");
beforeEach(() => {
  resetMapStub();
  restoreObserver = installResizeObserver(400);
  engine = fakeFetch();
  barClient = 0;
  Object.defineProperty(proto, "clientWidth", {
    configurable: true,
    get(this: HTMLElement) {
      return this.classList.contains("vt-cmdbar") ? barClient : 0;
    },
  });
  Object.defineProperty(proto, "scrollWidth", {
    configurable: true,
    get(this: HTMLElement) {
      return this.classList.contains("vt-cmdbar")
        ? Math.max(barClient, barWanted[levelOf(this)])
        : 0;
    },
  });
});
afterEach(() => {
  cleanup();
  page?.close();
  page = undefined;
  engine.restore();
  restoreObserver();
  if (realScroll) Object.defineProperty(proto, "scrollWidth", realScroll);
  else Reflect.deleteProperty(proto, "scrollWidth");
  if (realClient) Object.defineProperty(proto, "clientWidth", realClient);
  else Reflect.deleteProperty(proto, "clientWidth");
});

const mount = (
  state: RunState = "running",
  clock: Record<string, unknown> = { mode: "realtime" },
) => {
  page = mountDashboard(DashboardPage, {
    preload: { entities: [wireEntity(1, { name: "Alpha" })], state, clock },
  });
  return page;
};
const banner = (): HTMLElement =>
  screen.getAllByRole("banner").find((el) => el.classList.contains("vt-cmdbar")) as HTMLElement;
const clockButton = () => within(banner()).getByRole("button", { name: "Clock" });
const scenarioButton = () => within(banner()).getByRole("button", { name: "Scenario" });
const scenarioPanel = () => screen.queryByRole("region", { name: "Scenario" });
const clockPanel = () => document.getElementById(clockButton().getAttribute("aria-controls") ?? "");
const filled = () => [...document.querySelectorAll("button.vt-btn-filled")];
const openScenario = () => fireEvent.click(scenarioButton());
const openClock = () => fireEvent.click(clockButton());
const floatingPanels = () =>
  [
    scenarioPanel(),
    clockButton().getAttribute("aria-expanded") === "true" ? clockPanel() : null,
  ].filter(Boolean);

describe("one filled Button with a popover open", () => {
  // Catches: the loader's submit (or anything in the popover) adding a second azure Button (ScenarioPanel submitVariant).
  test("the Scenario panel open while running", () => {
    mount("running");
    openScenario();
    expect(scenarioPanel()).not.toBeNull();
    expect(filled().map((b) => b.textContent)).toHaveLength(1);
  });

  // Catches: the Clock panel (ClockControl chips, step Buttons) adding a filled Button.
  test("the Clock panel open while running a stepped clock", () => {
    mount("running", { mode: "stepped" });
    openClock();
    expect(filled()).toHaveLength(1);
  });
});

describe("where the Scenario panel sits", () => {
  // Catches: the panel floating at its own spot and overlapping the alerts (it is the last child of the alert column).
  test("it is the last child of the alert column, after the alerts", () => {
    mount("running");
    openScenario();
    const column = screen.getByRole("main").querySelector(".dashboard-alert") as HTMLElement;
    const panel = scenarioPanel() as HTMLElement;
    expect(column).toContainElement(panel);
    expect(column.lastElementChild).toContainElement(panel);
  });
});

describe("the popovers are exclusive", () => {
  // Catches: three or more floating panels at once.
  test("opening the Scenario panel closes the Clock panel and the reverse", () => {
    mount("running");
    openClock();
    expect(clockButton()).toHaveAttribute("aria-expanded", "true");
    openScenario();
    expect(scenarioPanel()).not.toBeNull();
    expect(clockButton()).toHaveAttribute("aria-expanded", "false");
    openClock();
    expect(scenarioPanel()).toBeNull();
    expect(clockButton()).toHaveAttribute("aria-expanded", "true");
    expect(floatingPanels()).toHaveLength(1);
  });

  // Catches: left panel, inspector, scenario and clock all open (more than three floating panels).
  test("with the left panel and the inspector open there are at most three", () => {
    mount("running");
    act(() => lastMapProps().onSelect?.(1));
    openScenario();
    openClock();
    openScenario();
    const asides = screen.getAllByRole("complementary").length; // the left panel and the inspector
    expect(asides).toBe(2);
    expect(asides + floatingPanels().length).toBeLessThanOrEqual(3);
  });
});

describe("popover accessibility", () => {
  // Catches: aria-controls pointing nowhere or a hand-made id.
  test("aria-controls of each button names an element that exists when it is open", () => {
    mount("running");
    openClock();
    const clockId = clockButton().getAttribute("aria-controls") as string;
    expect(clockId).toBeTruthy();
    expect(document.getElementById(clockId)).not.toBeNull();
    openScenario();
    const scenarioId = scenarioButton().getAttribute("aria-controls") as string;
    expect(scenarioId).toBeTruthy();
    expect(document.getElementById(scenarioId)).toContainElement(scenarioPanel());
    expect(scenarioId).not.toBe(clockId);
  });

  // Catches: focus staying on the button (a keyboard user cannot reach the panel in the DOM order after the map).
  test("opening moves focus into the Scenario panel, closing returns it to the button", () => {
    mount("running");
    openScenario();
    expect(scenarioPanel()).toContainElement(document.activeElement as HTMLElement);
    fireEvent.keyDown(document.activeElement as HTMLElement, { key: "Escape" });
    expect(scenarioPanel()).toBeNull();
    expect(document.activeElement).toBe(scenarioButton());
  });

  test("opening moves focus into the Clock panel, Escape closes it and returns focus to the button", () => {
    mount("running");
    openClock();
    expect(clockPanel()).toContainElement(document.activeElement as HTMLElement);
    fireEvent.keyDown(document.activeElement as HTMLElement, { key: "Escape" });
    expect(clockButton()).toHaveAttribute("aria-expanded", "false");
    expect(document.activeElement).toBe(clockButton());
  });

  // Catches: Escape closing a popover that is not open or stealing other keys.
  test("other keys do nothing", () => {
    mount("running");
    openScenario();
    fireEvent.keyDown(document.activeElement as HTMLElement, { key: "a" });
    expect(scenarioPanel()).not.toBeNull();
  });

  // Catches: the toggle button not closing the panel it opened.
  test("the button toggles and focus stays on it", () => {
    mount("running");
    openScenario();
    fireEvent.click(scenarioButton());
    expect(scenarioPanel()).toBeNull();
    expect(document.activeElement).toBe(scenarioButton());
  });
});

describe("the compact steps follow the measured bar (ASM-988)", () => {
  // Catches: compact mode for a bar that fits, or for an unlaid-out page.
  test.each([0, 1500])("a bar %d px wide that fits has the title and labelled buttons", (w) => {
    barClient = w;
    mount("running");
    expect(within(banner()).getByText("Live map", { selector: "b" })).toBeInTheDocument();
    expect(clockButton()).toHaveTextContent("Clock");
    expect(scenarioButton()).toHaveTextContent("Scenario");
    expect(within(banner()).getByRole("switch", { name: "Day theme" })).toHaveTextContent(
      "Day theme",
    );
    expect(within(banner()).getByRole("button", { name: "Start" })).toHaveTextContent("Start");
  });

  // Catches: the duplicated page title (AppNav already shows "Live map"), text on the icon buttons.
  test("step 1: the title is dropped and Clock, Scenario and the theme switch are icon-only", () => {
    barClient = 1300; // the full bar needs 1400, the compact one 1250
    mount("running");
    expect(within(banner()).queryByText("Live map", { selector: "b" })).toBeNull();
    expect(
      within(within(banner()).getByRole("navigation", { name: "Pages" })).getByRole("link"),
    ).toHaveTextContent("Live map");
    for (const b of [clockButton(), scenarioButton()]) {
      expect(b).toHaveAttribute("aria-label");
      expect(b.textContent?.replace(/\s/g, "")).toMatch(/^[a-z_]*$/); // only the icon ligature
    }
    const toggle = within(banner()).getByRole("switch", { name: "Day theme" });
    expect(toggle).not.toHaveTextContent("Day theme");
    expect(banner().querySelector(".vt-theme-toggle .vt-icon")).not.toBeNull();
    // the lifecycle buttons keep their labels at this step
    expect(within(banner()).getByRole("button", { name: "Start" })).toHaveTextContent("Start");
  });

  // Catches: no second step when the compact bar still overflows.
  test("step 2: the lifecycle buttons become icons with a name and a tooltip", () => {
    barClient = 1100;
    mount("running");
    const start = within(banner()).getByRole("button", { name: "Start" });
    expect(start).toHaveAttribute("title", "Start");
    expect(start.textContent?.trim()).toMatch(/^[a-z_]+$/);
    expect(clockButton()).toHaveAttribute("aria-label", "Clock");
  });

  // Catches: the step being decided by the viewport width instead of the content (the 1366 and 1440 px defect).
  test("the same viewport needs a step or not depending on the content width", () => {
    barClient = 1450;
    barWanted = [1400, 1250, 1000];
    mount("running");
    expect(within(banner()).getByText("Live map", { selector: "b" })).toBeInTheDocument();
    cleanup();
    barWanted = [1600, 1250, 1000]; // wide readouts
    mount("running");
    expect(within(banner()).queryByText("Live map", { selector: "b" })).toBeNull();
  });
});

describe("the Scenario popover and a remote load (review round 2)", () => {
  // Catches: the panel coming back when the run goes idle and then ready again (another client loaded a scenario).
  test("it closes when the run becomes idle and stays closed after the next ready", () => {
    const d = mount("running");
    openScenario();
    expect(scenarioPanel()).not.toBeNull();
    d.send(wireFrame(50, [], "idle"));
    expect(scenarioPanel()).toBeNull();
    d.send(wireFrame(51, [], "ready"));
    expect(scenarioPanel()).toBeNull();
    expect(scenarioButton()).toHaveAttribute("aria-expanded", "false");
    expect(document.activeElement).not.toBe(scenarioButton());
  });
});

describe("Escape in a form control", () => {
  // Catches: a focused <select> (Clock override) swallowing the key.
  test("Escape on a select inside the Scenario panel closes it", () => {
    mount("running");
    openScenario();
    const select = within(scenarioPanel() as HTMLElement).getByRole("combobox", {
      name: "Clock override",
    });
    select.focus();
    fireEvent.keyDown(select, { key: "Escape" });
    expect(scenarioPanel()).toBeNull();
    expect(document.activeElement).toBe(scenarioButton());
  });
});

describe("the step group and the command problem", () => {
  // Catches: the step row staying in the 48 px bar (it went off-screen).
  test("a running stepped clock shows Step in the Clock panel, not in the bar's Run controls", () => {
    mount("running", { mode: "stepped" });
    const controls = within(banner()).getByRole("group", { name: "Run controls" });
    expect(within(controls).queryByRole("button", { name: /^Step/ })).toBeNull();
    expect(within(banner()).queryAllByRole("group", { name: "Step" })).toHaveLength(0);
    openClock();
    expect(
      within(clockPanel() as HTMLElement).getByRole("group", { name: "Step" }),
    ).toBeInTheDocument();
  });

  // Catches: a failed command alert rendered inside the bar (overflow) or lost.
  test("a failed command shows its Alert in the alert column under the bar", async () => {
    engine.restore();
    engine = fakeFetch(() => problem(409, "invalid_state"));
    mount("running");
    fireEvent.click(within(banner()).getByRole("button", { name: "Stop" }));
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("invalid_state");
    expect(alert.closest(".dashboard-alert")).not.toBeNull();
    expect(banner()).not.toContainElement(alert);
  });

  // Catches: lifecycle and step errors not sharing one slot in the dashboard (the README: "until the next command").
  test("a failed step shows once in the alert column and the next lifecycle command clears it", async () => {
    let n = 0;
    engine.restore();
    engine = fakeFetch(() =>
      n++ === 0 ? problem(409, "step_cancelled") : json({ state: "running" }),
    );
    mount("running", { mode: "stepped" });
    openClock();
    fireEvent.click(
      within(clockPanel() as HTMLElement).getByRole("button", { name: "Step 1 tick" }),
    );
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("step_cancelled");
    expect(alert.closest(".dashboard-alert")).not.toBeNull();
    expect(screen.getAllByRole("alert")).toHaveLength(1);
    fireEvent.click(within(banner()).getByRole("button", { name: "Pause" }));
    await waitFor(() => expect(screen.queryByRole("alert")).toBeNull());
  });

  // Catches: the loader reachable only in idle (a successful load closes the panel; covered in the wiring file).
  test("the Scenario panel still loads a scenario", async () => {
    engine.restore();
    engine = fakeFetch(() =>
      json({
        session: 2,
        name: "s",
        path: "x.toml",
        seed: "1",
        tick_rate_hz: 30,
        tick_rate_source: "scenario",
        duration_ticks: 1,
        entity_count: 0,
      }),
    );
    mount("running");
    openScenario();
    fireEvent.change(screen.getByRole("textbox", { name: "Scenario path" }), {
      target: { value: "x.toml" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Load scenario" }));
    await waitFor(() => expect(scenarioPanel()).toBeNull());
  });
});
