// WI-13 PluginList refetching (ASM-955): the report is read again when the run state or the session changes
// (a fault arrives with a state change); it is never polled.
import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { act, cleanup } from "@testing-library/react";
import {
  advance,
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

const gets = () => engine.to("GET", "/api/v1/plugins").length;

async function mountAt(state: string, session = 1) {
  const m = mount(<PluginList />, { state, session });
  await settle();
  return m;
}

describe("PluginList refetching", () => {
  test("mounting requests the report once", async () => {
    await mountAt("ready");
    expect(gets()).toBe(1);
  });

  test("a run-state change requests it again, once", async () => {
    const m = await mountAt("ready");
    act(() => m.push(runFrame(2, "running")));
    await settle();
    expect(gets()).toBe(2);
  });

  test("a session change with the same run state requests it again, once", async () => {
    const m = await mountAt("running", 1);
    act(() => m.push(runFrame(2, "running", 2)));
    await settle();
    expect(gets()).toBe(2);
  });

  test("frames that only advance the tick cause no request", async () => {
    const m = await mountAt("running");
    for (let seq = 2; seq < 12; seq++) act(() => m.push(runFrame(seq, "running")));
    await settle();
    expect(gets()).toBe(1);
  });

  test("never polls, not even while running", async () => {
    await mountAt("running");
    await advance(30_000);
    expect(gets()).toBe(1);
  });
});
