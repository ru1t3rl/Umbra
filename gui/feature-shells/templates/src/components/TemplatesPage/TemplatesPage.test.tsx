// WI-15 (docs/plans/M8.md): the templates page under the providers the app mounts, with a fake engine behind
// `fetch` (fixture responses) and a real stream connection over a fake socket. Written before the page.
import { afterEach, describe, expect, test } from "bun:test";
import { cleanup, fireEvent, screen, within } from "@testing-library/react";
import { fakeEngine, mountTemplates, settle, TEMPLATES } from "../../testing.tsx";
import TemplatesPage from "./TemplatesPage.tsx";

let engine: ReturnType<typeof fakeEngine> | undefined;
afterEach(() => {
  cleanup();
  engine?.restore();
  engine = undefined;
});

async function open(state = "ready") {
  engine = fakeEngine();
  const mounted = mountTemplates(TemplatesPage, state);
  await settle();
  return mounted;
}

const optionOf = (id: string): HTMLElement => {
  const found = within(screen.getByRole("listbox", { name: "Templates" }))
    .getAllByRole("option")
    .find((o) => o.textContent?.includes(id));
  if (!found) throw new Error(`no option for ${id}`);
  return found;
};

describe("TemplatesPage landmarks", () => {
  test("one banner holding the page navigation and the theme switch, one main with the page heading", async () => {
    await open();
    const banner = screen.getByRole("banner");
    expect(screen.getAllByRole("banner")).toHaveLength(1);
    expect(within(banner).getByRole("navigation", { name: "Pages" })).toBeInTheDocument();
    expect(within(banner).getByRole("switch")).toBeInTheDocument();
    const main = screen.getByRole("main");
    expect(screen.getAllByRole("main")).toHaveLength(1);
    expect(within(main).getByRole("heading", { level: 1, name: "Templates" })).toBeInTheDocument();
  });

  // Catches: the page title repeated in the bar next to the current navigation link (ASM-989 precedent, ASM-993).
  test("the bar has no visible title; the page heading is hidden and the nav link names the page", async () => {
    await open();
    const banner = screen.getByRole("banner");
    expect(banner.querySelector(".vt-cmdbar-title b")?.textContent).toBe("");
    expect(within(banner).queryByText("Templates", { selector: "b" })).toBeNull();
    expect(screen.getByRole("heading", { level: 1, name: "Templates" })).toHaveClass(
      "vt-visually-hidden",
    );
  });

  test("the list and the detail are two named regions inside main", async () => {
    await open();
    const main = screen.getByRole("main");
    expect(within(main).getByRole("region", { name: "Template list" })).toBeInTheDocument();
    expect(within(main).getByRole("region", { name: "Template detail" })).toBeInTheDocument();
    expect(within(main).getByRole("listbox", { name: "Templates" })).toBeInTheDocument();
  });

  test("the navigation marks the Templates page as current", async () => {
    await open();
    const nav = screen.getByRole("navigation", { name: "Pages" });
    expect(within(nav).getByRole("link", { name: /Templates/ })).toHaveAttribute(
      "aria-current",
      "page",
    );
    expect(within(nav).getByRole("link", { name: /Live map/ })).not.toHaveAttribute("aria-current");
  });
});

describe("TemplatesPage selection", () => {
  test("with nothing selected the detail pane asks for a selection", async () => {
    await open();
    const detail = screen.getByRole("region", { name: "Template detail" });
    expect(within(detail).getByText("Select a template to see its details")).toBeInTheDocument();
    expect(engine?.paths.some((p) => p.startsWith("/api/v1/templates/"))).toBe(false);
  });

  test("selecting a row shows that template in the detail and marks the row selected", async () => {
    await open();
    fireEvent.click(optionOf("scout"));
    await settle();
    const detail = screen.getByRole("region", { name: "Template detail" });
    expect(within(detail).getByRole("heading", { name: "scout" })).toBeInTheDocument();
    expect(within(detail).getByText("Fast recon vehicle")).toBeInTheDocument();
    expect(within(detail).queryByText("Select a template to see its details")).toBeNull();
    expect(optionOf("scout")).toHaveAttribute("aria-selected", "true");
  });

  test("selecting another row replaces the detail", async () => {
    await open();
    fireEvent.click(optionOf("scout"));
    await settle();
    fireEvent.click(optionOf("tank"));
    await settle();
    const detail = screen.getByRole("region", { name: "Template detail" });
    expect(within(detail).getByRole("heading", { name: "tank" })).toBeInTheDocument();
    expect(within(detail).queryByRole("heading", { name: "scout" })).toBeNull();
    expect(optionOf("scout")).toHaveAttribute("aria-selected", "false");
  });
});

describe("TemplatesPage run states and the filled Button rule", () => {
  test("idle shows the list's empty state and requests nothing", async () => {
    await open("idle");
    expect(screen.getByText("Load a scenario to browse its templates")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Open the dashboard" })).toBeInTheDocument();
    expect(engine?.paths).toEqual([]);
  });

  test("a selected template in a ready run has exactly one filled Button (Spawn)", async () => {
    await open();
    fireEvent.click(optionOf("tank"));
    await settle();
    const filled = [...document.querySelectorAll("button.vt-btn-filled")];
    expect(filled).toHaveLength(1);
    expect(filled[0]).toHaveTextContent(/spawn/i);
  });

  test("with nothing selected there is no filled Button", async () => {
    await open();
    expect(document.querySelectorAll(".vt-btn-filled")).toHaveLength(0);
  });
});

// WI-15 review m4 (ASM-993): the selection belongs to the scenario.
describe("TemplatesPage selection and the scenario", () => {
  const hint = "Select a template to see its details";

  // Catches: the old selection coming back for the next scenario, or the idle state shown twice.
  test("an unload clears the selection and renders the idle state once; a new scenario starts empty", async () => {
    const run = await open();
    fireEvent.click(optionOf("tank"));
    await settle();
    run.push("idle");
    await settle();
    expect(screen.getAllByText("Load a scenario to browse its templates")).toHaveLength(1);
    expect(screen.getAllByRole("link", { name: "Open the dashboard" })).toHaveLength(1);
    expect(screen.getByRole("region", { name: "Template detail" })).toHaveTextContent(hint);
    expect(screen.queryByRole("heading", { name: "tank" })).toBeNull();
    engine?.setTemplates(TEMPLATES.filter((tpl) => tpl.id !== "tank"));
    run.push("ready", 2);
    await settle();
    expect(screen.queryByRole("heading", { name: "tank" })).toBeNull();
    expect(screen.getByRole("region", { name: "Template detail" })).toHaveTextContent(hint);
    expect(optionOf("scout")).toHaveAttribute("aria-selected", "false");
  });

  // Catches: a session change without an idle frame keeping a template the new scenario may not have.
  test("a new session clears the selection", async () => {
    const run = await open();
    fireEvent.click(optionOf("tank"));
    await settle();
    run.push("ready", 2);
    await settle();
    expect(screen.queryByRole("heading", { name: "tank" })).toBeNull();
    expect(screen.getByRole("region", { name: "Template detail" })).toHaveTextContent(hint);
  });
});
