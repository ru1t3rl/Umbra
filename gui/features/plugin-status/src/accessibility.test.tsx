// WI-13 accessibility pins: names and roles over the four exports mounted together (role queries only; the axe
// pass of the whole page is the e2e test of WI-17). Contracts (ASM-955): every Table has a caption name, every
// switch a unique name, every list a name, Cards are level 3 headings, the empty state link has a name.
import { afterEach, describe, expect, test } from "bun:test";
import { cleanup, screen, within } from "@testing-library/react";
import { AdapterList } from "./components/AdapterList/AdapterList.tsx";
import { ComponentList } from "./components/ComponentList/ComponentList.tsx";
import { FaultSummary } from "./components/FaultSummary/FaultSummary.tsx";
import { PluginList } from "./components/PluginList/PluginList.tsx";
import { engineWith, type FakeEngine, mount, PLUGINS, settle } from "./statusTesting.tsx";

let engine: FakeEngine | undefined;
afterEach(() => {
  cleanup();
  engine?.restore();
  engine = undefined;
});

async function mountAll(state = "ready") {
  engine = engineWith({ plugins: PLUGINS });
  mount(
    <>
      <FaultSummary />
      <PluginList />
      <AdapterList />
      <ComponentList />
    </>,
    { state },
  );
  await settle();
}

describe("plugin-status accessibility", () => {
  test("every table has a non-empty accessible name and column headers", async () => {
    await mountAll();
    const tables = screen.getAllByRole("table");
    expect(tables).toHaveLength(4);
    for (const table of tables) {
      expect(table).toHaveAccessibleName(/\S/);
      expect(within(table).getAllByRole("columnheader").length).toBeGreaterThan(0);
    }
  });

  test("every table name is unique, so a screen reader user can tell them apart", async () => {
    await mountAll();
    const names = screen
      .getAllByRole("table")
      .map((t) => t.getAttribute("aria-label") ?? t.querySelector("caption")?.textContent);
    expect(new Set(names).size).toBe(names.length);
  });

  test("every switch has its own accessible name and is a keyboard-focusable button", async () => {
    await mountAll();
    const switches = screen.getAllByRole("switch");
    expect(switches).toHaveLength(2);
    expect(new Set(switches.map((s) => s.getAttribute("aria-label"))).size).toBe(2);
    for (const s of switches) {
      expect(s).toHaveAccessibleName(/enabled/);
      expect(s.tagName).toBe("BUTTON");
      expect(s).toHaveAttribute("aria-checked");
    }
  });

  test("a faulted adapter's switch is not focusable (disabled)", async () => {
    await mountAll();
    expect(screen.getByRole("switch", { name: "hla-lab enabled" })).toBeDisabled();
  });

  test("every list has an accessible name", async () => {
    await mountAll();
    const lists = screen.getAllByRole("list");
    expect(lists.length).toBeGreaterThan(0);
    for (const list of lists) expect(list).toHaveAccessibleName(/\S/);
  });

  test("every Card title of the page is a level 3 heading: eight cards, none skipped to another level", async () => {
    await mountAll();
    expect(screen.getAllByRole("heading", { level: 3 })).toHaveLength(2 + 2 + 4);
    expect(screen.queryAllByRole("heading", { level: 1 })).toHaveLength(0);
    expect(screen.queryAllByRole("heading", { level: 2 })).toHaveLength(0);
  });

  test("a fault says what it is in words, not only by colour", async () => {
    await mountAll();
    const list = screen.getByRole("list", { name: "Faults" });
    for (const li of within(list).getAllByRole("listitem")) expect(li).toHaveTextContent(/fault/i);
  });

  test("the empty state of an idle engine is reachable: a link with a name", async () => {
    await mountAll("idle");
    const links = screen.getAllByRole("link", { name: "Open the dashboard" });
    expect(links.length).toBeGreaterThan(0);
    for (const l of links) expect(l).toHaveAttribute("href");
  });
});
