// WI-17 (docs/plans/M8.md): the settings page under the providers the app mounts, with a fake engine behind
// `fetch` (`GET /status`), a real stream connection over a recording fake socket and a recording settings backend.
// Written before the page. Contract:
// - route `/settings`; docked `CommandBar` (AppNav + ThemeToggle "Day theme", no visible title, ASM-993 n3), one
//   `main` with a visually hidden `h1` "Settings" and a visually hidden `h2` "App settings" (axe heading-order:
//   the Cards render `h3`);
// - seven Cards in this order: Engine connection, Appearance, Live data, Map, Log, Scenarios, About;
// - the page does not depend on a scenario: it shows every section in the idle state (no idle state);
// - the shell holds no logic: each section lays out an editor of `@umbra/settings` or `@umbra/desktop`
//   (EngineConnectionSettings, ThemePreferenceSetting, StreamRateSetting, AffiliationOverridesEditor,
//   JournalCapacitySetting, RecentScenariosSetting), tested in those packages; here the wiring is tested;
// - the stored stream rate is applied by the app (`StreamRateApplier`, tested in src/), not by this page (ASM-997);
// - Appearance has a second switch labelled "Use the Day theme" (the bar's is "Day theme");
// - About: a description list with "API version" (`api_version` of `GET /status`, "Unavailable" when that fails),
//   "App version" (a semver) and a link "Design showcase" to `/showcase`.
import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { cleanup, fireEvent, screen, within } from "@testing-library/react";
import {
  API_VERSION,
  type EngineScript,
  fakeEngine,
  type MountOptions,
  mountSettingsPage,
  settle,
} from "../../testing.tsx";
import SettingsPage from "./SettingsPage.tsx";

let engine: ReturnType<typeof fakeEngine> | undefined;
beforeEach(() => {
  window.localStorage.clear();
});
afterEach(() => {
  cleanup();
  engine?.restore();
  engine = undefined;
  window.localStorage.clear();
});

async function open(options: MountOptions = {}, script: EngineScript = {}) {
  engine = fakeEngine(script, options.state ?? "ready");
  const mounted = mountSettingsPage(SettingsPage, options);
  await settle();
  return mounted;
}

const SECTIONS = [
  "Engine connection",
  "Appearance",
  "Live data",
  "Map",
  "Log",
  "Scenarios",
  "About",
];
/** The Card with the given title: its heading's parent. */
const card = (title: string): HTMLElement => {
  const parent = screen.getByRole("heading", { level: 3, name: title }).parentElement;
  if (!parent) throw new Error(`no card ${title}`);
  return parent;
};

describe("SettingsPage landmarks", () => {
  test("one banner holding the page navigation and the theme switch, one main with the page heading", async () => {
    await open();
    const banner = screen.getByRole("banner");
    expect(screen.getAllByRole("banner")).toHaveLength(1);
    expect(within(banner).getByRole("navigation", { name: "Pages" })).toBeInTheDocument();
    expect(within(banner).getByRole("switch", { name: "Day theme" })).toBeInTheDocument();
    const main = screen.getByRole("main");
    expect(screen.getAllByRole("main")).toHaveLength(1);
    expect(within(main).getByRole("heading", { level: 1, name: "Settings" })).toBeInTheDocument();
  });

  // Catches: the page title repeated in the bar next to the current navigation link (ASM-989, ASM-993 n3).
  test("the bar has no visible title; the page heading is hidden and the nav link names the page", async () => {
    await open();
    const banner = screen.getByRole("banner");
    expect(banner.querySelector(".vt-cmdbar-title b")?.textContent).toBe("");
    expect(within(banner).queryByText("Settings", { selector: "b" })).toBeNull();
    expect(screen.getByRole("heading", { level: 1, name: "Settings" })).toHaveClass(
      "vt-visually-hidden",
    );
  });

  // Catches: the h3 card titles under the h1 with no h2 between (axe heading-order).
  test("a hidden h2 sits between the page heading and the cards", async () => {
    await open();
    const h2 = screen.getByRole("heading", { level: 2 });
    expect(h2).toHaveClass("vt-visually-hidden");
    expect(h2).toHaveTextContent("App settings");
  });

  test("the navigation marks the Settings page as current", async () => {
    await open();
    const nav = screen.getByRole("navigation", { name: "Pages" });
    expect(within(nav).getByRole("link", { name: /Settings/ })).toHaveAttribute(
      "aria-current",
      "page",
    );
    for (const other of [/Live map/, /Templates/, /Plugins/])
      expect(within(nav).getByRole("link", { name: other })).not.toHaveAttribute("aria-current");
  });

  test("the seven sections are Cards in the planned order, inside main", async () => {
    await open();
    const titles = within(screen.getByRole("main"))
      .getAllByRole("heading", { level: 3 })
      .map((h) => h.textContent);
    expect(titles).toEqual(SECTIONS);
  });

  test("a loaded page has no filled Button", async () => {
    await open({ stored: { scenarios: { recent: ["a.yaml"] } } });
    expect(document.querySelectorAll(".vt-btn-filled")).toHaveLength(0);
  });
});

