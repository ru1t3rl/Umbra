// WI-13 review round (ASM-966, ASM-967): the stream's faults show while REST is pending or failed, the alert sits
// above the rows, "No faults" needs both reads to have succeeded, and a loading line shows first.
import { afterEach, describe, expect, test } from "bun:test";
import { act, cleanup, screen, within } from "@testing-library/react";
import {
  ADAPTERS,
  advance,
  engineWith,
  type FakeEngine,
  fakeEngine,
  fakeTimers,
  jsonReply,
  mount,
  NO_PLUGINS,
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

const fault = {
  plugin: "acme.radar",
  capability: "acme.radar.track",
  call: "step",
  message: "boom",
};
const netFault = { adapter: "dis-main", reason: "socket closed" };
const never = () => new Promise<Response>(() => {});
const healthy = ADAPTERS.map((a) => ({ ...a, faulted: false, fault_reason: null }));

function withRun(m: ReturnType<typeof mount>, run: Record<string, unknown>): void {
  const frame = runFrame(2, "running");
  act(() => m.push({ ...frame, run: { ...frame.run, ...run } }));
}

describe("FaultSummary with partial data", () => {
  test("the run status faults show while both reads are pending, and No faults does not", async () => {
    engine = fakeEngine(never);
    const m = mount(<FaultSummary />, { state: "running" });
    withRun(m, { fault, network_fault: netFault });
    await settle();
    const items = within(screen.getByRole("list", { name: "Faults" })).getAllByRole("listitem");
    expect(items.map((li) => li.textContent)).toEqual([
      expect.stringContaining("Plugin fault"),
      expect.stringContaining("Network fault"),
    ]);
    expect(screen.queryByText("No faults")).toBeNull();
  });

  test("the run status faults show below the alert when the plugins read failed", async () => {
    engine = fakeEngine((call) =>
      call.path === "/api/v1/plugins"
        ? problemReply(404, "not_found", "Plugins gone", "no")
        : jsonReply(healthy),
    );
    const m = mount(<FaultSummary />, { state: "running" });
    withRun(m, { network_fault: netFault });
    await settle();
    const alert = screen.getByRole("alert");
    const list = screen.getByRole("list", { name: "Faults" });
    expect(alert).toHaveTextContent("Plugins gone");
    expect(list).toHaveTextContent("Network fault");
    expect(Boolean(alert.compareDocumentPosition(list) & Node.DOCUMENT_POSITION_FOLLOWING)).toBe(
      true,
    );
  });

  test("the REST faults show while the other read is still pending", async () => {
    engine = fakeEngine((call) =>
      call.path === "/api/v1/plugins" ? never() : jsonReply(ADAPTERS),
    );
    mount(<FaultSummary />, { state: "ready" });
    await settle();
    expect(screen.getByRole("list", { name: "Faults" })).toHaveTextContent("Faulted adapter");
  });

  test("No faults needs both reads to have succeeded: not while one failed", async () => {
    engine = fakeEngine((call) =>
      call.path === "/api/v1/plugins"
        ? problemReply(404, "not_found", "Plugins gone", "no")
        : jsonReply(healthy),
    );
    mount(<FaultSummary />, { state: "ready" });
    await settle();
    expect(screen.getByRole("alert")).toBeInTheDocument();
    expect(screen.queryByText("No faults")).toBeNull();
    expect(screen.queryByText("Loading...")).toBeNull();
  });

  test("No faults goes away when a later read fails, although the earlier data was healthy", async () => {
    fakeTimers();
    let failing = false;
    engine = fakeEngine((call) =>
      failing && call.path === "/api/v1/plugins"
        ? problemReply(500, "internal", "Engine failure", "boom")
        : jsonReply(call.path === "/api/v1/plugins" ? NO_PLUGINS : healthy),
    );
    const m = mount(<FaultSummary />, { state: "ready" });
    await settle();
    expect(screen.getByText("No faults")).toBeInTheDocument();
    failing = true;
    act(() => m.push(runFrame(2, "running")));
    await settle();
    await advance(1000);
    await advance(2000);
    expect(screen.getByRole("alert")).toHaveTextContent("Engine failure");
    expect(screen.queryByText("No faults")).toBeNull();
  });

  test("No faults goes away when a later adapters read fails (a poll), although the earlier data was healthy", async () => {
    fakeTimers();
    let failing = false;
    engine = fakeEngine((call) =>
      failing && call.path === "/api/v1/network/adapters"
        ? problemReply(500, "internal", "Engine failure", "boom")
        : jsonReply(call.path === "/api/v1/plugins" ? NO_PLUGINS : healthy),
    );
    mount(<FaultSummary />, { state: "running" });
    await settle();
    expect(screen.getByText("No faults")).toBeInTheDocument();
    failing = true;
    await advance(2000);
    await advance(1000);
    await advance(2000);
    expect(screen.getByRole("alert")).toHaveTextContent("Engine failure");
    expect(screen.queryByText("No faults")).toBeNull();
  });

  test("No faults is not shown while a read is pending; a loading line is", async () => {
    engine = fakeEngine((call) => (call.path === "/api/v1/plugins" ? never() : jsonReply(healthy)));
    mount(<FaultSummary />, { state: "ready" });
    await settle();
    expect(screen.queryByText("No faults")).toBeNull();
    expect(screen.getByText("Loading...")).toBeInTheDocument();
  });

  test("a loading line shows until the reads arrive, then No faults", async () => {
    engine = engineWith({ plugins: NO_PLUGINS, adapters: [] });
    mount(<FaultSummary />, { state: "ready" });
    expect(screen.getByText("Loading...")).toBeInTheDocument();
    await settle();
    expect(screen.queryByText("Loading...")).toBeNull();
    expect(screen.getByText("No faults")).toBeInTheDocument();
  });
});
