// WI-13 FaultSummary, written before the implementation. Contracts (ASM-849, ASM-955):
// - no props; it reads the plugin report, the adapters and the run status (fault, network fault) and shows
//   `faultRows` of them;
// - no fault: a success Badge "No faults"; otherwise a list named "Faults" with one item per row, each with a danger
//   Badge naming the type ("Plugin fault", "Network fault", "Faulted capability", "Faulted adapter"), the
//   subject and the detail;
// - the fixtures are contract-shaped (`api/openapi.json`); the run status comes in through the stream frame, so
//   the frames below carry `fault` and `network_fault` the way `RunStatus` does;
// - idle engine: the empty state, no request; `no_scenario` from the adapters is an empty list, not a failure.
import { afterEach, describe, expect, test } from "bun:test";
import { act, cleanup, screen, within } from "@testing-library/react";
import type { AdapterDto, PluginReport } from "@umbra/data";
import {
  ADAPTERS,
  advance,
  engineWith,
  type FakeEngine,
  fakeTimers,
  mount,
  PLUGINS,
  problemReply,
  restoreTimers,
  runFrame,
  settle,
} from "../../statusTesting.tsx";
import { FaultSummary } from "./FaultSummary.tsx";

let engine: FakeEngine | undefined;
afterEach(() => {
  cleanup();
  engine?.restore();
  engine = undefined;
  restoreTimers();
});

const healthyPlugins: PluginReport = {
  plugins: PLUGINS.plugins.map((p) => ({
    ...p,
    capabilities: p.capabilities.map((c) => ({ ...c, faulted: false })),
  })),
};
const healthyAdapters: AdapterDto[] = ADAPTERS.map((a) => ({
  ...a,
  faulted: false,
  fault_reason: null,
}));

const wireFault = {
  plugin: "acme.radar",
  capability: "acme.radar.track",
  call: "step",
  message: "panicked at lib.rs:12",
};
const wireNetworkFault = { adapter: "dis-main", reason: "socket closed" };

async function mountSummary(
  script: Parameters<typeof engineWith>[0],
  run: Record<string, unknown> = {},
  state = "running",
) {
  engine = engineWith(script);
  const m = mount(<FaultSummary />, { state });
  if (Object.keys(run).length > 0) {
    const frame = runFrame(2, state);
    act(() => m.push({ ...frame, run: { ...frame.run, ...run } }));
  }
  await settle();
  return m;
}

const faults = () => screen.getByRole("list", { name: "Faults" });
const items = () => within(faults()).getAllByRole("listitem");

describe("FaultSummary without faults", () => {
  test("shows a success Badge No faults and no list", async () => {
    await mountSummary({ plugins: healthyPlugins, adapters: healthyAdapters });
    const badge = screen.getByText("No faults");
    expect(badge).toHaveClass("vt-badge-success");
    expect(screen.queryByRole("list", { name: "Faults" })).toBeNull();
  });

  test("a null plugin fault in the report is no fault", async () => {
    await mountSummary({ plugins: { ...healthyPlugins, fault: null }, adapters: healthyAdapters });
    expect(screen.getByText("No faults")).toBeInTheDocument();
  });

  test("no plugin and no adapter is no fault", async () => {
    await mountSummary({ plugins: { plugins: [] }, adapters: [] });
    expect(screen.getByText("No faults")).toBeInTheDocument();
  });
});

