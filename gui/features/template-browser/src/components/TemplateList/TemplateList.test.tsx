// WI-12 TemplateList, written before the implementation. Contracts (ASM-947):
// - props `{ selectedId: string | null; onSelect(id: string): void }`; the templates come from `@umbra/data`
//   (`GET /api/v1/templates`, one query, nothing requested while the engine is idle), the run state from
//   `useRunStatus`;
// - a listbox named "Templates" with one `option` per template (a plain `ul`/`li` listbox up to 50 templates, the
//   ui-kit `VirtualList` role=listbox above, ASM-862; both have the same keyboard model: ArrowUp/Down/Home/End
//   move the active row (`aria-activedescendant`), Enter and a click call `onSelect`, `aria-selected` marks
//   `selectedId`); the option text holds the id, the description, the motion model and the behavior id, and
//   "No behavior" when there is none;
// - a search field "Search templates" filtering by case-insensitive substring of id, description, motion model
//   and behavior; a polite status "N of M templates"; "No templates match" and, for an empty scenario,
//   "No templates in this scenario";
// - idle engine: the empty state "Load a scenario to browse its templates" with an AppLink "Open the dashboard"
//   to "/" (ASM-815); no request; a stopped or unknown run state lists normally;
// - a failed request is shown as a danger Alert (role alert) with the problem's title and detail.
import { afterEach, beforeEach, describe, expect, jest, test } from "bun:test";
import { act, cleanup, fireEvent, screen, within } from "@testing-library/react";
import {
  engineWith,
  type FakeEngine,
  installResizeObserver,
  manyTemplates,
  mount,
  problemReply,
  runFrame,
  settle,
  TEMPLATES,
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

const listbox = () => screen.getByRole("listbox", { name: "Templates" });
const options = () => within(listbox()).queryAllByRole("option");
const optionOf = (id: string): HTMLElement => {
  const found = options().find((o) => o.textContent?.includes(id));
  if (!found) throw new Error(`no option for ${id}`);
  return found;
};

async function mountList(
  props: { selectedId?: string | null; onSelect?: (id: string) => void } = {},
  opts: { state?: string; templates?: Parameters<typeof engineWith>[0] } = {},
) {
  engine = engineWith(opts.templates ?? {});
  const m = mount(
    <TemplateList selectedId={props.selectedId ?? null} onSelect={props.onSelect ?? (() => {})} />,
    { state: opts.state ?? "ready" },
  );
  await settle();
  return m;
}

describe("TemplateList rows", () => {
  test("lists every template id of the scenario", async () => {
    await mountList();
    expect(options()).toHaveLength(TEMPLATES.length);
    for (const t of TEMPLATES) expect(optionOf(t.id)).toBeInTheDocument();
  });

  test("a row shows the id, the description, the motion model and the behavior", async () => {
    await mountList();
    const row = optionOf("tank");
    expect(row).toHaveTextContent("tank");
    expect(row).toHaveTextContent("Main battle tank");
    expect(row).toHaveTextContent("builtin.waypoint");
    expect(row).toHaveTextContent("builtin.patrol");
  });

  test("a template without behavior says so and shows no null", async () => {
    await mountList();
    const row = optionOf("scout");
    expect(row).toHaveTextContent("No behavior");
    expect(row).not.toHaveTextContent("null");
    expect(row).not.toHaveTextContent("undefined");
  });

  test("asks the engine for the templates once", async () => {
    await mountList();
    expect((engine as FakeEngine).to("GET", "/api/v1/templates")).toHaveLength(1);
  });

  test("an empty scenario says there are no templates", async () => {
    await mountList({}, { templates: { templates: [] } });
    expect(screen.getByText("No templates in this scenario")).toBeInTheDocument();
    expect(screen.queryAllByRole("option")).toHaveLength(0);
  });

  test("a failed request shows the problem in an alert", async () => {
    await mountList(
      {},
      {
        templates: {
          templates: problemReply(
            409,
            "no_scenario",
            "No scenario loaded",
            "Load a scenario first",
          ),
        },
      },
    );
    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent("No scenario loaded");
    expect(alert).toHaveTextContent("Load a scenario first");
  });
});

describe("TemplateList text filter", () => {
  const search = () => screen.getByRole("textbox", { name: "Search templates" });
  const type = (text: string) =>
    act(() => void fireEvent.change(search(), { target: { value: text } }));

  test.each([
    ["id", "scou", ["scout"]],
    ["description", "quadcopter", ["drone"]],
    ["motion model", "hover", ["drone"]],
    ["behavior", "patrol", ["tank"]],
    ["any case", "MAIN BATTLE", ["tank"]],
  ])("matches the %s", async (_what, text, ids) => {
    await mountList();
    type(text);
    expect(options().map((o) => o.textContent)).toHaveLength(ids.length);
    for (const id of ids) expect(optionOf(id)).toBeInTheDocument();
  });

  test("a shared motion model matches every template that uses it", async () => {
    await mountList();
    type("builtin.waypoint");
    expect(options()).toHaveLength(2);
  });

  test("clearing the text shows every template again", async () => {
    await mountList();
    type("drone");
    expect(options()).toHaveLength(1);
    type("");
    expect(options()).toHaveLength(TEMPLATES.length);
  });

  test("says when nothing matches", async () => {
    await mountList();
    type("zzz");
    expect(options()).toHaveLength(0);
    expect(screen.getByText("No templates match")).toBeInTheDocument();
  });

  test("a polite status counts the shown and all templates", async () => {
    await mountList();
    const status = screen.getByRole("status");
    expect(status).toHaveTextContent("3 of 3 templates");
    expect(status).toHaveAttribute("aria-live", "polite");
    type("drone");
    expect(status).toHaveTextContent("1 of 3 templates");
  });
});

describe("TemplateList selection", () => {
  test("a click selects the template", async () => {
    const picked: string[] = [];
    await mountList({ onSelect: (id) => picked.push(id) });
    fireEvent.click(optionOf("drone"));
    expect(picked).toEqual(["drone"]);
  });

  test("marks the selected template with aria-selected", async () => {
    await mountList({ selectedId: "scout" });
    expect(optionOf("scout")).toHaveAttribute("aria-selected", "true");
    expect(optionOf("tank")).toHaveAttribute("aria-selected", "false");
  });

  test("a selected id that is not in the scenario marks nothing", async () => {
    await mountList({ selectedId: "gone" });
    expect(options().every((o) => o.getAttribute("aria-selected") === "false")).toBe(true);
  });

  test.each([
    ["a plain list", 3],
    ["a virtual list", 60],
  ])("%s: the keyboard moves the active row and Enter selects it", async (_kind, count) => {
    const picked: string[] = [];
    await mountList(
      { onSelect: (id) => picked.push(id) },
      { templates: { templates: manyTemplates(count) } },
    );
    const box = listbox();
    fireEvent.keyDown(box, { key: "ArrowDown" });
    fireEvent.keyDown(box, { key: "ArrowDown" });
    const active = box.getAttribute("aria-activedescendant");
    expect(active).toBeTruthy();
    expect(document.getElementById(active as string)).toHaveTextContent("t001");
    fireEvent.keyDown(box, { key: "Enter" });
    expect(picked).toEqual(["t001"]);
    fireEvent.keyDown(box, { key: "ArrowUp" });
    fireEvent.keyDown(box, { key: "Enter" });
    expect(picked).toEqual(["t001", "t000"]);
  });

  test("arrows alone do not select, and Home and End jump to the ends", async () => {
    const picked: string[] = [];
    await mountList(
      { onSelect: (id) => picked.push(id) },
      { templates: { templates: manyTemplates(5) } },
    );
    const box = listbox();
    fireEvent.keyDown(box, { key: "End" });
    expect(
      document.getElementById(box.getAttribute("aria-activedescendant") as string),
    ).toHaveTextContent("t004");
    fireEvent.keyDown(box, { key: "Home" });
    expect(
      document.getElementById(box.getAttribute("aria-activedescendant") as string),
    ).toHaveTextContent("t000");
    expect(picked).toEqual([]);
  });

  test("the listbox is one tab stop", async () => {
    await mountList();
    expect(listbox()).toHaveAttribute("tabindex", "0");
    expect(options().every((o) => o.getAttribute("tabindex") === null)).toBe(true);
  });
});

describe("TemplateList windowing (ASM-862)", () => {
  test("50 templates render as a plain list: every option is in the DOM", async () => {
    await mountList({}, { templates: { templates: manyTemplates(50) } });
    expect(options()).toHaveLength(50);
  });

  test("51 templates render in a virtual list: only the visible rows and overscan exist", async () => {
    await mountList({}, { templates: { templates: manyTemplates(51) } });
    expect(options().length).toBeGreaterThan(0);
    expect(options().length).toBeLessThan(51);
  });

  test("filtering a long list down to 50 or fewer switches to the plain list", async () => {
    await mountList({}, { templates: { templates: manyTemplates(120) } });
    expect(options().length).toBeLessThan(51);
    act(
      () =>
        void fireEvent.change(screen.getByRole("textbox", { name: "Search templates" }), {
          target: { value: "t00" },
        }),
    );
    expect(options()).toHaveLength(10);
  });

  test("the status counts all templates of a virtual list, not the rendered rows", async () => {
    await mountList({}, { templates: { templates: manyTemplates(200) } });
    expect(screen.getByRole("status")).toHaveTextContent("200 of 200 templates");
  });
});

describe("TemplateList run state", () => {
  test("an idle engine shows the empty state and requests nothing", async () => {
    await mountList({}, { state: "idle" });
    expect(screen.getByText("Load a scenario to browse its templates")).toBeInTheDocument();
    expect(screen.queryByRole("listbox")).toBeNull();
    expect((engine as FakeEngine).calls).toHaveLength(0);
  });

  test("the empty state links to the dashboard through the app navigation", async () => {
    const visited: string[] = [];
    engine = engineWith();
    mount(<TemplateList selectedId={null} onSelect={() => {}} />, {
      state: "idle",
      navigate: (to) => visited.push(to),
    });
    await settle();
    const link = screen.getByRole("link", { name: "Open the dashboard" });
    expect(link).toHaveAttribute("href", "/");
    fireEvent.click(link);
    expect(visited).toEqual(["/"]);
  });

  test("the list appears when a scenario is loaded while the page is open", async () => {
    const m = await mountList({}, { state: "idle" });
    act(() => m.push(runFrame(2, "ready")));
    await settle();
    expect(options()).toHaveLength(TEMPLATES.length);
    expect(screen.queryByText("Load a scenario to browse its templates")).toBeNull();
  });

  test.each(["ready", "running", "paused", "stopped"])(
    "lists the templates when %s",
    async (state) => {
      await mountList({}, { state });
      expect(options()).toHaveLength(TEMPLATES.length);
    },
  );

  test("before the first frame the run state is unknown and the templates are requested", async () => {
    engine = engineWith();
    mount(<TemplateList selectedId={null} onSelect={() => {}} />);
    await settle();
    expect((engine as FakeEngine).to("GET", "/api/v1/templates")).toHaveLength(1);
  });
});

describe("TemplateList accessibility and design", () => {
  test("the search field and the list have accessible names", async () => {
    await mountList();
    expect(screen.getByRole("textbox", { name: "Search templates" })).toBeInTheDocument();
    expect(screen.getByRole("listbox", { name: "Templates" })).toBeInTheDocument();
  });

  test("an option is presentational content: no button or nested option inside a row", async () => {
    await mountList();
    for (const o of options()) {
      expect(within(o).queryAllByRole("button")).toHaveLength(0);
      expect(within(o).queryAllByRole("option")).toHaveLength(0);
    }
  });
});
