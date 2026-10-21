import { afterEach, expect, test } from "bun:test";
import { cleanup, render, screen } from "@testing-library/react";
import { Readout } from "./Readout.tsx";

afterEach(cleanup);

test("the label is the term and the value its definition", () => {
  render(<Readout label="Entities" value="1 204" />);
  expect(screen.getByRole("term")).toHaveTextContent("Entities");
  expect(screen.getByRole("term")).toHaveClass("vt-label");
  const value = screen.getByRole("definition");
  expect(value).toHaveClass("vt-readout-value");
  expect(value.textContent).toBe("1 204");
});

test("not live by default; live adds vt-readout-live", () => {
  const { container, rerender } = render(<Readout label="A" value="1" />);
  expect(container.firstElementChild).toHaveClass("vt-readout");
  expect(container.firstElementChild).not.toHaveClass("vt-readout-live");
  rerender(<Readout label="A" value="1" live className="extra" />);
  expect(container.firstElementChild).toHaveClass("vt-readout", "vt-readout-live", "extra");
});

test("the unit follows the value after a thin space", () => {
  const { container } = render(<Readout label="Rate" value="60" unit="Hz" />);
  expect(container.querySelector(".vt-readout-unit")?.textContent).toBe("Hz");
  expect(screen.getByRole("definition").textContent).toBe("60 Hz");
});

test("no unit element without a unit (also for an empty string)", () => {
  const { container } = render(<Readout label="Rate" value="60" unit="" />);
  expect(container.querySelector(".vt-readout-unit")).toBeNull();
  expect(screen.getByRole("definition").textContent).toBe("60");
});

test.each([
  ["up", "trending_up", "is-up", "Increasing"],
  ["down", "trending_down", "is-down", "Decreasing"],
  ["flat", "trending_flat", null, "Unchanged"],
] as const)("trend %s shows a glyph and a word besides the colour", (trend, glyph, cls, word) => {
  const { container } = render(<Readout label="A" value="1" delta="+3" trend={trend} />);
  const delta = container.querySelector(".vt-readout-delta");
  expect(delta).toHaveClass("vt-readout-delta");
  if (cls) expect(delta).toHaveClass(cls);
  else expect(delta).not.toHaveClass("is-up", "is-down");
  expect(delta?.querySelector(".vt-icon")?.textContent).toBe(glyph);
  expect(delta?.querySelector(".vt-visually-hidden")?.textContent?.trim()).toBe(word);
  expect(delta?.textContent).toContain("+3");
});

test("a delta without a trend has no glyph", () => {
  const { container } = render(<Readout label="A" value="1" delta="+3" />);
  expect(container.querySelector(".vt-readout-delta .vt-icon")).toBeNull();
  expect(container.querySelector(".vt-readout-delta")?.textContent).toBe("+3");
});

test("no delta element without a delta", () => {
  const { container } = render(<Readout label="A" value="1" />);
  expect(container.querySelector(".vt-readout-delta")).toBeNull();
});
