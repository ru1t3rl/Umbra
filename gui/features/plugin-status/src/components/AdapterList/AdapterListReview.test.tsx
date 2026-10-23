// WI-13 review round (ASM-963, ASM-965, ASM-967): a failed read keeps the last good data under an alert, switch
// errors end with the session, the fault reason describes the switch, and a loading line shows until data arrives.
import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { act, cleanup, fireEvent, screen } from "@testing-library/react";
import {
  ADAPTERS,
  advance,
  engineWith,
  type FakeEngine,
  fakeEngine,
  fakeTimers,
  jsonReply,
  mount,
  problemReply,
  restoreTimers,
  runFrame,
  settle,
} from "../../statusTesting.tsx";
import { AdapterList } from "./AdapterList.tsx";

let engine: FakeEngine | undefined;
beforeEach(() => fakeTimers());
afterEach(() => {
  cleanup();
  engine?.restore();
  engine = undefined;
  restoreTimers();
});

const switchOf = (name: string) => screen.getByRole("switch", { name: `${name} enabled` });
const heading = (name: string) => screen.getByRole("heading", { name, level: 3 });
const above = (a: HTMLElement, b: HTMLElement) =>
  Boolean(a.compareDocumentPosition(b) & Node.DOCUMENT_POSITION_FOLLOWING);
const failingReads = () => problemReply(500, "internal", "Engine failure", "the host crashed");

describe("AdapterList keeps its data when a read fails", () => {
  test("a failed poll shows the alert above the last adapters", async () => {
    let failing = false;
    engine = fakeEngine((call) =>
      call.path === "/api/v1/network/adapters"
        ? failing
          ? failingReads()
          : jsonReply(ADAPTERS)
        : problemReply(404, "not_found", "Not found", ""),
    );
    mount(<AdapterList />, { state: "running" });
    await settle();
    failing = true;
    await advance(2000);
    await advance(1000);
    await advance(2000);
    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent("Engine failure");
    expect(heading("dis-main")).toBeInTheDocument();
    expect(above(alert, heading("dis-main"))).toBe(true);
  });

  test("a failed read after a switch shows the alert above the adapters", async () => {
    let toggled = false;
    engine = fakeEngine((call) => {
      if (call.method === "POST") {
        toggled = true;
        return jsonReply({ ...ADAPTERS[0], enabled: false });
      }
      return toggled ? failingReads() : jsonReply(ADAPTERS);
    });
    mount(<AdapterList />, { state: "ready" });
    await settle();
    fireEvent.click(switchOf("dis-main"));
    await settle();
    await advance(1000);
    await advance(2000);
    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent("Engine failure");
    expect(above(alert, heading("dis-main"))).toBe(true);
    expect(switchOf("dis-main")).not.toBeDisabled();
  });

  test("the alert goes away with the next good read", async () => {
    let failing = true;
    engine = fakeEngine(() => (failing ? failingReads() : jsonReply(ADAPTERS)));
    const m = mount(<AdapterList />, { state: "ready" });
    await settle();
    await advance(1000);
    await advance(2000);
    expect(screen.getByRole("alert")).toBeInTheDocument();
    failing = false;
    act(() => m.push(runFrame(2, "paused")));
    await settle();
    expect(screen.queryByRole("alert")).toBeNull();
    expect(heading("dis-main")).toBeInTheDocument();
  });
});

describe("AdapterList switch errors", () => {
  test("a failed switch alert ends when the session changes", async () => {
    engine = engineWith({
      toggle: () => problemReply(409, "adapter_faulted", "Adapter faulted", "it has faulted"),
    });
    const m = mount(<AdapterList />, { state: "ready", session: 1 });
    await settle();
    fireEvent.click(switchOf("dis-main"));
    await settle();
    expect(screen.getByRole("alert")).toHaveTextContent("Adapter faulted");
    act(() => m.push(runFrame(2, "ready", 2)));
    await settle();
    expect(screen.queryByRole("alert")).toBeNull();
  });

  test("a failed switch alert stays across frames of the same session", async () => {
    engine = engineWith({
      toggle: () => problemReply(409, "adapter_faulted", "Adapter faulted", "it has faulted"),
    });
    const m = mount(<AdapterList />, { state: "ready", session: 1 });
    await settle();
    fireEvent.click(switchOf("dis-main"));
    await settle();
    act(() => m.push(runFrame(2, "ready", 1)));
    await settle();
    expect(screen.getByRole("alert")).toHaveTextContent("Adapter faulted");
  });
});

describe("AdapterList accessibility and loading", () => {
  test("the fault reason describes the faulted adapter's switch", async () => {
    engine = engineWith();
    mount(<AdapterList />, { state: "ready" });
    await settle();
    const id = switchOf("hla-lab").getAttribute("aria-describedby");
    expect(id).toBeTruthy();
    expect(document.getElementById(id as string)).toHaveTextContent("socket closed");
    expect(switchOf("dis-main")).not.toHaveAttribute("aria-describedby");
  });

  test("a loading line shows until the adapters arrive", async () => {
    engine = engineWith();
    mount(<AdapterList />, { state: "ready" });
    expect(screen.getByText("Loading...")).toBeInTheDocument();
    await settle();
    expect(screen.queryByText("Loading...")).toBeNull();
  });
});