describe("FaultSummary rows, one contract-shaped fixture per fault type", () => {
  test("a plugin fault in the report is a danger row with the plugin, the capability, the call and the message", async () => {
    await mountSummary({
      plugins: { ...healthyPlugins, fault: wireFault },
      adapters: healthyAdapters,
    });
    expect(items()).toHaveLength(1);
    const row = items()[0] as HTMLElement;
    expect(within(row).getByText("Plugin fault")).toHaveClass("vt-badge-danger");
    expect(row).toHaveTextContent("acme.radar / acme.radar.track");
    expect(row).toHaveTextContent("step: panicked at lib.rs:12");
    expect(screen.queryByText("No faults")).toBeNull();
  });

  test("a plugin fault only in the run status is a row too", async () => {
    await mountSummary(
      { plugins: healthyPlugins, adapters: healthyAdapters },
      { fault: wireFault },
    );
    expect(items()).toHaveLength(1);
    expect(items()[0]).toHaveTextContent("Plugin fault");
  });

  test("a network fault in the run status is a danger row with the adapter and the reason", async () => {
    await mountSummary(
      { plugins: healthyPlugins, adapters: healthyAdapters },
      { network_fault: wireNetworkFault, stop_reason: "network_fault", state: "stopped" },
      "stopped",
    );
    expect(items()).toHaveLength(1);
    const row = items()[0] as HTMLElement;
    expect(within(row).getByText("Network fault")).toHaveClass("vt-badge-danger");
    expect(row).toHaveTextContent("dis-main");
    expect(row).toHaveTextContent("socket closed");
  });

  test("a faulted capability is a danger row with the plugin, the capability and the kind", async () => {
    await mountSummary({ plugins: PLUGINS, adapters: healthyAdapters });
    expect(items()).toHaveLength(1);
    const row = items()[0] as HTMLElement;
    expect(within(row).getByText("Faulted capability")).toHaveClass("vt-badge-danger");
    expect(row).toHaveTextContent("acme.radar / acme.radar.track");
    expect(row).toHaveTextContent("Behavior");
  });

  test("a faulted adapter is a danger row with its name and the reason", async () => {
    await mountSummary({ plugins: healthyPlugins, adapters: ADAPTERS });
    expect(items()).toHaveLength(1);
    const row = items()[0] as HTMLElement;
    expect(within(row).getByText("Faulted adapter")).toHaveClass("vt-badge-danger");
    expect(row).toHaveTextContent("hla-lab");
    expect(row).toHaveTextContent("socket closed");
  });

  test("every fault type together is four rows in the order plugin, network, capability, adapter", async () => {
    await mountSummary(
      {
        // distinct subjects, so no row hides another (ASM-966)
        // contract-realistic: the capability and adapter behind the two faults are faulted too (deduped)
        plugins: {
          ...PLUGINS,
          fault: { ...wireFault, plugin: "acme.dis-plugin", capability: "acme.dis" },
          plugins: PLUGINS.plugins.map((p) => ({
            ...p,
            capabilities: p.capabilities.map((c) =>
              c.id === "acme.dis" ? { ...c, faulted: true } : c,
            ),
          })),
        },
        adapters: ADAPTERS.map((a) => (a.name === "dis-main" ? { ...a, faulted: true } : a)),
      },
      { network_fault: wireNetworkFault },
    );
    expect(items().map((li) => li.textContent)).toEqual([
      expect.stringContaining("Plugin fault"),
      expect.stringContaining("Network fault"),
      expect.stringContaining("Faulted capability"),
      expect.stringContaining("Faulted adapter"),
    ]);
  });

  test("a fault that appears in a later frame shows without a reload", async () => {
    const m = await mountSummary({ plugins: healthyPlugins, adapters: healthyAdapters });
    expect(screen.getByText("No faults")).toBeInTheDocument();
    const frame = runFrame(3, "running");
    act(() => m.push({ ...frame, run: { ...frame.run, network_fault: wireNetworkFault } }));
    await settle();
    expect(items()).toHaveLength(1);
    expect(screen.queryByText("No faults")).toBeNull();
  });
});

describe("FaultSummary states", () => {
  test("an idle engine shows the empty state and requests nothing", async () => {
    await mountSummary({}, {}, "idle");
    expect(screen.getByText("Load a scenario to see plugin status")).toBeInTheDocument();
    expect(screen.queryByText("No faults")).toBeNull();
    expect(engine?.calls).toHaveLength(0);
  });

  test("adapters answering no_scenario count as empty, not as a failure", async () => {
    engine = engineWith({
      plugins: healthyPlugins,
      adapters: problemReply(409, "no_scenario", "No scenario", "no scenario is loaded"),
    });
    mount(<FaultSummary />, { state: "ready" });
    await settle();
    expect(screen.queryByRole("alert")).toBeNull();
    expect(screen.getByText("No faults")).toBeInTheDocument();
  });

  test("another failed request is a danger alert with the title and the detail", async () => {
    fakeTimers();
    await mountSummary({
      plugins: healthyPlugins,
      adapters: problemReply(500, "internal", "Engine failure", "the host crashed"),
    });
    // the data layer's retry (1 s, then 2 s) applies before the alert shows (ASM-963)
    await advance(1000);
    await advance(2000);
    expect(engine?.to("GET", "/api/v1/network/adapters")).toHaveLength(3);
    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent("Engine failure");
    expect(alert).toHaveTextContent("the host crashed");
  });
});
