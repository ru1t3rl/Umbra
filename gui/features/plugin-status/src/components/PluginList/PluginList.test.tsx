// WI-13 PluginList, written before the implementation. Contracts (ASM-955):
// - no props; the report comes from `@umbra/data` (`GET /api/v1/plugins`), the run state from `useRunStatus`;
// - one ui-kit Card per plugin, titled with the plugin name (a level 3 heading), holding the id, the version,
//   "ABI major.minor", the root and the origin, and a ui-kit Table named "Capabilities of <name>" with the
//   columns Kind, Id, Version and Status; Kind is the `kindLabel`; Status is "OK" or a danger Badge "Faulted";
// - no plugin: "No plugins loaded"; a plugin without capabilities: the Table's empty text "No capabilities";
// - idle engine: the empty state "Load a scenario to see plugin status" with an AppLink "Open the dashboard",
//   and no request; a stopped or unknown run state requests normally;
// - a failed request is a danger Alert (role alert) with the problem's title and detail.
import { afterEach, describe, expect, test } from "bun:test";
import { cleanup, screen, within } from "@testing-library/react";
import type { PluginReport } from "@umbra/data";
import {
  advance,
  engineWith,
  type FakeEngine,
  fakeTimers,
  mount,
  NO_PLUGINS,
  PLUGINS,
  problemReply,
  restoreTimers,
  settle,
} from "../../statusTesting.tsx";
import { PluginList } from "./PluginList.tsx";

let engine: FakeEngine | undefined;
afterEach(() => {
  cleanup();
  engine?.restore();
  engine = undefined;
  restoreTimers();
});

async function mountList(plugins: PluginReport | Response = PLUGINS, state = "ready") {
  engine = engineWith({ plugins });
  const m = mount(<PluginList />, { state });
  await settle();
  return m;
}

const cardOf = (name: string): HTMLElement => {
  const heading = screen.getByRole("heading", { name, level: 3 });
  const card = heading.closest(".vt-card");
  if (!(card instanceof HTMLElement)) throw new Error(`no card for ${name}`);
  return card;
};

describe("PluginList cards", () => {
  test("renders one Card per plugin, titled with the plugin name", async () => {
    await mountList();
    expect(screen.getAllByRole("heading", { level: 3 })).toHaveLength(2);
    expect(cardOf("Acme Radar Pack")).toBeInTheDocument();
    expect(cardOf("Acme DIS")).toBeInTheDocument();
  });

  test("a card shows the id, the version, the ABI as major.minor, the root and the origin", async () => {
    await mountList();
    const card = cardOf("Acme Radar Pack");
    expect(card).toHaveTextContent("acme.radar");
    expect(card).toHaveTextContent("1.2.0");
    expect(card).toHaveTextContent(/ABI\s*1\.3/);
    expect(card).toHaveTextContent("scenario");
    expect(card).toHaveTextContent("plugins/radar.so");
  });

  test("a second plugin shows its own root and origin, not the first one's", async () => {
    await mountList();
    const card = cardOf("Acme DIS");
    expect(card).toHaveTextContent("plugins[0]");
    expect(card).toHaveTextContent("dis.so");
    expect(card).toHaveTextContent(/ABI\s*1\.0/);
    expect(card).not.toHaveTextContent("plugins/radar.so");
  });

  test("the plugins come in load order", async () => {
    await mountList();
    expect(screen.getAllByRole("heading", { level: 3 }).map((h) => h.textContent)).toEqual([
      "Acme Radar Pack",
      "Acme DIS",
    ]);
  });
});

