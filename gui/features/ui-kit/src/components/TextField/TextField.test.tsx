import { afterEach, expect, test } from "bun:test";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { TextField } from "./TextField.tsx";

afterEach(cleanup);

test("the label names the input and clicking the label focuses it", async () => {
  const user = userEvent.setup();
  render(<TextField label="Callsign" />);
  const input = screen.getByRole("textbox", { name: "Callsign" });
  await user.click(screen.getByText("Callsign"));
  expect(input).toHaveFocus();
  expect(screen.getByText("Callsign")).toHaveClass("vt-label");
});

test("help text describes the input without joining its name", () => {
  render(<TextField label="Speed" help="Up to 65 km/h." />);
  const input = screen.getByRole("textbox", { name: "Speed" });
  expect(input).toHaveAccessibleDescription("Up to 65 km/h.");
  expect(screen.getByText("Up to 65 km/h.")).toHaveClass("vt-help");
});

test("a caller aria-describedby is kept alongside help", () => {
  render(
    <>
      <span id="extra">Extra hint</span>
      <TextField label="Speed" help="Help." aria-describedby="extra" />
    </>,
  );
  expect(screen.getByRole("textbox")).toHaveAccessibleDescription("Extra hint Help.");
});

test("invalid sets aria-invalid and the invalid class", () => {
  const { container, rerender } = render(<TextField label="Speed" />);
  expect(screen.getByRole("textbox")).not.toHaveAttribute("aria-invalid");
  expect(container.firstElementChild).toHaveClass("vt-field");
  expect(container.firstElementChild).not.toHaveClass("vt-field-invalid");
  rerender(<TextField label="Speed" invalid />);
  expect(screen.getByRole("textbox")).toHaveAttribute("aria-invalid", "true");
  expect(container.firstElementChild).toHaveClass("vt-field", "vt-field-invalid");
});

test("mono adds vt-field-mono; className and native attributes pass through", () => {
  const { container } = render(
    <TextField label="Lat" mono className="extra" placeholder="52.0" defaultValue="51.9" />,
  );
  expect(container.firstElementChild).toHaveClass("vt-field", "vt-field-mono", "extra");
  expect(screen.getByRole("textbox")).toHaveValue("51.9");
  expect(screen.getByPlaceholderText("52.0")).toBeInTheDocument();
});

test("an explicit id is used and typing works; disabled blocks typing", async () => {
  const user = userEvent.setup();
  const { rerender } = render(<TextField label="Name" id="fixed" />);
  const input = screen.getByRole("textbox", { name: "Name" });
  expect(input.id).toBe("fixed");
  await user.type(input, "abc");
  expect(input).toHaveValue("abc");
  rerender(<TextField label="Name" id="fixed" disabled />);
  expect(screen.getByRole("textbox")).toBeDisabled();
});

test("two fields get distinct ids", () => {
  render(
    <>
      <TextField label="A" />
      <TextField label="B" />
    </>,
  );
  expect(screen.getByRole("textbox", { name: "A" }).id).not.toBe(
    screen.getByRole("textbox", { name: "B" }).id,
  );
});

test("an empty id falls back to a generated one so the label still works", () => {
  render(<TextField label="Name" id="" />);
  const input = screen.getByRole("textbox", { name: "Name" });
  expect(input.id).not.toBe("");
});

test("a caller aria-invalid passes through; invalid wins", () => {
  const { rerender } = render(<TextField label="A" aria-invalid="grammar" />);
  expect(screen.getByRole("textbox")).toHaveAttribute("aria-invalid", "grammar");
  rerender(<TextField label="A" aria-invalid="false" invalid />);
  expect(screen.getByRole("textbox")).toHaveAttribute("aria-invalid", "true");
});
