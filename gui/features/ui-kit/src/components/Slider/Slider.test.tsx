import { afterEach, expect, test } from "bun:test";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { Slider } from "./Slider.tsx";

afterEach(cleanup);

test("a range input named by the label with the min, max and step passed through", () => {
  render(<Slider label="Time rate" min={1} max={8} step={1} value={4} onChange={() => {}} />);
  const input = screen.getByRole("slider", { name: "Time rate" });
  expect(input).toHaveClass("vt-slider");
  expect(input).toHaveAttribute("type", "range");
  expect(input).toHaveAttribute("min", "1");
  expect(input).toHaveAttribute("max", "8");
  expect(input).toHaveAttribute("step", "1");
  expect(input).toHaveValue("4");
});

test("shows the formatted value, in the readout and as aria-valuetext", () => {
  const { container } = render(
    <Slider
      label="Update rate"
      min={1}
      max={60}
      value={30}
      format={(v) => `${v} Hz`}
      onChange={() => {}}
    />,
  );
  expect(container.querySelector(".vt-slider-value")?.textContent).toBe("30 Hz");
  expect(screen.getByRole("slider")).toHaveAttribute("aria-valuetext", "30 Hz");
  expect(container.querySelector(".vt-slider-wrap .vt-slider-head .vt-label")?.textContent).toBe(
    "Update rate",
  );
});

test("without format the raw value is shown", () => {
  const { container } = render(<Slider label="Opacity" value={7} onChange={() => {}} />);
  expect(container.querySelector(".vt-slider-value")?.textContent).toBe("7");
});

test("a native input change reports through onChange", () => {
  let seen = "";
  render(
    <Slider
      label="Rate"
      min={0}
      max={10}
      value={5}
      onChange={(e) => {
        seen = e.currentTarget.value;
      }}
    />,
  );
  fireEvent.change(screen.getByRole("slider"), { target: { value: "6" } });
  expect(seen).toBe("6");
});

test("disabled and className pass through", () => {
  const { container } = render(
    <Slider label="Rate" value={1} disabled className="extra" onChange={() => {}} />,
  );
  expect(screen.getByRole("slider")).toBeDisabled();
  expect(container.firstElementChild).toHaveClass("vt-slider-wrap", "extra");
});
