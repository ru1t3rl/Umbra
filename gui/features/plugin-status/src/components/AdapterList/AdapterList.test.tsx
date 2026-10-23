// WI-13 AdapterList, written before the implementation. Contracts (ASM-823, ASM-955):
// - no props; the adapters come from `@umbra/data` (`GET /api/v1/network/adapters`), enable and disable from its
//   `useEnableNetworkAdapter` and `useDisableNetworkAdapter`; the run state from `useRunStatus`;
// - one ui-kit Card per adapter, titled with the adapter's name (level 3 heading), holding "Adapter <component id>",
//   "Plugin <plugin id>" or "Plugin built-in", "Ghosts <n>", "Key space base <base>, <count> keys", and a ui-kit
//   Table named "Counters of <name>" (columns Counter and Value, rows from `counterRows`);
// - a switch named "<name> enabled" (ui-kit Toggle, visible label "Enabled"): checked = `enabled`; a click
//   POSTs disable when on and enable when off, then the list is read again; disabled while the adapter is
//   faulted (ASM-823) and while its request is in flight; a failed request is a danger alert with the problem;
// - a faulted adapter shows a danger Badge "Faulted" and its `fault_reason` (none for a healthy one);
// - no adapter: "No network adapters in this scenario"; idle engine, or `no_scenario` from the engine: the empty
//   state "Load a scenario to see plugin status" (nothing is requested when the stream says idle).
import { afterEach, describe, expect, test } from "bun:test";
import { cleanup, fireEvent, screen, within } from "@testing-library/react";
import type { AdapterDto } from "@umbra/data";
import {
  ADAPTERS,
  advance,
  engineWith,
  type FakeEngine,
  fakeTimers,
  jsonReply,
  mount,
  problemReply,
  restoreTimers,
  settle,
} from "../../statusTesting.tsx";
import { AdapterList } from "./AdapterList.tsx";

let engine: FakeEngine | undefined;
afterEach(() => {
  cleanup();
  engine?.restore();
  engine = undefined;
  restoreTimers();
});

async function mountList(script: Parameters<typeof engineWith>[0] = {}, state = "ready") {
  engine = engineWith(script);
  const m = mount(<AdapterList />, { state });
  await settle();
  return m;
}

const cardOf = (name: string): HTMLElement => {
  const card = screen.getByRole("heading", { name, level: 3 }).closest(".vt-card");
  if (!(card instanceof HTMLElement)) throw new Error(`no card for ${name}`);
  return card;
};
const switchOf = (name: string) => screen.getByRole("switch", { name: `${name} enabled` });

describe("AdapterList cards", () => {
  test("renders one Card per adapter titled with its name, in scenario order", async () => {
    await mountList();
    expect(screen.getAllByRole("heading", { level: 3 }).map((h) => h.textContent)).toEqual([
      "dis-main",
      "hla-lab",
    ]);
  });

  test("a card shows the adapter component id", async () => {
    await mountList();
    expect(cardOf("dis-main")).toHaveTextContent(/Adapter\s*builtin\.dis/);
    expect(cardOf("hla-lab")).toHaveTextContent(/Adapter\s*acme\.hla/);
  });

  test("an adapter without a plugin shows built-in", async () => {
    await mountList();
    expect(cardOf("dis-main")).toHaveTextContent(/Plugin\s*built-in/);
  });

  test("an adapter with a null plugin shows built-in too", async () => {
    const adapters = [{ ...(ADAPTERS[0] as AdapterDto), plugin: null }];
    await mountList({ adapters });
    expect(cardOf("dis-main")).toHaveTextContent(/Plugin\s*built-in/);
  });

  test("an adapter of a plugin shows the plugin id", async () => {
    await mountList();
    expect(cardOf("hla-lab")).toHaveTextContent(/Plugin\s*acme\.hla-plugin/);
    expect(cardOf("hla-lab")).not.toHaveTextContent("built-in");
  });

  test("a card shows the ghosts", async () => {
    await mountList();
    expect(cardOf("dis-main")).toHaveTextContent(/Ghosts\s*3/);
    expect(cardOf("hla-lab")).toHaveTextContent(/Ghosts\s*0/);
  });

  test("a card shows the key space as base and count", async () => {
    await mountList();
    expect(cardOf("dis-main")).toHaveTextContent(/Key space\s*base 1000, 500 keys/);
    expect(cardOf("hla-lab")).toHaveTextContent(/Key space\s*base 5000, 100 keys/);
  });
});

describe("AdapterList counters", () => {
  test("a Table named Counters of <name> with the columns Counter and Value", async () => {
    await mountList();
    const table = within(cardOf("dis-main")).getByRole("table", { name: "Counters of dis-main" });
    expect(
      within(table)
        .getAllByRole("columnheader")
        .map((h) => h.textContent),
    ).toEqual(["Counter", "Value"]);
  });

  test("has one row per counter with its label and value", async () => {
    await mountList();
    const table = within(cardOf("dis-main")).getByRole("table", { name: "Counters of dis-main" });
    const rows = within(table).getAllByRole("row").slice(1);
    expect(rows).toHaveLength(14);
    const row = (label: string) => rows.find((r) => r.textContent?.startsWith(label));
    expect(row("Published")).toHaveTextContent("120");
    expect(row("Received")).toHaveTextContent("87");
    expect(row("Inbound dropped")).toHaveTextContent("4");
    expect(row("Decode errors")).toHaveTextContent("2");
  });

  test("each adapter's table shows its own counters", async () => {
    await mountList();
    const table = within(cardOf("hla-lab")).getByRole("table", { name: "Counters of hla-lab" });
    const io = within(table)
      .getAllByRole("row")
      .find((r) => r.textContent?.startsWith("I/O errors"));
    expect(io).toHaveTextContent("9");
  });
});

