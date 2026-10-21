import { afterEach, expect, test } from "bun:test";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Chip, ChipGroup } from "./Chip.tsx";

afterEach(cleanup);

test("a chip is a toggle button reflecting selected through aria-pressed", () => {
  const { rerender } = render(<Chip>Roads</Chip>);
  const chip = screen.getByRole("button", { name: "Roads" });
  expect(chip).toHaveClass("vt-chip");
  expect(chip).not.toHaveClass("vt-chip-glass");
  expect(chip).toHaveAttribute("aria-pressed", "false");
  rerender(
    <Chip selected glass>
      Roads
    </Chip>,
  );
  expect(chip).toHaveAttribute("aria-pressed", "true");
  expect(chip).toHaveClass("vt-chip", "vt-chip-glass");
});

test("Enter and Space activate a chip", async () => {
  const user = userEvent.setup();
  let clicks = 0;
  render(<Chip onClick={() => clicks++}>Roads</Chip>);
  await user.tab();
  await user.keyboard("{Enter}");
  await user.keyboard(" ");
  expect(clicks).toBe(2);
});

test("a disabled chip ignores input", async () => {
  const user = userEvent.setup();
  let clicks = 0;
  render(
    <Chip disabled onClick={() => clicks++}>
      Roads
    </Chip>,
  );
  expect(screen.getByRole("button")).toBeDisabled();
  await user.click(screen.getByRole("button"));
  expect(clicks).toBe(0);
});

test("ChipGroup is a labelled group", () => {
  render(
    <ChipGroup label="Time rate" className="extra">
      <Chip>1x</Chip>
      <Chip selected>4x</Chip>
    </ChipGroup>,
  );
  const group = screen.getByRole("group", { name: "Time rate" });
  expect(group).toHaveClass("vt-chipgroup", "extra");
  expect(group.querySelectorAll("button")).toHaveLength(2);
});

test("an icon chip renders a decorative glyph", () => {
  const { container } = render(<Chip icon="layers">Layers</Chip>);
  expect(container.querySelector(".vt-icon")?.textContent).toBe("layers");
});