describe("SettingsPage without a scenario", () => {
  // Catches: gating the page like the scenario pages (settings are app-level).
  test("idle shows every section and no idle state", async () => {
    await open({ state: "idle" });
    expect(screen.queryByText(/load a scenario/i)).toBeNull();
    expect(within(screen.getByRole("main")).getAllByRole("heading", { level: 3 })).toHaveLength(7);
  });

  test("idle still reads the API version for About", async () => {
    await open({ state: "idle" });
    expect(within(card("About")).getByText(API_VERSION)).toBeInTheDocument();
  });
});

describe("Engine connection", () => {
  test("shows the engine URL of the runtime config, read-only", async () => {
    await open({ engineUrl: "http://engine.test:8080" });
    const field = within(card("Engine connection")).getByRole("textbox", { name: "Engine URL" });
    expect(field).toHaveValue("http://engine.test:8080");
    expect(field).toHaveAttribute("readonly");
  });
});

describe("Appearance", () => {
  test("the theme preference Select offers Dark, Light and System", async () => {
    await open();
    const select = within(card("Appearance")).getByRole("combobox", { name: "Theme preference" });
    expect(
      within(select)
        .getAllByRole("option")
        .map((o) => o.textContent),
    ).toEqual(["Dark", "Light", "System"]);
    expect(select).toHaveValue("dark");
  });

  test("choosing Light switches the theme and the bar's switch follows", async () => {
    await open();
    const select = within(card("Appearance")).getByRole("combobox", { name: "Theme preference" });
    fireEvent.change(select, { target: { value: "light" } });
    expect(window.localStorage.getItem("umbra.theme")).toBe("light");
    expect(document.documentElement).toHaveAttribute("data-theme", "light");
    expect(
      within(screen.getByRole("banner")).getByRole("switch", { name: "Day theme" }),
    ).toBeChecked();
  });

  test("the section's own switch is named 'Use the Day theme' and the bar's stays 'Day theme'", async () => {
    await open();
    expect(screen.getAllByRole("switch")).toHaveLength(2);
    const own = within(card("Appearance")).getByRole("switch", { name: "Use the Day theme" });
    expect(own).not.toBeChecked();
    fireEvent.click(own);
    expect(own).toBeChecked();
    expect(
      within(screen.getByRole("banner")).getByRole("switch", { name: "Day theme" }),
    ).toBeChecked();
    expect(
      within(card("Appearance")).getByRole("combobox", { name: "Theme preference" }),
    ).toHaveValue("light");
  });
});

describe("Live data", () => {
  test("the stream rate slider shows the stored rate", async () => {
    await open({ stored: { stream: { maxRateHz: 12 } } });
    const slider = within(card("Live data")).getByRole("slider", { name: "Stream rate" });
    expect(slider).toHaveValue("12");
    expect(slider).toHaveAttribute("min", "1");
    expect(slider).toHaveAttribute("max", "60");
  });

  test("a change is persisted and the page itself sends no subscribe (the app does, ASM-997)", async () => {
    const { writes, subscribes } = await open({ stored: { stream: { maxRateHz: 12 } } });
    const before = subscribes().length; // the connection's own default subscribe
    fireEvent.change(within(card("Live data")).getByRole("slider", { name: "Stream rate" }), {
      target: { value: "45" },
    });
    expect(writes.at(-1)?.stream.maxRateHz).toBe(45);
    expect(subscribes()).toHaveLength(before);
  });
});

