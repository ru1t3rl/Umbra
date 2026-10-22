import { afterEach, expect, spyOn, test } from "bun:test";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Button } from "./Button.tsx";

afterEach(cleanup);

test("defaults to a tonal button of type button", () => {
  render(<Button>Start</Button>);
  const b = screen.getByRole("button", { name: "Start" });
  expect(b).toHaveClass("vt-btn", "vt-btn-tonal");
  expect(b).not.toHaveClass("vt-btn-sm");
  expect(b).toHaveAttribute("type", "button");
});

test("variant, size and className map to vt-* classes", () => {
  render(
    <Button variant="filled" size="sm" className="extra">
      Go
    </Button>,
  );
  expect(screen.getByRole("button")).toHaveClass("vt-btn", "vt-btn-filled", "vt-btn-sm", "extra");
});

test("Enter and Space activate the button", async () => {
  const user = userEvent.setup();
  let clicks = 0;
  render(<Button onClick={() => clicks++}>Start</Button>);
  await user.tab();
  expect(screen.getByRole("button")).toHaveFocus();
  await user.keyboard("{Enter}");
  await user.keyboard(" ");
  expect(clicks).toBe(2);
});

test("disabled buttons ignore clicks and keys", async () => {
  const user = userEvent.setup();
  let clicks = 0;
  render(
    <Button disabled onClick={() => clicks++}>
      Start
    </Button>,
  );
  const b = screen.getByRole("button");
  expect(b).toBeDisabled();
  await user.click(b);
  await user.keyboard("{Enter}");
  expect(clicks).toBe(0);
});

test("icon plus text renders a decorative glyph and the text name", () => {
  const { container } = render(<Button icon="play_arrow">Start</Button>);
  expect(container.querySelector(".vt-icon")?.textContent).toBe("play_arrow");
  expect(screen.getByRole("button", { name: "Start" })).not.toHaveClass("vt-btn-icon");
});

test("icon-only button is named by aria-label and gets vt-btn-icon", () => {
  const warn = spyOn(console, "warn").mockImplementation(() => {});
  render(<Button icon="close" aria-label="Close" />);
  expect(screen.getByRole("button", { name: "Close" })).toHaveClass("vt-btn-icon");
  expect(warn).not.toHaveBeenCalled();
  warn.mockRestore();
});

test("icon-only button without aria-label warns in development", () => {
  const warn = spyOn(console, "warn").mockImplementation(() => {});
  // @ts-expect-error deliberately violates the type to exercise the runtime guard
  render(<Button icon="close" />);
  expect(warn).toHaveBeenCalledTimes(1);
  expect(String(warn.mock.calls[0]?.[0])).toContain("aria-label");
  warn.mockRestore();
});

test("icon-only button without aria-label does not warn in production", () => {
  const warn = spyOn(console, "warn").mockImplementation(() => {});
  const prev = process.env.NODE_ENV;
  process.env.NODE_ENV = "production";
  try {
    // @ts-expect-error deliberately violates the type to exercise the runtime guard
    render(<Button icon="close" />);
  } finally {
    if (prev === undefined) delete process.env.NODE_ENV;
    else process.env.NODE_ENV = prev;
  }
  expect(warn).not.toHaveBeenCalled();
  warn.mockRestore();
});
