import { afterEach, expect, test } from "bun:test";
import { act, cleanup, render, screen } from "@testing-library/react";
import { EngineStreamProvider } from "@umbra/engine-stream";
import { Profiler } from "react";
import { entity, frame, hello, type Rig, rig } from "../../testing.ts";
import { RunStatusBar } from "./RunStatusBar.tsx";

afterEach(cleanup);

function setup(): { r: Rig; renders: () => number } {
  const r = rig();
  let n = 0;
  render(
    <EngineStreamProvider connection={r.connection}>
      <Profiler
        id="bar"
        onRender={() => {
          n += 1;
        }}
      >
        <RunStatusBar />
      </Profiler>
    </EngineStreamProvider>,
  );
  return { r, renders: () => n };
}

const value = (label: string): HTMLElement => {
  const dd = screen.getByText(label).closest("dl")?.querySelector(".vt-readout-value");
  if (!(dd instanceof HTMLElement)) throw new Error(`no readout ${label}`);
  return dd;
};
const readout = (label: string): HTMLElement => {
  const dl = screen.getByText(label).closest("dl");
  if (!(dl instanceof HTMLElement)) throw new Error(`no readout ${label}`);
  return dl;
};

test("before any run: placeholders, no badge, nothing live", () => {
  setup();
  expect(readout("Sim time")).not.toHaveClass("vt-readout-live");
  expect(value("Sim time")).toHaveTextContent("—");
  expect(value("Tick")).toHaveTextContent("—");
  expect(value("Entities")).toHaveTextContent("—");
  expect(document.querySelector(".vt-badge")).toBeNull();
});

test("shows sim time, tick, entity count and the run badge", () => {
  const { r } = setup();
  act(() => r.push(hello(), frame(0, 3600, [entity(1), entity(2)])));
  expect(value("Sim time")).toHaveTextContent("T+00:02:00");
  expect(value("Tick")).toHaveTextContent("3600");
  expect(value("Entities")).toHaveTextContent("2");
  expect(screen.getByText("Running")).toHaveClass("vt-badge-success");
});

test("count changes and run changes update independently", () => {
  const { r } = setup();
  act(() => r.push(hello(), frame(0, 1, [entity(1)])));
  act(() => r.push(frame(1, 1, [entity(1), entity(2), entity(3)])));
  expect(value("Entities")).toHaveTextContent("3");
  expect(value("Tick")).toHaveTextContent("1");
  act(() => r.push(frame(2, 2, [entity(1), entity(2), entity(3)])));
  expect(value("Tick")).toHaveTextContent("2");
  expect(value("Entities")).toHaveTextContent("3");
});

test("the sim time is live only while running", () => {
  const { r } = setup();
  act(() => r.push(hello(), frame(0, 1)));
  expect(readout("Sim time")).toHaveClass("vt-readout-live");
  for (const [state, word] of [
    ["paused", "Paused"],
    ["ready", "Ready"],
    ["idle", "Idle"],
    ["stopped", "Stopped"],
    ["mystery", "Unknown"],
  ] as const) {
    act(() => r.push(frame(10, 2, [], { state })));
    expect(readout("Sim time")).not.toHaveClass("vt-readout-live");
    expect(screen.getByText(word)).toBeInTheDocument();
  }
  act(() => r.push(frame(11, 3)));
  expect(readout("Sim time")).toHaveClass("vt-readout-live");
});

test("the sim time is live only while the stream is open", () => {
  const { r } = setup();
  act(() => r.push(hello(), frame(0, 1)));
  expect(readout("Sim time")).toHaveClass("vt-readout-live");
  act(() => r.drop());
  expect(readout("Sim time")).not.toHaveClass("vt-readout-live");
  expect(screen.getByText("Running")).toBeInTheDocument();
  act(() => r.reconnect());
  act(() => r.push(hello(), frame(1, 2)));
  expect(readout("Sim time")).toHaveClass("vt-readout-live");
});

test("the run state is the only live region; readouts are not", () => {
  const { r } = setup();
  act(() => r.push(hello(), frame(0, 1, [entity(1)])));
  const status = screen.getByRole("status");
  expect(status).toHaveTextContent("Run state: Running");
  for (const label of ["Sim time", "Tick", "Entities"]) {
    expect(readout(label).closest("[role=status],[role=alert],[aria-live]")).toBeNull();
    expect(status.contains(readout(label))).toBe(false);
  }
});

test("stopped is danger after a fault, an error, an unknown or a future reason", () => {
  const { r } = setup();
  act(() => r.push(hello()));
  let seq = 0;
  for (const reason of ["plugin_fault", "error", "network_fault"]) {
    seq += 1;
    act(() => r.push(frame(seq, seq, [], { state: "stopped", stop_reason: reason })));
    expect(screen.getByText("Stopped")).toHaveClass("vt-badge-danger");
  }
  const fault = { call: "tick", capability: "c", plugin: "p", message: "boom" };
  seq += 1;
  act(() => r.push(frame(seq, seq, [], { state: "stopped", stop_reason: "duration", fault })));
  expect(screen.getByText("Stopped")).toHaveClass("vt-badge-danger");
});

test("stopped is danger after a plugin fault and neutral after a command", () => {
  const { r } = setup();
  act(() => r.push(hello(), frame(0, 1, [], { state: "stopped", stop_reason: "plugin_fault" })));
  expect(screen.getByText("Stopped")).toHaveClass("vt-badge-danger");
  act(() => r.push(frame(1, 2, [], { state: "stopped", stop_reason: "command" })));
  expect(screen.getByText("Stopped")).not.toHaveClass("vt-badge-danger");
});

test("an unknown state and clock still show tick and time", () => {
  const { r } = setup();
  act(() => r.push(hello(), frame(0, 900, [], { state: "hibernating", clock: { mode: "warp" } })));
  expect(screen.getByText("Unknown")).toBeInTheDocument();
  expect(value("Sim time")).toHaveTextContent("T+00:00:30");
  expect(value("Tick")).toHaveTextContent("900");
});

test("no scenario loaded: null tick and time are placeholders", () => {
  const { r } = setup();
  act(() => r.push(hello(), frame(0, 0, [], { state: "idle", tick: null, time_s: null })));
  expect(value("Sim time")).toHaveTextContent("—");
  expect(value("Tick")).toHaveTextContent("—");
  expect(screen.getByText("Idle")).toBeInTheDocument();
});

test("100 frames with an unchanged run and count render nothing", () => {
  const { r, renders } = setup();
  const entities = [entity(1), entity(2)];
  act(() => r.push(hello(), frame(0, 5, entities)));
  const before = renders();
  for (let seq = 1; seq <= 100; seq += 1) act(() => r.push(frame(seq, 5, entities)));
  expect(renders()).toBe(before);
  act(() => r.push(frame(101, 6, entities)));
  expect(renders()).toBe(before + 1);
});

test("a frame that only changes the count renders the count alone once", () => {
  const { r, renders } = setup();
  act(() => r.push(hello(), frame(0, 5, [entity(1)])));
  const before = renders();
  act(() => r.push(frame(1, 5, [entity(1), entity(2)])));
  expect(renders()).toBe(before + 1);
});
