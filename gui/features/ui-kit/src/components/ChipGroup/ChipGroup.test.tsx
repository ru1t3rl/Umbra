import { afterEach, expect, test } from "bun:test";
import { cleanup, render, screen } from "@testing-library/react";
import { Chip } from "../Chip/Chip.tsx";
import { ChipGroup } from "./ChipGroup.tsx";

afterEach(cleanup);

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
