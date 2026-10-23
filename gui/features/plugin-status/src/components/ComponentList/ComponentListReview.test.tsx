// WI-13 review round (ASM-963, ASM-967): the last good registry stays under an alert; a loading line shows first.
import { afterEach, beforeEach, expect, test } from "bun:test";
import { act, cleanup, screen } from "@testing-library/react";
import {
  advance,
  COMPONENTS,
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
import { ComponentList } from "./ComponentList.tsx";

let engine: FakeEngine | undefined;
beforeEach(() => fakeTimers());
afterEach(() => {
  cleanup();
  engine?.restore();
  engine = undefined;
  restoreTimers();
});

test("a failed read on a session change shows the alert above the last components", async () => {
  let failing = false;
  engine = fakeEngine(() =>
    failing ? problemReply(500, "internal", "Engine failure", "boom") : jsonReply(COMPONENTS),
  );
  const m = mount(<ComponentList />, { state: "ready", session: 1 });
  await settle();
  failing = true;
  act(() => m.push(runFrame(2, "ready", 2)));
  await settle();
  await advance(1000);
  await advance(2000);
  const alert = screen.getByRole("alert");
  const card = screen.getByRole("heading", { name: "Sensor", level: 3 });
  expect(alert).toHaveTextContent("Engine failure");
  expect(Boolean(alert.compareDocumentPosition(card) & Node.DOCUMENT_POSITION_FOLLOWING)).toBe(
    true,
  );
});

test("a loading line shows until the registry arrives", async () => {
  engine = engineWith();
  mount(<ComponentList />, { state: "ready" });
  expect(screen.getByText("Loading...")).toBeInTheDocument();
  await settle();
  expect(screen.queryByText("Loading...")).toBeNull();
});
