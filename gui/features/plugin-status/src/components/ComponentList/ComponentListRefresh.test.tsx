// WI-13 ComponentList refetching (ASM-955, ASM-947 d): the registry changes only with a session (a new scenario
// may bring plugins), so it is read again on a session change and not on a run-state change; never polled.
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
import { ComponentList } from "./ComponentList.tsx";

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

const gets = () => engine.to("GET", /^\/api\/v1\/components/).length;

async function mountAt(state: string, session = 1) {
  const m = mount(<ComponentList />, { state, session });
  await settle();
  return m;
}

describe("ComponentList refetching", () => {
  test("mounting requests the registry once", async () => {
    await mountAt("ready");
    expect(gets()).toBe(1);
  });

  test("a session change requests it again, once", async () => {
    const m = await mountAt("ready", 1);
    act(() => m.push(runFrame(2, "ready", 2)));
    await settle();
    expect(gets()).toBe(2);
  });

  test("a run-state change does not", async () => {
    const m = await mountAt("ready");
    act(() => m.push(runFrame(2, "running")));
    act(() => m.push(runFrame(3, "paused")));
    await settle();
    expect(gets()).toBe(1);
  });

  test("never polls, not even while running", async () => {
    await mountAt("running");
    await advance(30_000);
    expect(gets()).toBe(1);
  });
});
