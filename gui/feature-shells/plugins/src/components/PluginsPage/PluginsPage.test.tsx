// WI-16 (docs/plans/M8.md): the plugins page under the providers the app mounts, with a fake engine behind
// `fetch` (a plugin with a faulted capability, a faulted adapter) and a real stream connection over a fake
// socket. Written before the page.
import { afterEach, describe, expect, test } from "bun:test";
import { cleanup, fireEvent, screen, within } from "@testing-library/react";
import {
  ADAPTERS,
  type EngineScript,
  fakeEngine,
  HEALTHY_PLUGINS,
  mountPlugins,
  settle,
} from "../../testing.tsx";
import PluginsPage from "./PluginsPage.tsx";

let engine: ReturnType<typeof fakeEngine> | undefined;
afterEach(() => {
  cleanup();
  engine?.restore();
  engine = undefined;
});

async function open(state = "ready", script: EngineScript = {}) {
  engine = fakeEngine(script);
  const mounted = mountPlugins(PluginsPage, state);
  await settle();
  return mounted;
}

const tab = (name: string): HTMLElement => screen.getByRole("tab", { name });

describe("PluginsPage landmarks", () => {
  test("one banner holding the page navigation and the theme switch, one main with the page heading", async () => {
    await open();
    const banner = screen.getByRole("banner");
    expect(screen.getAllByRole("banner")).toHaveLength(1);
    expect(within(banner).getByRole("navigation", { name: "Pages" })).toBeInTheDocument();
    expect(within(banner).getByRole("switch")).toBeInTheDocument();
    const main = screen.getByRole("main");
    expect(screen.getAllByRole("main")).toHaveLength(1);
    expect(within(main).getByRole("heading", { level: 1, name: "Plugins" })).toBeInTheDocument();
  });

  // Catches: the page title repeated in the bar next to the current navigation link (ASM-989, ASM-993 n3).
  test("the bar has no visible title; the page heading is hidden and the nav link names the page", async () => {
    await open();
    const banner = screen.getByRole("banner");
    expect(banner.querySelector(".vt-cmdbar-title b")?.textContent).toBe("");
    expect(within(banner).queryByText("Plugins", { selector: "b" })).toBeNull();
    expect(screen.getByRole("heading", { level: 1, name: "Plugins" })).toHaveClass(
      "vt-visually-hidden",
    );
  });

  // Catches: the h3 card titles under the h1 with no h2 between (axe heading-order); the h2 has its own text.
  test("a hidden h2 sits between the page heading and the cards", async () => {
    await open();
    const h2 = screen.getByRole("heading", { level: 2 });
    expect(h2).toHaveClass("vt-visually-hidden");
    expect(h2).toHaveTextContent("Plugin status");
  });

  test("the navigation marks the Plugins page as current", async () => {
    await open();
    const nav = screen.getByRole("navigation", { name: "Pages" });
    expect(within(nav).getByRole("link", { name: /Plugins/ })).toHaveAttribute(
      "aria-current",
      "page",
    );
    expect(within(nav).getByRole("link", { name: /Live map/ })).not.toHaveAttribute("aria-current");
    expect(within(nav).getByRole("link", { name: /Templates/ })).not.toHaveAttribute(
      "aria-current",
    );
  });
});

