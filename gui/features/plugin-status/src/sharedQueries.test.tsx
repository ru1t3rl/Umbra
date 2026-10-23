// WI-13 review round (ASM-962): PluginList, FaultSummary and AdapterList read the same two queries. One change of
// run state or session is one extra request per endpoint, not one per reader; the poll is one request per 2 s.
import { afterEach, beforeEach, expect, test } from "bun:test";
import { act, cleanup } from "@testing-library/react";
import { AdapterList } from "./components/AdapterList/AdapterList.tsx";
import { ComponentList } from "./components/ComponentList/ComponentList.tsx";
import { FaultSummary } from "./components/FaultSummary/FaultSummary.tsx";
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

const counts = () => ({
  plugins: engine.to("GET", "/api/v1/plugins").length,
  adapters: engine.to("GET", "/api/v1/network/adapters").length,
  components: engine.to("GET", /^\/api\/v1\/components/).length,
});

async function mountAll(state: string) {
  const m = mount(
    <>
      <PluginList />
      <FaultSummary />
      <AdapterList />
      <ComponentList />
    </>,
    { state, session: 1 },
  );
  await settle();
  return m;
}

test("mounting four readers requests each endpoint once", async () => {
  await mountAll("ready");
  expect(counts()).toEqual({ plugins: 1, adapters: 1, components: 1 });
});

test("a run-state change is one more request for plugins and for adapters, none for components", async () => {
  const m = await mountAll("ready");
  act(() => m.push(runFrame(2, "paused")));
  await settle();
  expect(counts()).toEqual({ plugins: 2, adapters: 2, components: 1 });
});

test("a session change is one more request for every endpoint", async () => {
  const m = await mountAll("paused");
  act(() => m.push(runFrame(2, "paused", 2)));
  await settle();
  expect(counts()).toEqual({ plugins: 2, adapters: 2, components: 2 });
});

test("while running the adapters are polled once per 2 s however many readers there are", async () => {
  await mountAll("running");
  await advance(2000);
  expect(counts()).toEqual({ plugins: 1, adapters: 2, components: 1 });
  await advance(2000);
  expect(counts()).toEqual({ plugins: 1, adapters: 3, components: 1 });
});
