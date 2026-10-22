// WI-7 failing tests. Assumed API:
//   <ClockControl />  reads useRunStatus().clock/state; sends PUT /api/v1/run/clock (useSetClock) with the ClockSetting body.
//   Markup: a status line "Clock: <clockLabel>"; ChipGroup "Clock mode" of Chips "As fast as possible", "Real time",
//   "Stepped", "Scaled" (aria-pressed = current mode); only for mode scaled: Slider "Scale" (range 0..300, logarithmic),
//   TextField "Scale factor" and a Button "Apply scale". Chips click sends at once; the scale is a draft sent by Apply scale.
//   Clicking the active mode sends nothing. Disabled (with reason help) when setClock is unavailable (idle, stopped, no status).
import { afterEach, describe, expect, test } from "bun:test";
import { cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { type FakeEngine, fakeEngine, mount, problemReply } from "../../controlsTesting.tsx";
import { ClockControl } from "./ClockControl.tsx";

let engine: FakeEngine | undefined;
afterEach(() => {
  cleanup();
  engine?.restore();
  engine = undefined;
});

const chip = (name: string): HTMLButtonElement =>
  screen.getByRole("button", { name }) as HTMLButtonElement;
const CHIPS = ["As fast as possible", "Real time", "Stepped", "Scaled"] as const;

describe("mode chips", () => {
  // Catches: wrong pressed chip, or two pressed.
  test.each([
    [{ mode: "afap" }, "As fast as possible"],
    [{ mode: "realtime" }, "Real time"],
    [{ mode: "stepped" }, "Stepped"],
    [{ mode: "scaled", scale: 4 }, "Scaled"],
  ] as const)("%j presses %s only", (clock, pressed) => {
    engine = fakeEngine();
    mount(<ClockControl />, { state: "running", clock });
    const group = screen.getByRole("group", { name: "Clock mode" });
    for (const n of CHIPS) {
      expect(within(group).getByRole("button", { name: n })).toHaveAttribute(
        "aria-pressed",
        String(n === pressed),
      );
    }
  });

  // Catches: a body with the wrong mode word or a stray scale.
  test.each([
    ["As fast as possible", { mode: "afap" }],
    ["Stepped", { mode: "stepped" }],
  ] as const)("%s sends setClock %j", async (name, body) => {
    engine = fakeEngine();
    mount(<ClockControl />, { state: "running", clock: { mode: "realtime" } });
    fireEvent.click(chip(name));
    await waitFor(() => expect(engine?.calls).toHaveLength(1));
    expect(engine?.calls[0]).toEqual({ method: "PUT", path: "/api/v1/run/clock", body });
  });

  // Catches: clicking the current mode still sending a request.
  test("the active mode sends nothing", () => {
    engine = fakeEngine();
    mount(<ClockControl />, { state: "running", clock: { mode: "realtime" } });
    fireEvent.click(chip("Real time"));
    expect(engine.calls).toHaveLength(0);
  });

  // Catches: Scaled sent without a scale (contract requires one).
  test("choosing Scaled sends the scale draft, 1 by default", async () => {
    engine = fakeEngine();
    mount(<ClockControl />, { state: "running", clock: { mode: "realtime" } });
    fireEvent.click(chip("Scaled"));
    await waitFor(() => expect(engine?.calls).toHaveLength(1));
    expect(engine?.calls[0]?.body).toEqual({ mode: "scaled", scale: 1 });
  });

  // Catches: setClock enabled where the engine answers invalid_state.
  test.each([
    ["idle", true],
    ["ready", false],
    ["running", false],
    ["paused", false],
    ["stopped", true],
  ] as const)("%s: chips disabled = %p", (state, disabled) => {
    engine = fakeEngine();
    mount(<ClockControl />, { state, clock: { mode: "realtime" } });
    for (const n of CHIPS) expect(chip(n).disabled).toBe(disabled);
  });

  // Catches: live chips before the first status.
  test("disabled without a status", () => {
    engine = fakeEngine();
    mount(<ClockControl />);
    for (const n of CHIPS) expect(chip(n).disabled).toBe(true);
  });

  // Catches: a 409 swallowed.
  test("a problem shows its detail and the chips re-enable", async () => {
    engine = fakeEngine(() =>
      problemReply(409, "invalid_state", "Invalid state", "the run has stopped"),
    );
    mount(<ClockControl />, { state: "running", clock: { mode: "realtime" } });
    fireEvent.click(chip("Stepped"));
    expect(await screen.findByRole("alert")).toHaveTextContent("the run has stopped");
    expect(chip("Stepped").disabled).toBe(false);
  });

  // Catches: clockLabel not shown / label of the wrong clock.
  test("the status line shows the clock label", () => {
    engine = fakeEngine();
    const m = mount(<ClockControl />, { state: "running", clock: { mode: "scaled", scale: 4 } });
    expect(screen.getByText("Clock: Scaled ×4")).toBeInTheDocument();
    m.setRun({ clock: { mode: "afap" } });
    expect(screen.getByText("Clock: As fast as possible")).toBeInTheDocument();
  });
});

describe("scale (ASM-811)", () => {
  const scaled = (scale: number) => ({ state: "running", clock: { mode: "scaled", scale } });
  const slider = () => screen.getByRole("slider", { name: "Scale" }) as HTMLInputElement;
  const field = () => screen.getByRole("textbox", { name: "Scale factor" }) as HTMLInputElement;
  const apply = () => screen.getByRole("button", { name: "Apply scale" }) as HTMLButtonElement;

  // Catches: scale controls shown for other modes.
  test("only a scaled clock shows the scale controls", () => {
    engine = fakeEngine();
    const m = mount(<ClockControl />, { state: "running", clock: { mode: "realtime" } });
    expect(screen.queryByRole("slider")).toBeNull();
    expect(screen.queryByRole("textbox", { name: "Scale factor" })).toBeNull();
    m.setRun({ clock: { mode: "scaled", scale: 2 } });
    expect(slider()).toBeInTheDocument();
  });

  // Catches: slider and field out of sync with the engine's scale; linear slider.
  test("slider and field show the current scale on a log scale", () => {
    engine = fakeEngine();
    mount(<ClockControl />, scaled(10));
    expect(slider().min).toBe("0");
    expect(slider().max).toBe("300");
    expect(slider().value).toBe("200");
    expect(slider()).toHaveAttribute("aria-valuetext", "×10");
    expect(field().value).toBe("10");
  });

  // Catches: slider moves not reaching the draft/field, linear mapping.
  test("moving the slider updates the field and Apply sends the scale", async () => {
    engine = fakeEngine();
    mount(<ClockControl />, scaled(1));
    expect(apply().disabled).toBe(true); // draft equals the current scale
    fireEvent.change(slider(), { target: { value: "150" } });
    expect(field().value).toBe("3.16");
    expect(apply().disabled).toBe(false);
    expect(engine.calls).toHaveLength(0); // dragging sends nothing
    fireEvent.click(apply());
    await waitFor(() => expect(engine?.calls).toHaveLength(1));
    expect(engine?.calls[0]).toEqual({
      method: "PUT",
      path: "/api/v1/run/clock",
      body: { mode: "scaled", scale: 3.16 },
    });
  });

  // Catches: the numeric field limited to the slider range (contract is 0.001..1,000,000).
  test.each(["0.001", "1000000", "2500"])("the field accepts %s", async (text) => {
    engine = fakeEngine();
    mount(<ClockControl />, scaled(1));
    fireEvent.change(field(), { target: { value: text } });
    expect(field()).not.toHaveAttribute("aria-invalid", "true");
    fireEvent.click(apply());
    await waitFor(() => expect(engine?.calls).toHaveLength(1));
    expect(engine?.calls[0]?.body).toEqual({ mode: "scaled", scale: Number(text) });
  });

  // Catches: out-of-contract values reaching the server; the slider thumb leaving its range.
  test.each(["0", "0.0001", "1000001", "abc", ""])("the field refuses %p", (text) => {
    engine = fakeEngine();
    mount(<ClockControl />, scaled(1));
    fireEvent.change(field(), { target: { value: text } });
    expect(field()).toHaveAttribute("aria-invalid", "true");
    expect(apply().disabled).toBe(true);
  });

  test("a scale beyond the slider range parks the thumb at the end", () => {
    engine = fakeEngine();
    mount(<ClockControl />, scaled(5000));
    expect(slider().value).toBe("300");
    expect(field().value).toBe("5000");
  });

  // Catches: the draft overwriting a scale changed by another client or the next frame.
  test("a new scale from the engine replaces an applied draft", () => {
    engine = fakeEngine();
    const m = mount(<ClockControl />, scaled(1));
    m.setRun({ clock: { mode: "scaled", scale: 20 } });
    expect(field().value).toBe("20");
    expect(slider().value).toBe(String(Math.round(100 * (Math.log10(20) + 1))));
  });

  // Catches: unlabeled inputs (a11y).
  test("range and text inputs have accessible names and help", () => {
    engine = fakeEngine();
    mount(<ClockControl />, scaled(1));
    expect(slider()).toHaveAccessibleName("Scale");
    expect(field()).toHaveAccessibleName("Scale factor");
    expect(screen.getByText("0.001 to 1,000,000")).toBeInTheDocument();
  });
});