describe("AdapterList faults", () => {
  test("a faulted adapter shows a danger Badge Faulted and its fault reason", async () => {
    await mountList();
    const card = cardOf("hla-lab");
    expect(within(card).getByText("Faulted")).toHaveClass("vt-badge-danger");
    expect(card).toHaveTextContent("socket closed");
  });

  test("a healthy adapter shows neither", async () => {
    await mountList();
    const card = cardOf("dis-main");
    expect(within(card).queryByText("Faulted")).toBeNull();
    expect(card).not.toHaveTextContent("socket closed");
  });

  test("a faulted adapter without a reason still shows Faulted", async () => {
    const adapters = [{ ...(ADAPTERS[1] as AdapterDto), fault_reason: null }];
    await mountList({ adapters });
    expect(within(cardOf("hla-lab")).getByText("Faulted")).toBeInTheDocument();
  });
});

describe("AdapterList enable switch", () => {
  test("reflects the enabled state", async () => {
    await mountList();
    expect(switchOf("dis-main")).toHaveAttribute("aria-checked", "true");
    expect(switchOf("hla-lab")).toHaveAttribute("aria-checked", "false");
  });

  test("the switch of an enabled adapter calls disable for that adapter", async () => {
    await mountList();
    fireEvent.click(switchOf("dis-main"));
    await settle();
    expect(engine?.to("POST", "/api/v1/network/adapters/dis-main/disable")).toHaveLength(1);
    expect(engine?.to("POST", /enable$/).filter((c) => !c.path.endsWith("/disable"))).toHaveLength(
      0,
    );
  });

  test("the switch of a disabled, healthy adapter calls enable", async () => {
    const adapters = [{ ...(ADAPTERS[0] as AdapterDto), enabled: false }];
    await mountList({ adapters });
    fireEvent.click(switchOf("dis-main"));
    await settle();
    expect(engine?.to("POST", "/api/v1/network/adapters/dis-main/enable")).toHaveLength(1);
  });

  test("the list is read again after the call and the switch follows the new state", async () => {
    await mountList();
    expect(engine?.to("GET", "/api/v1/network/adapters")).toHaveLength(1);
    fireEvent.click(switchOf("dis-main"));
    await settle();
    expect(engine?.to("GET", "/api/v1/network/adapters")).toHaveLength(2);
    expect(switchOf("dis-main")).toHaveAttribute("aria-checked", "false");
  });

  test("a faulted adapter's switch is disabled and a click calls nothing (ASM-823)", async () => {
    await mountList();
    const toggle = switchOf("hla-lab");
    expect(toggle).toBeDisabled();
    fireEvent.click(toggle);
    await settle();
    expect(engine?.to("POST", /\/network\/adapters\//)).toHaveLength(0);
  });

  test("the switch is disabled while its request is in flight", async () => {
    let release: (r: Response) => void = () => {};
    const held = new Promise<Response>((resolve) => {
      release = resolve;
    });
    await mountList({ toggle: () => held });
    fireEvent.click(switchOf("dis-main"));
    await settle();
    expect(switchOf("dis-main")).toBeDisabled();
    release(jsonReply({ ...(ADAPTERS[0] as AdapterDto), enabled: false }));
    await settle();
    expect(switchOf("dis-main")).not.toBeDisabled();
  });

  test("a failed call is a danger alert with the title and detail, and the switch keeps its state", async () => {
    await mountList({
      toggle: () =>
        problemReply(409, "adapter_faulted", "Adapter faulted", "the adapter has faulted"),
    });
    fireEvent.click(switchOf("dis-main"));
    await settle();
    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent("Adapter faulted");
    expect(alert).toHaveTextContent("the adapter has faulted");
    expect(switchOf("dis-main")).toHaveAttribute("aria-checked", "true");
  });

  test("every switch has its own accessible name, and shows the visible label Enabled", async () => {
    await mountList();
    expect(screen.getAllByRole("switch")).toHaveLength(2);
    expect(cardOf("dis-main")).toHaveTextContent("Enabled");
  });
});

describe("AdapterList states", () => {
  test("a scenario without adapters shows No network adapters in this scenario", async () => {
    await mountList({ adapters: [] });
    expect(screen.getByText("No network adapters in this scenario")).toBeInTheDocument();
    expect(screen.queryAllByRole("heading", { level: 3 })).toHaveLength(0);
  });

  test("an idle engine shows the empty state with a link to the dashboard and requests nothing", async () => {
    await mountList({}, "idle");
    expect(screen.getByText("Load a scenario to see plugin status")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Open the dashboard" })).toBeInTheDocument();
    expect(engine?.calls).toHaveLength(0);
  });

  test("an answer of no_scenario is the empty state, not an error", async () => {
    await mountList({
      adapters: problemReply(409, "no_scenario", "No scenario", "no scenario is loaded"),
    });
    expect(screen.getByText("Load a scenario to see plugin status")).toBeInTheDocument();
    expect(screen.queryByRole("alert")).toBeNull();
  });

  test("another failed request is a danger alert with the title and the detail", async () => {
    fakeTimers();
    await mountList({
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
