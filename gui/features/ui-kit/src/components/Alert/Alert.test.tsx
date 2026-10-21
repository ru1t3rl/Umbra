import { afterEach, expect, test } from "bun:test";
import { cleanup, render, screen } from "@testing-library/react";
import { Button } from "../Button/Button.tsx";
import { Alert } from "./Alert.tsx";

afterEach(cleanup);

test.each(["danger", "warning"] as const)("%s uses role alert", (tone) => {
  render(<Alert tone={tone} title="Order rejected" />);
  expect(screen.getByRole("alert")).toHaveClass("vt-alert", `vt-alert-${tone}`);
});

test.each(["info", "success"] as const)("%s uses role status", (tone) => {
  render(<Alert tone={tone} title="Saved" />);
  expect(screen.getByRole("status")).toHaveClass("vt-alert", `vt-alert-${tone}`);
});

test("tone defaults to info", () => {
  render(<Alert title="Note" />);
  expect(screen.getByRole("status")).toHaveClass("vt-alert-info");
});

test("shows title, body and one action; the glyph is decorative", () => {
  const { container } = render(
    <Alert
      tone="danger"
      title="Order rejected"
      action={
        <Button variant="ghost" size="sm">
          Retry
        </Button>
      }
    >
      Check the target and send it again.
    </Alert>,
  );
  expect(screen.getByText("Order rejected")).toHaveClass("vt-alert-title");
  expect(screen.getByText("Check the target and send it again.")).toHaveClass("vt-alert-body");
  expect(screen.getByRole("button", { name: "Retry" })).toBeInTheDocument();
  expect(container.querySelector(".vt-alert-icon")).toHaveAttribute("aria-hidden", "true");
});

test("no body element without children", () => {
  const { container } = render(<Alert title="Saved" />);
  expect(container.querySelector(".vt-alert-body")).toBeNull();
});
