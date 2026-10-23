// WI-12 TemplateDetail, written before the implementation. Contracts (ASM-947):
// - props `{ templateId: string | null }`; `null` renders nothing (like the entity inspector); the template comes
//   from `GET /api/v1/templates/{id}`, the schemas from `GET /api/v1/components` (ASM-816);
// - an `h2` with the template id; the description when there is one; a list named "Inheritance chain" whose
//   items are the chain ids in the order the engine sends them (the template first, the root last), each item's
//   text is just the id; a Readout "Source" (dt/dd) with the defining file;
// - one `section` per `paramsSections` entry with an `h3` "Motion model: <id>", "Sensor: <id>" or
//   "Behavior: <id>" and one read-only `SchemaForm` labelled "Parameters of <id>" (role group), given the joined
//   schema and the merged params; an unknown component or a failed components request leaves the schema
//   undefined, so the form shows the params as JSON (a `pre`, no textbox);
// - below them the spawn form (named "Spawn entity", see SpawnForm.test.tsx);
// - `listComponents` is queried once per session: a remount, a second template and many sensors reuse the one
//   cached answer (the query is never stale; a session change invalidates it through
//   `useInvalidateEngineQueries`, which the app calls, ASM-884);
// - idle engine: the same empty state as the list, no request; a failed request is a danger Alert (role alert)
//   with the problem's title and detail, and the detail stays usable for the other query.
import { afterEach, describe, expect, test } from "bun:test";
import { cleanup, screen, within } from "@testing-library/react";
import {
  type EngineScript,
  engineWith,
  type FakeEngine,
  mount,
  problemReply,
  SCOUT,
  settle,
  TANK,
} from "../../browserTesting.tsx";
import { TemplateDetail } from "./TemplateDetail.tsx";

let engine: FakeEngine | undefined;
afterEach(() => {
  cleanup();
  engine?.restore();
  engine = undefined;
});

async function mountDetail(templateId: string | null, script: EngineScript = {}, state = "ready") {
  engine = engineWith(script);
  const m = mount(<TemplateDetail templateId={templateId} />, { state });
  await settle();
  return m;
}

const readout = (label: string): HTMLElement => {
  const dt = screen.getByText(label, { selector: "dt" });
  const dd = dt.parentElement?.querySelector("dd");
  if (!dd) throw new Error(`no value for ${label}`);
  return dd;
};
const group = (id: string): HTMLElement =>
  screen.getByRole("group", { name: `Parameters of ${id}` });
const sectionHeadings = () =>
  screen
    .getAllByRole("heading", { level: 3 })
    .map((h) => h.textContent ?? "")
    .filter((t) => /^(Motion model|Sensor|Behavior): /.test(t));

describe("TemplateDetail header", () => {
  test("headed with the template id, with its description", async () => {
    await mountDetail("scout");
    expect(screen.getByRole("heading", { level: 2, name: "scout" })).toBeInTheDocument();
    expect(screen.getByText("Fast recon vehicle")).toBeInTheDocument();
  });

  test("shows the inheritance chain in engine order, the root last", async () => {
    await mountDetail("scout");
    const chain = screen.getByRole("list", { name: "Inheritance chain" });
    expect(
      within(chain)
        .getAllByRole("listitem")
        .map((li) => li.textContent),
    ).toEqual(["scout", "vehicle", "base"]);
  });

  test("a template that inherits from nothing shows its own id as the only chain item", async () => {
    await mountDetail("solo", {
      details: { solo: { ...SCOUT, id: "solo", chain: ["solo"] } },
    });
    const chain = screen.getByRole("list", { name: "Inheritance chain" });
    expect(within(chain).getAllByRole("listitem")).toHaveLength(1);
  });

  test("shows the defining file as the Source readout", async () => {
    await mountDetail("scout");
    expect(readout("Source")).toHaveTextContent("templates/scout.yaml");
  });

  test("a template without a description shows no empty paragraph or null", async () => {
    await mountDetail("tank");
    expect(screen.queryByText("null")).toBeNull();
    expect(screen.getByRole("heading", { level: 2, name: "tank" })).toBeInTheDocument();
  });

  test("null renders nothing and requests nothing", async () => {
    await mountDetail(null);
    expect(document.body.firstElementChild).toBeEmptyDOMElement();
    expect((engine as FakeEngine).calls).toHaveLength(0);
  });
});

