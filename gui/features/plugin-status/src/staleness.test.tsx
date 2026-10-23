// WI-16 review m3 (ASM-994 c): the answers stay fresh for the polling period, so a page that unmounts and
// remounts an export (switching tabs) reuses them; explicit refetches (run-state change, polling) still read.
import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { act, cleanup } from "@testing-library/react";
import { AdapterList } from "./components/AdapterList/AdapterList.tsx";
import { ComponentList } from "./components/ComponentList/ComponentList.tsx";
import { PluginList } from "./components/PluginList/PluginList.tsx";
import {
  advance,
  engineWith,
  type FakeEngine,
  fakeTimers,
  mount,
  restoreTimers,
  runFrame,
  settle,
} from "./statusTesting.tsx";

let engine: FakeEngine;
beforeEach(() => {
  fakeTimers();
  engine = engineWith();
});
afterEach(() => {
  cleanup();
  engine.restore();
  restoreTimers();
});

const none = <p>none</p>;
const gets = (path: string | RegExp) => engine.to("GET", path).length;

describe("answers stay fresh for the polling period", () => {
  test("remounting an export within 2 s makes no second request, after 2 s it does", async () => {
    const m = mount(<PluginList />, { state: "ready" });
    await settle();
    expect(gets("/api/v1/plugins")).toBe(1);
    m.rerender(none);
    await advance(1900);
    m.rerender(<PluginList />);
    await settle();
    expect(gets("/api/v1/plugins")).toBe(1);
    m.rerender(none);
    await advance(200);
    m.rerender(<PluginList />);
    await settle();
    expect(gets("/api/v1/plugins")).toBe(2);
  });

  test("the same for the adapters and the components", async () => {
    const m = mount(<AdapterList />, { state: "ready" });
    await settle();
    m.rerender(<ComponentList />);
    await settle();
    m.rerender(<AdapterList />);
    await settle();
    m.rerender(<ComponentList />);
    await settle();
    expect(gets("/api/v1/network/adapters")).toBe(1);
    expect(gets(/\/api\/v1\/components/)).toBe(1);
  });

  test("a run-state change still reads, inside the fresh period", async () => {
    const m = mount(<PluginList />, { state: "ready" });
    await settle();
    act(() => m.push(runFrame(2, "running")));
    await settle();
    expect(gets("/api/v1/plugins")).toBe(2);
  });

  test("polling still refetches the adapters every 2 s while running", async () => {
    mount(<AdapterList />, { state: "running" });
    await settle();
    expect(gets("/api/v1/network/adapters")).toBe(1);
    await advance(2000);
    expect(gets("/api/v1/network/adapters")).toBe(2);
    await advance(2000);
    expect(gets("/api/v1/network/adapters")).toBe(3);
  });
});