describe("Map", () => {
  test("an added override is persisted and recolours through useAffiliationOf", async () => {
    const { writes } = await open();
    expect(screen.getByLabelText("Affiliation of blue")).toHaveTextContent("friend");
    const map = card("Map");
    fireEvent.click(within(map).getByRole("button", { name: "Add override" }));
    const row = within(map)
      .getAllByRole("row")
      .find((r) => within(r).queryByRole("textbox", { name: "Force" }));
    if (!row) throw new Error("no override row");
    fireEvent.change(within(row).getByRole("combobox", { name: "Affiliation" }), {
      target: { value: "hostile" },
    });
    const force = within(row).getByRole("textbox", { name: "Force" });
    fireEvent.change(force, { target: { value: "Blue" } });
    fireEvent.blur(force);
    expect(writes.at(-1)?.map.affiliationOverrides).toEqual({ blue: "hostile" });
    expect(screen.getByLabelText("Affiliation of blue")).toHaveTextContent("hostile");
  });

  test("stored overrides are listed and in effect", async () => {
    await open({ stored: { map: { affiliationOverrides: { blue: "neutral" } } } });
    const force = within(card("Map")).getByRole("textbox", { name: "Force" });
    expect(force).toHaveValue("blue");
    expect(screen.getByLabelText("Affiliation of blue")).toHaveTextContent("neutral");
  });

  test("an invalid row is flagged and not saved", async () => {
    const { writes } = await open();
    const map = card("Map");
    fireEvent.click(within(map).getByRole("button", { name: "Add override" }));
    const force = within(map).getByRole("textbox", { name: "Force" });
    fireEvent.change(force, { target: { value: "constructor" } });
    fireEvent.blur(force);
    expect(force).toHaveAttribute("aria-invalid", "true");
    expect(force).toHaveAccessibleDescription(/reserved name/i);
    expect(writes).toHaveLength(0);
    expect(screen.getByLabelText("Affiliation of blue")).toHaveTextContent("friend");
  });
});

describe("Log", () => {
  test("the journal capacity is persisted on blur", async () => {
    const { writes } = await open();
    const field = within(card("Log")).getByRole("spinbutton", { name: "Journal capacity" });
    expect(field).toHaveValue(5000);
    fireEvent.change(field, { target: { value: "20000" } });
    fireEvent.blur(field);
    expect(writes.at(-1)?.journal.capacity).toBe(20_000);
  });

  test("an out-of-range capacity is flagged and not saved", async () => {
    const { writes } = await open();
    const field = within(card("Log")).getByRole("spinbutton", { name: "Journal capacity" });
    fireEvent.change(field, { target: { value: "999" } });
    fireEvent.blur(field);
    expect(field).toHaveAttribute("aria-invalid", "true");
    expect(writes).toHaveLength(0);
  });
});

describe("Scenarios", () => {
  test("clears the recent list and persists it", async () => {
    const { writes } = await open({ stored: { scenarios: { recent: ["a.yaml", "b.yaml"] } } });
    const scenarios = card("Scenarios");
    expect(within(scenarios).getByText("2 recent scenarios")).toBeInTheDocument();
    fireEvent.click(within(scenarios).getByRole("button", { name: "Clear recent scenarios" }));
    expect(writes.at(-1)?.scenarios.recent).toEqual([]);
    expect(within(scenarios).getByText("No recent scenarios")).toBeInTheDocument();
  });
});

describe("About", () => {
  test("lists the API version of the status, an app version and a link to the showcase", async () => {
    await open();
    const about = card("About");
    const apiVersion = within(about).getByText("API version");
    expect(apiVersion.tagName).toBe("DT");
    expect(apiVersion.nextElementSibling).toHaveTextContent(API_VERSION);
    const appVersion = within(about).getByText("App version");
    expect(appVersion.tagName).toBe("DT");
    expect(appVersion.nextElementSibling?.textContent).toMatch(/^\d+\.\d+\.\d+/);
    expect(within(about).getByRole("link", { name: "Design showcase" })).toHaveAttribute(
      "href",
      "/showcase",
    );
  });

  // Catches: showing the stream's hello version (1.0.0 in the double) instead of the status.
  test("the API version comes from GET /status", async () => {
    await open();
    expect(engine?.paths).toContain("/api/v1/status");
    expect(within(card("About")).queryByText("1.0.0")).toBeNull();
  });

  test("when the status cannot be read the API version says Unavailable and the rest still shows", async () => {
    await open({}, { statusFails: true });
    const about = card("About");
    expect(within(about).getByText("API version").nextElementSibling).toHaveTextContent(
      "Unavailable",
    );
    expect(within(about).getByRole("link", { name: "Design showcase" })).toBeInTheDocument();
  });
});