describe("TemplateDetail parameter sections", () => {
  test("one section per ComponentRef in the order motion, sensors, behavior", async () => {
    await mountDetail("tank");
    expect(sectionHeadings()).toEqual([
      "Motion model: builtin.waypoint",
      "Sensor: builtin.visual",
      "Sensor: acme.lidar",
      "Behavior: builtin.patrol",
    ]);
  });

  test("a template without behavior has no behavior section", async () => {
    await mountDetail("scout");
    expect(sectionHeadings()).toEqual([
      "Motion model: builtin.waypoint",
      "Sensor: builtin.visual",
      "Sensor: builtin.radar",
    ]);
  });

  test("each form is a read-only SchemaForm with the merged params of the template", async () => {
    await mountDetail("scout");
    const speed = within(group("builtin.waypoint")).getByRole("textbox", { name: "speed_mps" });
    expect(speed).toHaveValue("12");
    expect(speed).toHaveAttribute("readonly");
    expect(
      within(group("builtin.waypoint")).getByRole("switch", { name: "loop" }),
    ).not.toBeChecked();
  });

  test("each form gets the schema joined by its own component id", async () => {
    await mountDetail("scout");
    const visual = within(group("builtin.visual"));
    expect(visual.getByRole("textbox", { name: "range_m" })).toHaveValue("3000");
    expect(visual.getByRole("textbox", { name: "fov_deg" })).toHaveValue("120");
    const radar = within(group("builtin.radar"));
    expect(radar.getByRole("textbox", { name: "range_m" })).toHaveValue("20000");
    expect(radar.queryByRole("textbox", { name: "fov_deg" })).toBeNull();
  });

  test("a behavior section is a form too", async () => {
    await mountDetail("tank");
    expect(within(group("builtin.patrol")).getByRole("textbox", { name: "route" })).toHaveValue(
      "north",
    );
  });

  test("a component that GET /components does not list shows its params as JSON", async () => {
    await mountDetail("tank");
    const lidar = group("acme.lidar");
    expect(within(lidar).queryByRole("textbox")).toBeNull();
    const json = lidar.querySelector("pre");
    expect(json).not.toBeNull();
    expect(JSON.parse(json?.textContent ?? "null")).toEqual({ rays: 64 });
  });

  test("the known components of the same template still render as forms", async () => {
    await mountDetail("tank");
    expect(within(group("builtin.visual")).getByRole("textbox", { name: "range_m" })).toHaveValue(
      "2000",
    );
  });

  test("a failed components request shows every section as JSON and keeps the detail", async () => {
    await mountDetail("scout", {
      components: problemReply(404, "not_found", "Not found", "no component registry"),
    });
    expect(screen.getByRole("heading", { level: 2, name: "scout" })).toBeInTheDocument();
    expect(sectionHeadings()).toHaveLength(3);
    for (const id of ["builtin.waypoint", "builtin.visual", "builtin.radar"]) {
      expect(within(group(id)).queryByRole("textbox")).toBeNull();
      expect(group(id).querySelector("pre")).not.toBeNull();
    }
  });
});

describe("TemplateDetail components cache", () => {
  const componentCalls = () => (engine as FakeEngine).to("GET", /^\/api\/v1\/components/);

  test("queries listComponents once for a template with several components", async () => {
    await mountDetail("scout");
    expect(componentCalls()).toHaveLength(1);
  });

  test("a second template reuses the cached components", async () => {
    const m = await mountDetail("scout");
    m.rerender(<TemplateDetail templateId="tank" />);
    await settle();
    expect(screen.getByRole("heading", { level: 2, name: "tank" })).toBeInTheDocument();
    expect(componentCalls()).toHaveLength(1);
  });

  test("unmounting and mounting the detail again does not query the components again", async () => {
    const m = await mountDetail("scout");
    m.rerender(<div />);
    await settle();
    m.rerender(<TemplateDetail templateId="scout" />);
    await settle();
    expect(group("builtin.radar")).toBeInTheDocument();
    expect(componentCalls()).toHaveLength(1);
  });

  test("the components are asked for without a kind filter, so one answer serves every section", async () => {
    await mountDetail("tank");
    expect(componentCalls()[0]?.path).toBe("/api/v1/components");
  });
});

describe("TemplateDetail states", () => {
  test("an idle engine shows the empty state with a link to the dashboard and requests nothing", async () => {
    await mountDetail("scout", {}, "idle");
    expect(screen.getByText("Load a scenario to browse its templates")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Open the dashboard" })).toHaveAttribute("href", "/");
    expect((engine as FakeEngine).calls).toHaveLength(0);
  });

  test("an unknown template shows the problem in an alert", async () => {
    await mountDetail("nope");
    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent("Not found");
    expect(alert).toHaveTextContent("no such template");
  });

  test("contains the spawn form, preset with the template", async () => {
    await mountDetail("scout");
    const form = screen.getByRole("form", { name: "Spawn entity" });
    expect(within(form).getByRole("textbox", { name: "Template" })).toHaveValue("scout");
  });

  test("switching the template updates the heading, the chain, the sections and the spawn preset", async () => {
    const m = await mountDetail("scout");
    m.rerender(<TemplateDetail templateId="tank" />);
    await settle();
    expect(screen.getByRole("heading", { level: 2, name: "tank" })).toBeInTheDocument();
    expect(screen.queryByRole("heading", { level: 2, name: "scout" })).toBeNull();
    const items = within(screen.getByRole("list", { name: "Inheritance chain" })).getAllByRole(
      "listitem",
    );
    expect(items.map((li) => li.textContent)).toEqual(TANK.chain);
    expect(screen.getByRole("textbox", { name: "Template" })).toHaveValue("tank");
  });
});

describe("TemplateDetail accessibility", () => {
  test("every section is a region named by its heading", async () => {
    await mountDetail("scout");
    for (const name of ["Motion model: builtin.waypoint", "Sensor: builtin.visual"]) {
      expect(screen.getByRole("region", { name })).toBeInTheDocument();
    }
  });

  test("heading levels descend without skipping: h2 then h3", async () => {
    await mountDetail("scout");
    expect(screen.queryAllByRole("heading", { level: 1 })).toHaveLength(0);
    expect(screen.getAllByRole("heading", { level: 2 })).toHaveLength(1);
  });
});