describe("PluginList capabilities table", () => {
  const tableOf = (name: string) =>
    within(cardOf(name)).getByRole("table", { name: `Capabilities of ${name}` });

  test("is a table with the columns Kind, Id, Version and Status", async () => {
    await mountList();
    const headers = within(tableOf("Acme Radar Pack")).getAllByRole("columnheader");
    expect(headers.map((h) => h.textContent)).toEqual(["Kind", "Id", "Version", "Status"]);
  });

  test("has one row per capability in descriptor order, with the kind label, id and version", async () => {
    await mountList();
    const rows = within(tableOf("Acme Radar Pack")).getAllByRole("row").slice(1);
    expect(rows).toHaveLength(2);
    expect(rows[0]).toHaveTextContent("Sensor");
    expect(rows[0]).toHaveTextContent("acme.radar.scan");
    expect(rows[0]).toHaveTextContent("1.0.0");
    expect(rows[1]).toHaveTextContent("Behavior");
    expect(rows[1]).toHaveTextContent("acme.radar.track");
    expect(rows[1]).toHaveTextContent("0.4.1");
  });

  test("a faulted capability has a danger Badge Faulted in its row", async () => {
    await mountList();
    const row = within(tableOf("Acme Radar Pack")).getAllByRole("row")[2] as HTMLElement;
    const badge = within(row).getByText("Faulted");
    expect(badge).toHaveClass("vt-badge");
    expect(badge).toHaveClass("vt-badge-danger");
  });

  test("a healthy capability shows OK and no Faulted Badge", async () => {
    await mountList();
    const row = within(tableOf("Acme Radar Pack")).getAllByRole("row")[1] as HTMLElement;
    expect(row).toHaveTextContent("OK");
    expect(within(row).queryByText("Faulted")).toBeNull();
  });

  test("a plugin without capabilities shows No capabilities", async () => {
    const [first] = PLUGINS.plugins;
    await mountList({ plugins: [{ ...(first as NonNullable<typeof first>), capabilities: [] }] });
    expect(screen.getByText("No capabilities")).toBeInTheDocument();
  });

  test("an unknown capability kind is shown verbatim", async () => {
    const [first] = PLUGINS.plugins;
    const plugin = {
      ...(first as NonNullable<typeof first>),
      capabilities: [{ id: "x.q", kind: "quantum", version: "9", faulted: false }],
    } as unknown as PluginReport["plugins"][number];
    await mountList({ plugins: [plugin] });
    expect(screen.getByRole("cell", { name: "quantum" })).toBeInTheDocument();
  });
});

describe("PluginList states", () => {
  test("no plugin shows No plugins loaded", async () => {
    await mountList(NO_PLUGINS);
    expect(screen.getByText("No plugins loaded")).toBeInTheDocument();
    expect(screen.queryAllByRole("heading", { level: 3 })).toHaveLength(0);
  });

  test("an idle engine shows the empty state with a link to the dashboard and requests nothing", async () => {
    await mountList(PLUGINS, "idle");
    expect(screen.getByText("Load a scenario to see plugin status")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Open the dashboard" })).toBeInTheDocument();
    expect(screen.queryAllByRole("heading", { level: 3 })).toHaveLength(0);
    expect(engine?.calls).toHaveLength(0);
  });

  test("a stopped engine lists its plugins", async () => {
    await mountList(PLUGINS, "stopped");
    expect(cardOf("Acme DIS")).toBeInTheDocument();
  });

  test("an engine that has sent no frame yet requests and lists its plugins", async () => {
    engine = engineWith();
    mount(<PluginList />);
    await settle();
    expect(cardOf("Acme DIS")).toBeInTheDocument();
    expect(engine.to("GET", "/api/v1/plugins")).toHaveLength(1);
  });

  test("a failed request is a danger alert with the title and the detail of the problem", async () => {
    fakeTimers();
    await mountList(problemReply(500, "internal", "Engine failure", "the host crashed"));
    // the data layer's retry (1 s, then 2 s) applies before the alert shows (ASM-963)
    await advance(1000);
    await advance(2000);
    expect(engine?.to("GET", "/api/v1/plugins")).toHaveLength(3);
    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent("Engine failure");
    expect(alert).toHaveTextContent("the host crashed");
  });
});
