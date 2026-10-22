import { afterEach, expect, test } from "bun:test";
import { cleanup, render, screen } from "@testing-library/react";
import { Card } from "./Card.tsx";

afterEach(cleanup);

test("default level 2 adds no level class", () => {
  const { container } = render(<Card>x</Card>);
  expect((container.firstElementChild as HTMLElement).className).toBe("vt-card");
});

test.each([
  [1, "vt-card-1"],
  [3, "vt-card-3"],
] as const)("level %i maps to %s", (level, cls) => {
  const { container } = render(<Card level={level}>x</Card>);
  expect(container.firstElementChild).toHaveClass("vt-card", cls);
});

test("outline variant", () => {
  const { container } = render(<Card variant="outline">x</Card>);
  expect(container.firstElementChild).toHaveClass("vt-card", "vt-card-outline");
});

test("title is a heading, subtitle a caption; both optional", () => {
  const { container, rerender } = render(
    <Card title="2 MECH BDE" subtitle="Blue force">
      body
    </Card>,
  );
  expect(screen.getByRole("heading", { name: "2 MECH BDE", level: 3 })).toHaveClass(
    "vt-card-title",
  );
  expect(screen.getByText("Blue force")).toHaveClass("vt-card-sub");
  rerender(<Card>body</Card>);
  expect(container.querySelector(".vt-card-title")).toBeNull();
  expect(container.querySelector(".vt-card-sub")).toBeNull();
});
