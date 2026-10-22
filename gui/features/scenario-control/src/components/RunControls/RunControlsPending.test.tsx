// WI-7 additions: every lifecycle button is guarded while its own request is in flight, and "nothing sent" is
// asserted after the event loop turned (react-query sends in a later microtask, so a synchronous check proves nothing).
import { afterEach, expect, test } from "bun:test";
import { act, cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";
import {
  deferred,
  type FakeEngine,
  fakeEngine,
  mount,
  problemReply,
  runReply,
} from "../../controlsTesting.tsx";
import { hello } from "../../testing.ts";
import { ClockControl } from "../ClockControl/ClockControl.tsx";
import { RunControls } from "./RunControls.tsx";

let engine: FakeEngine | undefined;
afterEach(() => {
  cleanup();
  engine?.restore();
  engine = undefined;
});

const settle = (): Promise<void> => act(async () => new Promise((r) => setTimeout(r, 20)));

// Catches: a button whose in-flight flag is not wired (a double click would send twice).
test.each([
  ["ready", "Start"],
  ["running", "Pause"],
  ["paused", "Resume"],
  ["running", "Stop"],
  ["running", "Unload"],
] as const)("%s + %s is disabled while its request is in flight", async (state, name) => {
  const gate = deferred<Response>();
  engine = fakeEngine(() => gate.promise);
  mount(<RunControls />, { state });
  const button = screen.getByRole("button", { name }) as HTMLButtonElement;
  fireEvent.click(button);
  await waitFor(() => expect(button.disabled).toBe(true));
  fireEvent.click(button);
  await settle();
  expect(engine.calls).toHaveLength(1);
  await act(async () => gate.resolve(runReply()));
});

// Catches: the active mode chip sending a redundant setClock.
test("clicking the active clock mode sends nothing, even after the event loop turns", async () => {
  engine = fakeEngine();
  mount(<ClockControl />, { state: "running", clock: { mode: "realtime" } });
  fireEvent.click(screen.getByRole("button", { name: "Real time" }));
  await settle();
  expect(engine.calls).toHaveLength(0);
});

// Catches: Load sending a request later (the loader is WI-8).
test("Load sends nothing even after the event loop turns", async () => {
  engine = fakeEngine();
  mount(<RunControls />, { state: "idle" });
  fireEvent.click(screen.getByRole("button", { name: "Load" }));
  await settle();
  expect(engine.calls).toHaveLength(0);
});

// Catches: stale controls after the stream left "open" (connecting, reconnecting, closed, incompatible).
test.each(["reconnecting", "connecting", "closed", "incompatible"] as const)(
  "every control is disabled while the stream is %s",
  (status) => {
    engine = fakeEngine();
    const m = mount(
      <>
        <RunControls />
        <ClockControl />
      </>,
      { state: "running", clock: { mode: "stepped" } },
    );
    expect(screen.getByRole("button", { name: "Pause" })).not.toBeDisabled();
    act(() => {
      if (status === "closed") m.r.connection.close();
      else if (status === "incompatible")
        m.r.socket.onmessage?.({ data: JSON.stringify(hello("umbra.stream.v0")) });
      else {
        m.r.drop();
        if (status === "connecting") m.r.reconnect();
      }
    });
    for (const b of screen.getAllByRole("button")) expect(b).toBeDisabled();
    const pause = screen.getByRole("button", { name: "Pause" });
    const ids = (pause.getAttribute("aria-describedby") ?? "").split(" ");
    expect(document.getElementById(ids[0] ?? "")?.textContent).toBe(
      "Waiting for the engine connection.",
    );
  },
);

// Catches: ClockControl chips or Apply not guarded while their request is in flight.
test("ClockControl chips and Apply scale are guarded while a request is in flight", async () => {
  const gate = deferred<Response>();
  engine = fakeEngine(() => gate.promise);
  mount(<ClockControl />, { state: "running", clock: { mode: "realtime" } });
  const chip = screen.getByRole("button", { name: "Stepped" }) as HTMLButtonElement;
  fireEvent.click(chip);
  await waitFor(() => expect(chip.disabled).toBe(true));
  fireEvent.click(chip);
  await settle();
  expect(engine.calls).toHaveLength(1);
  await act(async () => gate.resolve(runReply()));
  cleanup();
  engine.restore();

  const gate2 = deferred<Response>();
  engine = fakeEngine(() => gate2.promise);
  mount(<ClockControl />, { state: "running", clock: { mode: "scaled", scale: 1 } });
  fireEvent.change(screen.getByRole("textbox", { name: "Scale factor" }), {
    target: { value: "4" },
  });
  const apply = screen.getByRole("button", { name: "Apply scale" }) as HTMLButtonElement;
  fireEvent.click(apply);
  await waitFor(() => expect(apply.disabled).toBe(true));
  fireEvent.click(apply);
  await settle();
  expect(engine.calls).toHaveLength(1);
  await act(async () => gate2.resolve(runReply()));
});

// Catches: a stale error left after a later step succeeds (the setProblem(null) in sendStep).
test("a failed step's alert is gone after the next step succeeds", async () => {
  let fail = true;
  engine = fakeEngine(() =>
    fail ? problemReply(409, "step_cancelled", "Step cancelled", "cancelled by pause") : runReply(),
  );
  mount(<RunControls />, { state: "running", clock: { mode: "stepped" } });
  fireEvent.click(screen.getByRole("button", { name: "Step 1 tick" }));
  await screen.findByRole("alert");
  fail = false;
  fireEvent.click(screen.getByRole("button", { name: "Step 10 ticks" }));
  await waitFor(() => expect(screen.queryByRole("alert")).toBeNull());
});

// Catches: disabled step buttons with no reason (a mute control).
test("every step button points at the one step reason while a step is pending", () => {
  engine = fakeEngine();
  mount(<RunControls />, { state: "running", clock: { mode: "stepped" }, step_pending: true });
  const group = screen.getByRole("group", { name: "Step" });
  const buttons = within(group).getAllByRole("button");
  expect(buttons).toHaveLength(5);
  const ids = new Set(buttons.map((b) => b.getAttribute("aria-describedby")));
  expect(ids.size).toBe(1);
  const id = [...ids][0];
  expect(document.getElementById(id ?? "")?.textContent).toBe("A step is already running.");
  expect(group.querySelectorAll("[hidden]")).toHaveLength(1);
});

// Catches: a reason span on enabled step buttons.
test("enabled step buttons have no description", () => {
  engine = fakeEngine();
  mount(<RunControls />, { state: "running", clock: { mode: "stepped" } });
  const group = screen.getByRole("group", { name: "Step" });
  for (const b of within(group).getAllByRole("button"))
    expect(b.getAttribute("aria-describedby")).toBeNull();
});
