import { afterEach, expect, test } from "bun:test";
import { cleanup, render, screen } from "@testing-library/react";
import { Button } from "../Button/Button.tsx";
import { CommandBar } from "./CommandBar.tsx";

afterEach(cleanup);

test("a docked bar is the banner landmark with brand, title and subtitle", () => {
  render(<CommandBar title="Exercise April" subtitle="Blue vs Red" />);
  const bar = screen.getByRole("banner");
  expect(bar).toHaveClass("vt-cmdbar");
  expect(bar).not.toHaveClass("vt-cmdbar-floating");
  expect(bar.querySelector(".vt-cmdbar-brand")?.textContent).toBe("Umbra");
  expect(bar.querySelector(".vt-cmdbar-title b")?.textContent).toBe("Exercise April");
  expect(bar.querySelector(".vt-cmdbar-title span")?.textContent).toBe("Blue vs Red");
});

test("brand is replaceable", () => {
  render(<CommandBar title="T" brand="Other" />);
  expect(screen.getByRole("banner").querySelector(".vt-cmdbar-brand")?.textContent).toBe("Other");
});

test("an empty brand falls back to the product name", () => {
  render(<CommandBar title="T" brand="" />);
  expect(screen.getByRole("banner").querySelector(".vt-cmdbar-brand")?.textContent).toBe("Umbra");
});

test("subtitle is optional", () => {
  render(<CommandBar title="T" />);
  expect(screen.getByRole("banner").querySelector(".vt-cmdbar-title span")).toBeNull();
});

test("the clock is a labelled timer, live-coloured while running", () => {
  render(<CommandBar title="T" clock="14:32:07Z" />);
  const clock = screen.getByRole("timer", { name: "Scenario clock" });
  expect(clock).toHaveClass("vt-cmdbar-clock");
  expect(clock).not.toHaveClass("is-paused");
  expect(clock.textContent).toBe("14:32:07Z");
});

test("paused adds is-paused and says so in the accessible name", () => {
  render(<CommandBar title="T" clock="14:32:07Z" paused />);
  const clock = screen.getByRole("timer", { name: "Scenario clock, paused" });
  expect(clock).toHaveClass("vt-cmdbar-clock", "is-paused");
});

test("no clock element without a clock", () => {
  render(<CommandBar title="T" />);
  expect(screen.queryByRole("timer")).toBeNull();
});

test("floating adds the glass class", () => {
  render(<CommandBar title="T" floating />);
  expect(screen.getByRole("banner")).toHaveClass("vt-cmdbar", "vt-cmdbar-floating");
});

test("children render in the actions slot; className is appended", () => {
  render(
    <CommandBar title="T" className="extra">
      <Button variant="filled">Resume</Button>
    </CommandBar>,
  );
  const bar = screen.getByRole("banner");
  expect(bar).toHaveClass("extra");
  expect(bar.querySelector(".vt-cmdbar-actions")).toContainElement(
    screen.getByRole("button", { name: "Resume" }),
  );
});
