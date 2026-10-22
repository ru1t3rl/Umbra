// WI-7 failing tests (skill write-frontend-tests). Assumed API:
//   <RunControls onLoadRequested?={() => void} />   reads useRunStatus() (the stream is the single truth, ADR 0045);
//   sends commands with the @umbra/data hooks useStartRun/usePauseRun/useResumeRun/useStopRun/useUnloadScenario/useStepRun;
//   no optimistic state (ASM-907). Markup: group "Run controls" with buttons Load, Start, Pause, Resume, Stop, Unload;
//   when the clock is stepped and the run is running, a group "Step" with buttons "Step 1 tick", "Step 10 ticks",
//   "Step 100 ticks", "Step 1000 ticks", a TextField "Tick count" and a button "Step".
//   A disabled button points (aria-describedby) at its disabledReason text. Errors: one Alert (danger, role alert,
//   title = problem.title, body = problem.detail) below the buttons.
import { afterEach, describe, expect, test } from "bun:test";
import { act, cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";
import {
  deferred,
  type FakeEngine,
  fakeEngine,
  mount,
  problemReply,
  runReply,
} from "../../controlsTesting.tsx";
import { RunControls } from "./RunControls.tsx";

let engine: FakeEngine | undefined;
afterEach(() => {
  cleanup();
  engine?.restore();
  engine = undefined;
});

const NAMES = ["Load", "Start", "Pause", "Resume", "Stop", "Unload"] as const;
const btn = (name: string): HTMLButtonElement =>
  screen.getByRole("button", { name }) as HTMLButtonElement;
const enabledNames = (): string[] => NAMES.filter((n) => !btn(n).disabled).sort();
const describedBy = (b: HTMLElement): string => {
  const ids = (b.getAttribute("aria-describedby") ?? "").split(" ").filter(Boolean);
  return ids.map((id) => document.getElementById(id)?.textContent ?? "").join(" ");
};

describe("buttons follow the docs/api.md state table", () => {
  // Catches: a button enabled in a state where the server answers 409 (and the reverse).
  test.each([
    ["idle", ["Load"]],
    ["ready", ["Load", "Start", "Stop", "Unload"]],
    ["running", ["Load", "Pause", "Stop", "Unload"]],
    ["paused", ["Load", "Resume", "Stop", "Unload"]],
    ["stopped", ["Load", "Unload"]],
  ] as const)("%s", (state, expected) => {
    engine = fakeEngine();
    mount(<RunControls />, { state });
    expect(enabledNames()).toEqual([...expected].sort());
  });

  // Catches: live buttons before the first frame.
  test("before any run status every button is disabled", () => {
    engine = fakeEngine();
    mount(<RunControls />);
    expect(enabledNames()).toEqual([]);
  });

  // Catches: local state that outlives the stream (buttons must follow the next frame).
  test("a new frame re-evaluates the buttons", () => {
    engine = fakeEngine();
    const m = mount(<RunControls />, { state: "ready" });
    expect(btn("Start").disabled).toBe(false);
    m.setRun({ state: "running" });
    expect(btn("Start").disabled).toBe(true);
    expect(btn("Pause").disabled).toBe(false);
  });

  // Catches: a disabled button with no explanation (a11y and ASM-809 "disabled reasons").
  test("a disabled button is described by its reason", () => {
    engine = fakeEngine();
    mount(<RunControls />, { state: "idle" });
    expect(describedBy(btn("Start"))).toBe("No scenario is loaded.");
    expect(describedBy(btn("Load"))).toBe("");
  });
});

describe("the one filled azure button (ASM-809)", () => {
  // Catches: zero or several filled buttons, or the wrong one.
  test.each([
    ["idle", "Load"],
    ["ready", "Start"],
    ["running", "Pause"],
    ["paused", "Resume"],
    ["stopped", "Load"],
  ] as const)("%s: %s", (state, primary) => {
    engine = fakeEngine();
    mount(<RunControls />, { state });
    const filled = document.querySelectorAll(".vt-btn-filled");
    expect(filled).toHaveLength(1);
    expect(filled[0]).toBe(btn(primary));
  });

  // Catches: no filled button without a status; destructive buttons not set apart.
  test("none before a status; Stop and Unload are danger", () => {
    engine = fakeEngine();
    mount(<RunControls />);
    expect(document.querySelectorAll(".vt-btn-filled")).toHaveLength(0);
    expect(btn("Stop")).toHaveClass("vt-btn-danger");
    expect(btn("Unload")).toHaveClass("vt-btn-danger");
  });
});

describe("commands", () => {
  // Catches: wrong verb or path per command.
  test.each([
    ["ready", "Start", "POST", "/api/v1/run/start"],
    ["running", "Pause", "POST", "/api/v1/run/pause"],
    ["paused", "Resume", "POST", "/api/v1/run/resume"],
    ["running", "Stop", "POST", "/api/v1/run/stop"],
    ["running", "Unload", "DELETE", "/api/v1/scenario"],
  ] as const)("%s + %s sends %s %s", async (state, name, method, path) => {
    engine = fakeEngine((c) =>
      c.method === "DELETE" ? new Response(null, { status: 204 }) : runReply(),
    );
    mount(<RunControls />, { state });
    fireEvent.click(btn(name));
    await waitFor(() => expect(engine?.calls).toHaveLength(1));
    expect(engine?.calls[0]).toEqual({ method, path, body: undefined });
  });

  // Catches: Load firing a request here (the loader is WI-8) or not calling the shell.
  test("Load calls onLoadRequested and sends nothing", () => {
    engine = fakeEngine();
    let n = 0;
    mount(<RunControls onLoadRequested={() => (n += 1)} />, { state: "idle" });
    fireEvent.click(btn("Load"));
    expect(n).toBe(1);
    expect(engine.calls).toHaveLength(0);
  });

  // Catches: an optimistic state change (the stream is the only truth, ASM-907).
  test("the buttons do not change until a frame arrives", async () => {
    engine = fakeEngine();
    const m = mount(<RunControls />, { state: "ready" });
    fireEvent.click(btn("Start"));
    await waitFor(() => expect(engine?.calls).toHaveLength(1));
    expect(btn("Start").disabled).toBe(false);
    m.setRun({ state: "running" });
    expect(btn("Start").disabled).toBe(true);
  });
});

describe("pending and errors", () => {
  // Catches: a double submit while the request is in flight.
  test("a pending command disables its button and ignores a second click", async () => {
    const gate = deferred<Response>();
    engine = fakeEngine(() => gate.promise);
    mount(<RunControls />, { state: "running" });
    fireEvent.click(btn("Pause"));
    await waitFor(() => expect(btn("Pause").disabled).toBe(true));
    fireEvent.click(btn("Pause"));
    expect(engine.calls).toHaveLength(1);
    await act(async () => gate.resolve(runReply({ state: "paused" })));
    await waitFor(() => expect(btn("Pause").disabled).toBe(false));
  });

  // Catches: the problem detail not shown, or the button stuck disabled after a 409.
  test("a 409 shows the detail in an alert and the button re-enables", async () => {
    engine = fakeEngine(() =>
      problemReply(
        409,
        "invalid_state",
        "Invalid state",
        "pause needs a running scenario; it is paused",
      ),
    );
    mount(<RunControls />, { state: "running" });
    fireEvent.click(btn("Pause"));
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("pause needs a running scenario; it is paused");
    expect(alert).toHaveClass("vt-alert-danger");
    expect(within(alert).getByText("Invalid state")).toBeInTheDocument();
    expect(btn("Pause").disabled).toBe(false);
  });

  // Catches: a stale error left on screen after a later success.
  test("the error clears when the next command starts", async () => {
    let fail = true;
    engine = fakeEngine(() =>
      fail ? problemReply(409, "invalid_state", "Invalid state", "already paused") : runReply(),
    );
    mount(<RunControls />, { state: "running" });
    fireEvent.click(btn("Pause"));
    await screen.findByRole("alert");
    fail = false;
    fireEvent.click(btn("Stop"));
    await waitFor(() => expect(screen.queryByRole("alert")).toBeNull());
  });

  // Catches: a client-side failure (engine down) producing no feedback.
  test("a network failure shows its detail", async () => {
    engine = fakeEngine();
    globalThis.fetch = (() =>
      Promise.reject(new TypeError("connection refused"))) as unknown as typeof fetch;
    mount(<RunControls />, { state: "ready" });
    fireEvent.click(btn("Start"));
    expect(await screen.findByRole("alert")).toHaveTextContent("connection refused");
  });

  // Catches: the engine error text being replaced by a generic message.
  test("no_scenario is shown with its own detail", async () => {
    engine = fakeEngine(() =>
      problemReply(409, "no_scenario", "No scenario", "no scenario is loaded"),
    );
    mount(<RunControls />, { state: "ready" });
    fireEvent.click(btn("Stop"));
    expect(await screen.findByRole("alert")).toHaveTextContent("no scenario is loaded");
  });
});

describe("step controls (ASM-810)", () => {
  const stepped = { state: "running", clock: { mode: "stepped" } };

  // Catches: step controls visible for the wrong clock or state.
  test.each([
    ["realtime running", { state: "running", clock: { mode: "realtime" } }],
    ["scaled running", { state: "running", clock: { mode: "scaled", scale: 2 } }],
    ["stepped paused", { state: "paused", clock: { mode: "stepped" } }],
    ["stepped ready", { state: "ready", clock: { mode: "stepped" } }],
  ] as const)("hidden for %s", (_name, run) => {
    engine = fakeEngine();
    mount(<RunControls />, run);
    expect(screen.queryByRole("group", { name: "Step" })).toBeNull();
  });

  // Catches: missing preset, wrong name or order.
  test("shown for a stepped running clock with the four presets", () => {
    engine = fakeEngine();
    mount(<RunControls />, stepped);
    const group = screen.getByRole("group", { name: "Step" });
    const names = within(group)
      .getAllByRole("button")
      .map((b) => b.getAttribute("aria-label") ?? b.textContent);
    expect(names).toEqual([
      "Step 1 tick",
      "Step 10 ticks",
      "Step 100 ticks",
      "Step 1000 ticks",
      "Step",
    ]);
  });

  // Catches: preset sending the wrong count or the wrong body shape.
  test("a preset sends its count", async () => {
    engine = fakeEngine(
      () =>
        new Response(JSON.stringify({ completed: true, tick: 100, run: {} }), {
          status: 200,
          headers: { "content-type": "application/json" },
        }),
    );
    mount(<RunControls />, stepped);
    fireEvent.click(btn("Step 100 ticks"));
    await waitFor(() => expect(engine?.calls).toHaveLength(1));
    expect(engine?.calls[0]).toEqual({
      method: "POST",
      path: "/api/v1/run/step",
      body: { count: 100 },
    });
  });

  // Catches: the numeric field ignored, or an invalid count sent.
  test("the numeric field sends its value and refuses an invalid one", async () => {
    engine = fakeEngine(() => runReply());
    mount(<RunControls />, stepped);
    const field = screen.getByRole("textbox", { name: "Tick count" });
    fireEvent.change(field, { target: { value: "0" } });
    expect(field).toHaveAttribute("aria-invalid", "true");
    expect(btn("Step").disabled).toBe(true);
    fireEvent.change(field, { target: { value: "1000001" } });
    expect(btn("Step").disabled).toBe(true);
    fireEvent.change(field, { target: { value: "2500" } });
    expect(field).not.toHaveAttribute("aria-invalid", "true");
    fireEvent.click(btn("Step"));
    await waitFor(() => expect(engine?.calls).toHaveLength(1));
    expect(engine?.calls[0]?.body).toEqual({ count: 2500 });
  });

  // Catches: stepping while step_in_progress would 409.
  test("every step button is disabled while a step is pending", () => {
    engine = fakeEngine();
    mount(<RunControls />, { ...stepped, step_pending: true });
    const group = screen.getByRole("group", { name: "Step" });
    for (const b of within(group).getAllByRole("button")) expect(b).toBeDisabled();
  });

  // Catches: the long-running step (reply after N ticks) allowing a second click locally.
  test("a step request in flight disables the step buttons", async () => {
    const gate = deferred<Response>();
    engine = fakeEngine(() => gate.promise);
    mount(<RunControls />, stepped);
    fireEvent.click(btn("Step 10 ticks"));
    await waitFor(() => expect(btn("Step 10 ticks").disabled).toBe(true));
    expect(btn("Step 1 tick").disabled).toBe(true);
    await act(async () => gate.resolve(runReply()));
  });

  // Catches: step_cancelled / step_in_progress swallowed.
  test("a step_cancelled problem shows its detail", async () => {
    engine = fakeEngine(() =>
      problemReply(409, "step_cancelled", "Step cancelled", "the step was cancelled by pause"),
    );
    mount(<RunControls />, stepped);
    fireEvent.click(btn("Step 1 tick"));
    expect(await screen.findByRole("alert")).toHaveTextContent("the step was cancelled by pause");
  });
});

describe("accessibility", () => {
  // Catches: unnamed groups or buttons (role/label assertions, gui/CLAUDE.md).
  test("the group and every control have accessible names", () => {
    engine = fakeEngine();
    mount(<RunControls />, { state: "running", clock: { mode: "stepped" } });
    const group = screen.getByRole("group", { name: "Run controls" });
    for (const n of NAMES)
      expect(within(group).getByRole("button", { name: n })).toBeInTheDocument();
    for (const b of screen.getAllByRole("button")) expect(b).toHaveAccessibleName();
    expect(screen.getByRole("textbox", { name: "Tick count" })).toBeInTheDocument();
  });

  // Catches: keyboard-unreachable disabled-state handling: native <button type=button>, no tabindex hacks.
  test("buttons are native and not tabindex-trapped", () => {
    engine = fakeEngine();
    mount(<RunControls />, { state: "ready" });
    for (const b of screen.getAllByRole("button")) {
      expect(b.tagName).toBe("BUTTON");
      expect(b.getAttribute("tabindex")).toBeNull();
    }
  });
});
