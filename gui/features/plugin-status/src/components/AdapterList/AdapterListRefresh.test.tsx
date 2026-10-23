// WI-13 AdapterList refetching (ASM-955): the adapters are read again when the run state or the session changes,
// and every 2 seconds while the run state is `running`; in every other state nothing polls. Fake timers; no real
// waiting. Tick-only frames (same state, same session) cause no request.
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
import { AdapterList } from "./AdapterList.tsx";

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

const gets = () => engine.to("GET", "/api/v1/network/adapters").length;

async function mountAt(state: string, session = 1) {
  const m = mount(<AdapterList />, { state, session });
  await settle();
  return m;
}

describe("AdapterList refetching", () => {
  test("mounting requests the adapters once", async () => {
    await mountAt("ready");
    expect(gets()).toBe(1);
  });

  test("a run-state change requests them again, once", async () => {
    const m = await mountAt("ready");
    act(() => m.push(runFrame(2, "paused")));
    await settle();
    expect(gets()).toBe(2);
  });

  test("a session change with the same run state requests them again, once", async () => {
    const m = await mountAt("paused", 1);
    act(() => m.push(runFrame(2, "paused", 2)));
    await settle();
    expect(gets()).toBe(2);
  });

  test("frames that only advance the tick cause no request", async () => {
    const m = await mountAt("paused");
    for (let seq = 2; seq < 12; seq++) act(() => m.push(runFrame(seq, "paused")));
    await settle();
    expect(gets()).toBe(1);
  });

  test("polls every 2 seconds while running", async () => {
    await mountAt("running");
    expect(gets()).toBe(1);
    await advance(2000);
    expect(gets()).toBe(2);
    await advance(2000);
    expect(gets()).toBe(3);
  });

  test("does not poll before the 2 seconds are over", async () => {
    await mountAt("running");
    await advance(1900);
    expect(gets()).toBe(1);
  });

  test("starting the run starts the polling", async () => {
    const m = await mountAt("ready");
    await advance(10_000);
    expect(gets()).toBe(1);
    act(() => m.push(runFrame(2, "running")));
    await settle();
    const afterStart = gets();
    await advance(2000);
    expect(gets()).toBe(afterStart + 1);
  });

  test("pausing stops the polling", async () => {
    const m = await mountAt("running");
    act(() => m.push(runFrame(2, "paused")));
    await settle();
    const afterPause = gets();
    await advance(10_000);
    expect(gets()).toBe(afterPause);
  });

  test.each(["ready", "paused", "stopped"])("does not poll in the %s state", async (state) => {
    await mountAt(state);
    await advance(10_000);
    expect(gets()).toBe(1);
  });

  test("an idle engine neither requests nor polls", async () => {
    await mountAt("idle");
    await advance(10_000);
    expect(gets()).toBe(0);
  });

  test("unmounting stops the polling", async () => {
    await mountAt("running");
    cleanup();
    const before = gets();
    await advance(10_000);
    expect(gets()).toBe(before);
  });

  test("leaving idle for a loaded scenario requests the adapters", async () => {
    const m = await mountAt("idle");
    act(() => m.push(runFrame(2, "ready", 2)));
    await settle();
    expect(gets()).toBe(1);
  });
});