describe("PluginsPage fault summary", () => {
  test("lists the faulted capability and the faulted adapter", async () => {
    await open();
    const faults = screen.getByRole("list", { name: "Faults" });
    expect(within(faults).getAllByRole("listitem")).toHaveLength(2);
    expect(within(faults).getByText("Faulted capability")).toBeInTheDocument();
    expect(within(faults).getByText("acme.radar / acme.radar.track")).toBeInTheDocument();
    expect(within(faults).getByText("Faulted adapter")).toBeInTheDocument();
    expect(within(faults).getByText("hla-lab")).toBeInTheDocument();
    expect(within(faults).getByText("socket closed")).toBeInTheDocument();
  });

  test("sits above the tabs, in main, outside every tab panel", async () => {
    await open();
    const faults = screen.getByRole("list", { name: "Faults" });
    const tabs = screen.getByRole("tablist");
    expect(faults.compareDocumentPosition(tabs) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(within(screen.getByRole("main")).getByRole("list", { name: "Faults" })).toBe(faults);
    expect(within(screen.getByRole("tabpanel")).queryByRole("list", { name: "Faults" })).toBeNull();
  });

  // Catches: the summary living inside one tab, so it vanishes when another tab is opened.
  test("stays visible on every tab", async () => {
    await open();
    for (const name of ["Network adapters", "Components", "Plugins"]) {
      fireEvent.click(tab(name));
      await settle();
      expect(screen.getByRole("list", { name: "Faults" })).toBeInTheDocument();
    }
  });

  test("with nothing faulted it says there are no faults", async () => {
    await open("ready", {
      plugins: HEALTHY_PLUGINS,
      adapters: ADAPTERS.map((a) => ({ ...a, faulted: false, fault_reason: null })),
    });
    expect(screen.queryByRole("list", { name: "Faults" })).toBeNull();
    expect(screen.getByText("No faults")).toBeInTheDocument();
  });
});

describe("PluginsPage tabs", () => {
  test("three tabs in a named tablist, Plugins selected first", async () => {
    await open();
    const list = screen.getByRole("tablist");
    expect(list).toHaveAccessibleName();
    expect(
      within(list)
        .getAllByRole("tab")
        .map((el) => el.textContent?.trim()),
    ).toEqual(["Plugins", "Network adapters", "Components"]);
    expect(tab("Plugins")).toHaveAttribute("aria-selected", "true");
    expect(tab("Network adapters")).toHaveAttribute("aria-selected", "false");
    expect(tab("Components")).toHaveAttribute("aria-selected", "false");
  });

  test("the selected tab controls the one tab panel, which it names", async () => {
    await open();
    const panel = screen.getByRole("tabpanel");
    expect(screen.getAllByRole("tabpanel")).toHaveLength(1);
    expect(panel.id).not.toBe("");
    expect(tab("Plugins").getAttribute("aria-controls")).toBe(panel.id);
    expect(panel).toHaveAccessibleName("Plugins");
  });

  // Catches: a hardcoded panel id (only the first tab would match).
  test("every tab, once open, controls the one tab panel", async () => {
    await open();
    for (const name of ["Network adapters", "Components", "Plugins"]) {
      fireEvent.click(tab(name));
      await settle();
      const panel = screen.getByRole("tabpanel");
      expect(tab(name).getAttribute("aria-controls")).toBe(panel.id);
      expect(panel).toHaveAccessibleName(name);
    }
  });

  // Catches: a scrolling panel nothing in it can focus (WCAG 2.1.1; APG tabs).
  test("the tab panel is focusable so a keyboard user can scroll it, on every tab", async () => {
    await open();
    for (const name of ["Plugins", "Network adapters", "Components"]) {
      fireEvent.click(tab(name));
      await settle();
      expect(screen.getByRole("tabpanel")).toHaveAttribute("tabindex", "0");
    }
  });

  test("the Plugins tab shows the plugin cards with the faulted capability", async () => {
    await open();
    const panel = screen.getByRole("tabpanel", { name: "Plugins" });
    expect(within(panel).getByText("Acme Radar Pack")).toBeInTheDocument();
    expect(within(panel).getByText("Acme DIS")).toBeInTheDocument();
    const table = within(panel).getByRole("table", { name: "Capabilities of Acme Radar Pack" });
    expect(within(table).getByText("Faulted")).toBeInTheDocument();
    expect(within(panel).queryByText("dis-main")).toBeNull();
  });

  test("the Network adapters tab shows the adapters; the faulted one cannot be switched", async () => {
    await open();
    fireEvent.click(tab("Network adapters"));
    await settle();
    expect(tab("Network adapters")).toHaveAttribute("aria-selected", "true");
    expect(tab("Plugins")).toHaveAttribute("aria-selected", "false");
    const panel = screen.getByRole("tabpanel", { name: "Network adapters" });
    expect(within(panel).getByText("dis-main")).toBeInTheDocument();
    expect(within(panel).getByRole("switch", { name: "dis-main enabled" })).toBeEnabled();
    expect(within(panel).getByRole("switch", { name: "hla-lab enabled" })).toBeDisabled();
    expect(within(panel).queryByText("Acme Radar Pack")).toBeNull();
  });

  test("the Components tab shows the components by kind", async () => {
    await open();
    fireEvent.click(tab("Components"));
    await settle();
    const panel = screen.getByRole("tabpanel", { name: "Components" });
    expect(
      within(panel).getByRole("list", { name: "Motion model components" }),
    ).toBeInTheDocument();
    expect(within(panel).getByRole("list", { name: "Behavior components" })).toBeInTheDocument();
    expect(within(panel).getByRole("list", { name: "Sensor components" })).toBeInTheDocument();
  });

  // Catches: the shell reading component data it does not show (only the open tab is mounted).
  test("components are requested when their tab opens, not before", async () => {
    await open();
    expect(engine?.paths.some((p) => p === "/api/v1/plugins")).toBe(true);
    expect(engine?.paths.some((p) => p === "/api/v1/network/adapters")).toBe(true);
    expect(engine?.paths.some((p) => p.startsWith("/api/v1/components"))).toBe(false);
    fireEvent.click(tab("Components"));
    await settle();
    expect(engine?.paths.some((p) => p.startsWith("/api/v1/components"))).toBe(true);
  });

  test("the arrow keys move between the tabs and open them", async () => {
    await open();
    fireEvent.keyDown(tab("Plugins"), { key: "ArrowRight" });
    await settle();
    expect(tab("Network adapters")).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("tabpanel", { name: "Network adapters" })).toBeInTheDocument();
    fireEvent.keyDown(tab("Network adapters"), { key: "End" });
    await settle();
    expect(tab("Components")).toHaveAttribute("aria-selected", "true");
  });
});

