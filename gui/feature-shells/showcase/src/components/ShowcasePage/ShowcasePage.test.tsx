import { afterEach, describe, expect, test } from "bun:test";
import { cleanup, render, screen, within } from "@testing-library/react";
import {
  colorTokens,
  radiusTokens,
  shadowTokens,
  spacingTokens,
  ThemeProvider,
  typeStyles,
} from "@umbra/ui-kit";
import { route } from "../../index.ts";
import ShowcasePage from "./ShowcasePage.tsx";

afterEach(cleanup);

function renderPage() {
  return render(
    <ThemeProvider>
      <ShowcasePage />
    </ThemeProvider>,
  );
}

describe("ShowcasePage tokens", () => {
  test.each([
    ["colour", colorTokens],
    ["type", typeStyles],
    ["spacing", spacingTokens],
    ["radius", radiusTokens],
    ["shadow", shadowTokens],
  ])("lists every %s token by name", (_group, names) => {
    renderPage();
    for (const name of names) expect(screen.getAllByText(name).length).toBeGreaterThan(0);
  });
});

describe("ShowcasePage components", () => {
  test("has the page banner with the theme switch", () => {
    const { container } = renderPage();
    // only the first (docked) bar is a direct child of the page root, hence the page banner (the fifth bar hosts AppNav, ASM-870)
    expect(container.firstElementChild?.matches("header.vt-cmdbar")).toBe(true);
    expect(container.querySelectorAll(".vt-cmdbar").length).toBe(5);
    expect(screen.getByRole("switch", { name: "Day theme" })).toBeTruthy();
  });

  test("every component is present by its accessible name", () => {
    renderPage();
    const byRole = (role: string, name: string | RegExp) =>
      expect(screen.getAllByRole(role, { name }).length).toBeGreaterThan(0);
    byRole("button", "Start exercise"); // Button
    byRole("button", "Settings"); // icon-only Button
    byRole("group", "Time rate"); // ChipGroup
    byRole("button", "1:4"); // Chip
    byRole("region", "Movement orders"); // Panel
    expect(screen.getByText("2nd Mechanised Brigade")).toBeTruthy(); // Card
    byRole("textbox", "Callsign"); // TextField
    byRole("switch", "Show historic tracks"); // Toggle
    byRole("slider", "Update rate"); // Slider
    byRole("tablist", "Sidebar"); // Tabs
    byRole("tab", /Order of battle/);
    byRole("timer", /Scenario clock/);
    expect(screen.getAllByText("Scenario clock").length).toBeGreaterThan(0); // Readout
    expect(screen.getAllByText("Running").length).toBeGreaterThan(0); // Badge
    expect(screen.getAllByText("2 MECH BDE").length).toBeGreaterThan(0); // AffiliationTag
    byRole("listbox", "Entities"); // EntityList
    byRole("option", /Alpha 1-1/); // EntityRow
    byRole("alert", "Route rejected"); // Alert
    byRole("alert", "Federate latency 480 ms");
    byRole("status", "Recording started");
  });

  test("every section has at most one filled azure button", () => {
    const { container } = renderPage();
    for (const section of container.querySelectorAll("section.showcase-section")) {
      const filled = section.querySelectorAll("button.vt-btn-filled");
      expect(filled.length).toBeLessThanOrEqual(1);
    }
    expect(within(container).getAllByRole("heading", { level: 3 }).length).toBeGreaterThan(8);
  });

  test("the floating sample has a glass chip and a panel inside the bar", () => {
    const { container } = renderPage();
    const bar = container.querySelector("#showcase-floating .vt-cmdbar");
    expect(bar?.querySelector(".vt-chip-glass")).toBeTruthy();
    expect(bar?.querySelector(".vt-panel")).toBeTruthy();
    expect(bar?.querySelector(".vt-readout-value")).toHaveTextContent("128");
  });
});

describe("ShowcasePage M8 additions (ADR 0073)", () => {
  test("shows Table, Select and AppNav by their accessible names", () => {
    renderPage();
    const table = screen.getByRole("table", { name: "Plugin capabilities" });
    expect(table).toHaveClass("vt-table");
    expect(within(table).getAllByRole("columnheader").length).toBeGreaterThan(1);
    expect(screen.getByRole("combobox", { name: "Coordinate format" })).toBeTruthy();
    expect(screen.getByRole("navigation", { name: "Pages" })).toBeTruthy();
  });

  test("the sample navigation marks one current page", () => {
    renderPage();
    const nav = screen.getByRole("navigation", { name: "Pages" });
    expect(nav.querySelectorAll("[aria-current='page']").length).toBe(1);
  });

  test("the new components sit in sections with the ids the screenshots use", () => {
    const { container } = renderPage();
    for (const id of ["showcase-table", "showcase-select", "showcase-appnav"]) {
      expect(container.querySelector(`#${id}`)).toBeTruthy();
    }
  });
});

describe("ShowcasePage VirtualList sample (ADR 0072)", () => {
  test("shows a virtual listbox and a log by their accessible names", () => {
    renderPage();
    const listbox = screen.getByRole("listbox", { name: "Sample entities" });
    expect(listbox.getAttribute("tabindex")).toBe("0");
    expect(screen.getByRole("log", { name: "Sample event log" })).toBeTruthy();
  });

  test("the samples hold thousands of items but render only a window", () => {
    renderPage();
    const options = within(screen.getByRole("listbox", { name: "Sample entities" })).getAllByRole(
      "option",
    );
    expect(options.length).toBeGreaterThan(0);
    expect(options.length).toBeLessThan(100);
    expect(options[0]).toHaveAttribute("aria-setsize", "10000");
  });

  test("the samples sit under their own heading inside the M8 group, not the M5b components group", () => {
    const { container } = renderPage();
    expect(screen.getByRole("heading", { name: "Virtual list", level: 3 })).toBeTruthy();
    const group = container.querySelector("#showcase-m8") as HTMLElement;
    for (const id of ["showcase-vlist-listbox", "showcase-vlist-log"]) {
      const el = container.querySelector(`#${id}`);
      expect(el).toBeTruthy();
      expect(group.contains(el)).toBe(true);
    }
    expect(container.querySelector("#showcase-components #showcase-vlist-listbox")).toBeNull();
  });
});

describe("headings", () => {
  test("one page h1, Tokens and Components as h2", () => {
    renderPage();
    expect(screen.getAllByRole("heading", { level: 1 }).map((h) => h.textContent)).toEqual([
      "Design system",
    ]);
    expect(screen.getByRole("heading", { level: 2, name: "Tokens" })).toBeTruthy();
    expect(screen.getByRole("heading", { level: 2, name: "Components" })).toBeTruthy();
  });
});

describe("route", () => {
  test("is /showcase and lazy-loads the page", async () => {
    expect(route.path).toBe("/showcase");
    const loaded = await route.lazy();
    expect(loaded.Component).toBe(ShowcasePage);
  });
});
