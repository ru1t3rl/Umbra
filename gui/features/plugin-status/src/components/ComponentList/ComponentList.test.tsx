// WI-13 ComponentList, written before the implementation. Contracts (ASM-822, ASM-955):
// - no props; the registry comes from `@umbra/data` (`GET /api/v1/components`, no kind filter);
// - one ui-kit Card per kind that has components, titled with `kindLabel(kind)` (level 3 heading), in the order of
//   the contract's kind table (motion model, terrain, behavior, sensor, condition, action, network adapter);
//   an unknown kind follows the known ones, titled verbatim, groups in first-seen order;
// - in a card a list named "<label> components", entries sorted by id, each with the id, the version, the
//   description and a ui-kit Badge with the provenance verbatim ("builtin", "file", "plugin", or an unknown
//   word); the Badge tone is neutral for builtin, info otherwise (ASM-955);
// - an empty registry: "No components registered"; idle engine: the empty state, no request;
// - a failed request is a danger alert with the problem's title and detail.
import { afterEach, describe, expect, test } from "bun:test";
import { cleanup, screen, within } from "@testing-library/react";
import type { ComponentEntry } from "@umbra/data";
import {
  advance,
  COMPONENTS,
  engineWith,
  type FakeEngine,
  fakeTimers,
  mount,
  problemReply,
  restoreTimers,
  settle,
} from "../../statusTesting.tsx";
import { ComponentList } from "./ComponentList.tsx";

let engine: FakeEngine | undefined;
afterEach(() => {
  cleanup();
  engine?.restore();
  engine = undefined;
  restoreTimers();
});

async function mountList(
  components: readonly ComponentEntry[] | Response = COMPONENTS,
  state = "ready",
) {
  engine = engineWith({ components });
  const m = mount(<ComponentList />, { state });
  await settle();
  return m;
}

const group = (label: string) =>
  within(screen.getByRole("list", { name: `${label} components` })).getAllByRole("listitem");

const wire = (over: Record<string, unknown>): ComponentEntry =>
  ({ ...(COMPONENTS[0] as ComponentEntry), ...over }) as ComponentEntry;

describe("ComponentList groups", () => {
  test("one Card per kind that has components, in the order of the kind table", async () => {
    await mountList();
    expect(screen.getAllByRole("heading", { level: 3 }).map((h) => h.textContent)).toEqual([
      "Motion model",
      "Behavior",
      "Sensor",
      "Network adapter",
    ]);
  });

  test("a kind without components has no Card", async () => {
    await mountList();
    expect(screen.queryByRole("heading", { name: "Terrain" })).toBeNull();
  });

  test("each group holds only its kind, sorted by id", async () => {
    await mountList();
    expect(group("Sensor").map((li) => li.textContent)).toEqual([
      expect.stringContaining("acme.radar.scan"),
      expect.stringContaining("builtin.visual"),
    ]);
    expect(group("Behavior")).toHaveLength(2);
    expect(group("Motion model")).toHaveLength(1);
  });

  test("an entry shows the id, the version and the description", async () => {
    await mountList();
    const row = group("Behavior").find((li) => li.textContent?.includes("file.loiter"));
    expect(row).toHaveTextContent("0.2.0");
    expect(row).toHaveTextContent("The file.loiter component");
  });
});

describe("ComponentList provenance", () => {
  test("every entry has a provenance Badge with the provenance verbatim", async () => {
    await mountList();
    const by = (id: string) =>
      group("Sensor")
        .concat(group("Behavior"))
        .find((li) => li.textContent?.includes(id)) as HTMLElement;
    expect(within(by("builtin.visual")).getByText("builtin")).toHaveClass("vt-badge");
    expect(within(by("acme.radar.scan")).getByText("plugin")).toHaveClass("vt-badge");
    expect(within(by("file.loiter")).getByText("file")).toHaveClass("vt-badge");
  });

  test("builtin is neutral and any other provenance is info", async () => {
    await mountList();
    const items = group("Sensor");
    const builtin = within(items[1] as HTMLElement).getByText("builtin");
    const plugin = within(items[0] as HTMLElement).getByText("plugin");
    expect(builtin).not.toHaveClass("vt-badge-info");
    expect(plugin).toHaveClass("vt-badge-info");
  });

  test("an unknown provenance is shown verbatim", async () => {
    await mountList([wire({ id: "x.y", provenance: "wasm" })]);
    expect(screen.getByText("wasm")).toHaveClass("vt-badge");
  });
});

describe("ComponentList unknown kinds", () => {
  test("an unknown kind has a Card titled verbatim after the known ones", async () => {
    await mountList([wire({ id: "q.one", kind: "quantum" }), wire({ id: "a.s", kind: "sensor" })]);
    expect(screen.getAllByRole("heading", { level: 3 }).map((h) => h.textContent)).toEqual([
      "Sensor",
      "quantum",
    ]);
    expect(group("quantum")).toHaveLength(1);
  });

  test("two unknown kinds come in first-seen order", async () => {
    await mountList([wire({ id: "z", kind: "zeta" }), wire({ id: "a", kind: "alpha" })]);
    expect(screen.getAllByRole("heading", { level: 3 }).map((h) => h.textContent)).toEqual([
      "zeta",
      "alpha",
    ]);
  });
});

describe("ComponentList states", () => {
  test("an empty registry shows No components registered", async () => {
    await mountList([]);
    expect(screen.getByText("No components registered")).toBeInTheDocument();
  });

  test("an idle engine shows the empty state and requests nothing", async () => {
    await mountList(COMPONENTS, "idle");
    expect(screen.getByText("Load a scenario to see plugin status")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Open the dashboard" })).toBeInTheDocument();
    expect(engine?.calls).toHaveLength(0);
  });

  test("a failed request is a danger alert with the title and the detail", async () => {
    fakeTimers();
    await mountList(problemReply(500, "internal", "Engine failure", "the registry is gone"));
    // the data layer's retry (1 s, then 2 s) applies before the alert shows (ASM-963)
    await advance(1000);
    await advance(2000);
    expect(engine?.to("GET", /^\/api\/v1\/components/)).toHaveLength(3);
    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent("Engine failure");
    expect(alert).toHaveTextContent("the registry is gone");
  });
});
