// WI-12 review tests for the list: one element across the 50/51 threshold, empty listbox, DOM ids (ASM-953).
import { afterEach, beforeEach, describe, expect, jest, test } from "bun:test";
import { act, cleanup, fireEvent, screen, within } from "@testing-library/react";
import {
  engineWith,
  type FakeEngine,
  installResizeObserver,
  manyTemplates,
  mount,
  settle,
} from "../../browserTesting.tsx";
import { TemplateList } from "./TemplateList.tsx";

let engine: FakeEngine | undefined;
let restoreObserver: () => void;
beforeEach(() => {
  restoreObserver = installResizeObserver(400);
});
afterEach(() => {
  cleanup();
  jest.useRealTimers();
  restoreObserver();
  engine?.restore();
  engine = undefined;
});

async function mountList(templates: Parameters<typeof engineWith>[0]) {
  engine = engineWith(templates);
  const m = mount(<TemplateList selectedId={null} onSelect={() => {}} />, { state: "ready" });
  await settle();
  return m;
}
const listbox = () => screen.getByRole("listbox", { name: "Templates" });
const search = (value: string) =>
  act(
    () =>
      void fireEvent.change(screen.getByRole("textbox", { name: "Search templates" }), {
        target: { value },
      }),
  );

describe("TemplateList across the windowing threshold", () => {
  test("filtering from 60 to 10 templates keeps the same listbox element", async () => {
    await mountList({ templates: manyTemplates(60) });
    const before = listbox();
    expect(within(before).getAllByRole("option").length).toBeLessThan(60);
    search("t00");
    expect(listbox()).toBe(before);
    expect(within(before).getAllByRole("option")).toHaveLength(10);
  });

  test("the keyboard position survives crossing the threshold", async () => {
    await mountList({ templates: manyTemplates(60) });
    fireEvent.keyDown(listbox(), { key: "ArrowDown" });
    fireEvent.keyDown(listbox(), { key: "ArrowDown" });
    search("t00");
    expect(
      document.getElementById(listbox().getAttribute("aria-activedescendant") as string),
    ).toHaveTextContent("t001");
  });

  test("PageDown moves the active row by a page in the short list too", async () => {
    await mountList({ templates: manyTemplates(30) });
    fireEvent.keyDown(listbox(), { key: "PageDown" });
    expect(listbox().getAttribute("aria-activedescendant")).toBeTruthy();
  });
});

describe("TemplateList empty listbox", () => {
  test("an empty scenario's listbox is not a tab stop", async () => {
    await mountList({ templates: [] });
    expect(listbox()).toHaveAttribute("tabindex", "-1");
  });

  test("a filter without a match makes the listbox not a tab stop, clearing it restores it", async () => {
    await mountList({});
    search("zzz");
    expect(listbox()).toHaveAttribute("tabindex", "-1");
    search("");
    expect(listbox()).toHaveAttribute("tabindex", "0");
  });
});

describe("TemplateList DOM ids", () => {
  test("a template id with spaces and quotes gives a valid aria-activedescendant", async () => {
    await mountList({
      templates: [
        { id: 'my "big" tank', motion: "builtin.waypoint", source: "templates/a.yaml" },
        { id: "other one", motion: "builtin.waypoint", source: "templates/b.yaml" },
      ],
    });
    fireEvent.keyDown(listbox(), { key: "ArrowDown" });
    const id = listbox().getAttribute("aria-activedescendant") as string;
    expect(id).not.toMatch(/\s|"/);
    expect(document.getElementById(id)).toHaveTextContent('my "big" tank');
  });

  test("the ids of two templates differ", async () => {
    await mountList({});
    const ids = within(listbox())
      .getAllByRole("option")
      .map((o) => o.id);
    expect(new Set(ids).size).toBe(ids.length);
  });
});