describe("PluginsPage run states and the filled Button rule", () => {
  // The idle state renders once for the whole body (ASM-994 d, PluginStatusGate); the faults and tabs are not mounted.
  test("idle shows the load-a-scenario state once with a link to the dashboard and requests nothing", async () => {
    await open("idle");
    expect(screen.getAllByText("Load a scenario to see plugin status")).toHaveLength(1);
    expect(screen.getAllByRole("link", { name: "Open the dashboard" })).toHaveLength(1);
    expect(screen.queryByRole("tablist")).toBeNull();
    expect(engine?.paths).toEqual([]);
  });

  // Catches: a gate that keeps its first answer (idle -> loaded -> idle never updates).
  test("idle, loaded and idle again: the body follows the run state, the open tab is kept", async () => {
    const { push } = await open("ready");
    const idleText = "Load a scenario to see plugin status";
    fireEvent.click(tab("Components"));
    await settle();
    push("idle");
    await settle();
    expect(screen.getAllByText(idleText)).toHaveLength(1);
    expect(screen.queryByRole("tablist")).toBeNull();
    expect(screen.queryByRole("list", { name: "Faults" })).toBeNull();
    const before = engine?.paths.length ?? 0;
    push("ready", 2);
    await settle();
    expect(screen.queryByText(idleText)).toBeNull();
    expect(tab("Components")).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("list", { name: "Faults" })).toBeInTheDocument();
    const fresh = engine?.paths.slice(before) ?? [];
    expect(fresh).toContain("/api/v1/plugins");
    expect(fresh).toContain("/api/v1/network/adapters");
    expect(fresh.some((p) => p.startsWith("/api/v1/components"))).toBe(true);
    push("idle", 2);
    await settle();
    expect(screen.getAllByText(idleText)).toHaveLength(1);
    expect(screen.queryByRole("tablist")).toBeNull();
  });

  test("a loaded page has no filled Button (the only control is the adapter switch)", async () => {
    await open();
    expect(document.querySelectorAll(".vt-btn-filled")).toHaveLength(0);
    fireEvent.click(tab("Network adapters"));
    await settle();
    expect(document.querySelectorAll(".vt-btn-filled")).toHaveLength(0);
  });
});
