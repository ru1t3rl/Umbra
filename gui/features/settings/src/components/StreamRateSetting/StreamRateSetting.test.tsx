// WI-17 (docs/plans/M8.md), written before the component. Contract:
// - `StreamRateSetting()` takes no props; it reads and writes `settings.stream.maxRateHz` (1..60, ADR 0074);
// - one ui-kit `Slider` named "Stream rate": min 1, max 60, step 1, the value shown as "<n> Hz" (also its
//   `aria-valuetext`);
// - a change writes through the backend at once (one write per effective change, nothing for an equal value);
// - the component only edits the setting: the subscribe that applies it is sent by whoever mounts the stream
//   consumer (the dashboard, the settings page).
import { afterEach, describe, expect, test } from "bun:test";
import { cleanup, fireEvent, screen } from "@testing-library/react";
import { mountSettings } from "../../testing.tsx";
import { StreamRateSetting } from "./StreamRateSetting.tsx";

afterEach(cleanup);

const slider = (): HTMLElement => screen.getByRole("slider", { name: "Stream rate" });

describe("StreamRateSetting", () => {
  test("is a slider from 1 to 60 with the default 30 Hz", () => {
    mountSettings(<StreamRateSetting />);
    expect(slider()).toHaveAttribute("min", "1");
    expect(slider()).toHaveAttribute("max", "60");
    expect(slider()).toHaveAttribute("step", "1");
    expect(slider()).toHaveValue("30");
    expect(slider()).toHaveAttribute("aria-valuetext", "30 Hz");
  });

  test("shows the stored rate", () => {
    mountSettings(<StreamRateSetting />, { stream: { maxRateHz: 12 } });
    expect(slider()).toHaveValue("12");
    expect(slider()).toHaveAttribute("aria-valuetext", "12 Hz");
  });

  test("a change is persisted through the backend and shown", () => {
    const { writes, stored } = mountSettings(<StreamRateSetting />);
    fireEvent.change(slider(), { target: { value: "45" } });
    expect(writes).toHaveLength(1);
    expect(stored()?.stream.maxRateHz).toBe(45);
    expect(slider()).toHaveValue("45");
    expect(slider()).toHaveAttribute("aria-valuetext", "45 Hz");
  });

  // Catches: a write on every render or for an unchanged value.
  test("the value already set writes nothing", () => {
    const { writes } = mountSettings(<StreamRateSetting />);
    fireEvent.change(slider(), { target: { value: "30" } });
    expect(writes).toHaveLength(0);
  });

  test("a change leaves every other setting as it was", () => {
    const { stored } = mountSettings(<StreamRateSetting />, {
      journal: { capacity: 8000 },
      scenarios: { recent: ["a.yaml"] },
    });
    fireEvent.change(slider(), { target: { value: "5" } });
    expect(stored()?.journal.capacity).toBe(8000);
    expect(stored()?.scenarios.recent).toEqual(["a.yaml"]);
  });
});

// WI-17 review m3 (ASM-995 d): a drag fires an `input` event per step and one `change` event when it ends; a
// keyboard step fires both. The value is shown while dragging and committed (one write, one subscribe for the page)
// on the gesture's end, so a drag from 10 to 40 is one write, not thirty.
describe("StreamRateSetting commits on the end of the gesture", () => {
  test("input events while dragging show the value and write nothing", () => {
    const { writes } = mountSettings(<StreamRateSetting />);
    for (const v of ["31", "32", "33", "40"]) fireEvent.input(slider(), { target: { value: v } });
    expect(writes).toHaveLength(0);
    expect(slider().getAttribute("aria-valuetext")).toBe("40 Hz");
  });

  test("the change event that ends the drag writes the final value once", () => {
    const { writes, stored } = mountSettings(<StreamRateSetting />);
    for (const v of ["31", "32", "40"]) fireEvent.input(slider(), { target: { value: v } });
    fireEvent.change(slider(), { target: { value: "40" } });
    expect(writes).toHaveLength(1);
    expect(stored()?.stream.maxRateHz).toBe(40);
  });

  test("a keyboard step (input then change) writes once", () => {
    const { writes } = mountSettings(<StreamRateSetting />);
    fireEvent.input(slider(), { target: { value: "31" } });
    fireEvent.change(slider(), { target: { value: "31" } });
    expect(writes).toHaveLength(1);
  });
});
