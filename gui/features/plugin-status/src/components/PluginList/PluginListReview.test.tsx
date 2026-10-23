// WI-13 review round (ASM-963, ASM-967): the last good report stays under an alert; a loading line shows first.
import { afterEach, beforeEach, expect, test } from "bun:test";
import { act, cleanup, screen } from "@testing-library/react";
import {
  advance,
  engineWith,
  type FakeEngine,
  fakeEngine,
  fakeTimers,
  jsonReply,
  mount,
  PLUGINS,
  problemReply,
  restoreTimers,
  runFrame,
  settle,
} from "../../statusTesting.tsx";
import { PluginList } from "./PluginList.tsx";

let engine: FakeEngine | undefined;
beforeEach(() => fakeTimers());
afterEach(() => {
  cleanup();
  engine?.restore();
  engine = undefined;
  restoreTimers();
});

test("a failed read on a state change shows the alert above the last plugins", async () => {
  let failing = false;
  engine = fakeEngine(() =>
    failing ? problemReply(500, "internal", "Engine failure", "boom") : jsonReply(PLUGINS),
  );
  const m = mount(<PluginList />, { state: "ready" });
  await settle();
  failing = true;
  act(() => m.push(runFrame(2, "running")));
  await settle();
  await advance(1000);
  await advance(2000);
  const alert = screen.getByRole("alert");
  const card = screen.getByRole("heading", { name: "Acme DIS", level: 3 });
  expect(alert).toHaveTextContent("Engine failure");
  expect(Boolean(alert.compareDocumentPosition(card) & Node.DOCUMENT_POSITION_FOLLOWING)).toBe(
    true,
  );
});

test("a loading line shows until the report arrives", async () => {
  engine = engineWith();
  mount(<PluginList />, { state: "ready" });
  expect(screen.getByRole("status")).toHaveTextContent("Loading...");
  await settle();
  expect(screen.queryByText("Loading...")).toBeNull();
});
