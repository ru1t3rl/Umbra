// The first frame after mount (no run state before it) is not a change: it causes no second request (ASM-962).
import { afterEach, beforeEach, expect, test } from "bun:test";
import { act, cleanup } from "@testing-library/react";
import {
  engineWith,
  type FakeEngine,
  fakeTimers,
  mount,
  restoreTimers,
  runFrame,
  settle,
} from "../../statusTesting.tsx";
import { PluginList } from "./PluginList.tsx";

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

test("the first frame after mounting without a run state requests nothing more", async () => {
  const m = mount(<PluginList />);
  await settle();
  expect(engine.to("GET", "/api/v1/plugins")).toHaveLength(1);
  act(() => m.push(runFrame(1, "ready")));
  await settle();
  expect(engine.to("GET", "/api/v1/plugins")).toHaveLength(1);
  act(() => m.push(runFrame(2, "running")));
  await settle();
  expect(engine.to("GET", "/api/v1/plugins")).toHaveLength(2);
});
