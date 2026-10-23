// ASM-994: the gate shows the idle state once and hides (does not mount) its children while idle.
import { afterEach, describe, expect, test } from "bun:test";
import { act, cleanup, screen } from "@testing-library/react";
import { engineWith, type FakeEngine, mount, runFrame, settle } from "../../statusTesting.tsx";
import { PluginList } from "../PluginList/PluginList.tsx";
import { PluginStatusGate } from "./PluginStatusGate.tsx";

let engine: FakeEngine | undefined;
afterEach(() => {
  cleanup();
  engine?.restore();
  engine = undefined;
});

const ui = (
  <PluginStatusGate>
    <p>inside</p>
    <PluginList />
  </PluginStatusGate>
);

describe("PluginStatusGate", () => {
  test("idle: the empty state once, no children, no request", async () => {
    engine = engineWith({});
    mount(ui, { state: "idle" });
    await settle();
    expect(screen.getAllByText("Load a scenario to see plugin status")).toHaveLength(1);
    expect(screen.getAllByRole("link", { name: "Open the dashboard" })).toHaveLength(1);
    expect(screen.queryByText("inside")).toBeNull();
    expect(engine.calls).toHaveLength(0);
  });

  test("a loaded scenario: the children, no empty state", async () => {
    engine = engineWith({});
    mount(ui, { state: "ready" });
    await settle();
    expect(screen.getByText("inside")).toBeInTheDocument();
    expect(screen.queryByText("Load a scenario to see plugin status")).toBeNull();
  });

  test("no run frame yet is not idle (ASM-958)", async () => {
    engine = engineWith({});
    mount(ui);
    await settle();
    expect(screen.getByText("inside")).toBeInTheDocument();
  });

  // Catches: a gate that keeps its first answer (it is public API).
  test("idle, loaded and idle again follows the run state", async () => {
    engine = engineWith({});
    const m = mount(ui, { state: "ready" });
    await settle();
    expect(screen.getByText("inside")).toBeInTheDocument();
    act(() => m.push(runFrame(2, "idle", 1)));
    await settle();
    expect(screen.getAllByText("Load a scenario to see plugin status")).toHaveLength(1);
    expect(screen.queryByText("inside")).toBeNull();
    act(() => m.push(runFrame(3, "ready", 2)));
    await settle();
    expect(screen.getByText("inside")).toBeInTheDocument();
    expect(screen.queryByText("Load a scenario to see plugin status")).toBeNull();
    act(() => m.push(runFrame(4, "idle", 2)));
    await settle();
    expect(screen.getAllByText("Load a scenario to see plugin status")).toHaveLength(1);
    expect(screen.queryByText("inside")).toBeNull();
  });
});
